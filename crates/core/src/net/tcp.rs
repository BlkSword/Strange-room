//! TCP 回退通道。
//!
//! QUIC 走 UDP：干净、快、自带加密与多路复用。但企业网络、访客 WiFi、
//! 部分 VPN 会直接把 UDP 封掉——没有回退路径时，产品在这些网络里就是
//! "连不上"，而用户看到的只是一句笼统的超时。
//!
//! # 设计：一条 TCP 连接 = QUIC 的一条双向流（一个方向）
//!
//! 房间的两个方向各用一条 TCP 连接（lane 0 / lane 1），**帧协议完全不变**，
//! 所以物品收发的代码一行都不用改；变的只有会话层"选哪条通道"。
//!
//! 为什么不自己做多路复用：两条 TCP 连接已经把"两个方向"表达清楚了，
//! 再套一层 mux 只会多一处能出错的地方。代价是握手时多一次 TCP+TLS 连接
//! （局域网里是几毫秒），换来的是与 QUIC 路径完全同构的代码。
//!
//! # 与 QUIC 的差异（都是刻意的）
//!
//! - **没有 12 秒空闲超时**：TCP 有 RST/FIN，对端进程被杀时读会立刻返回；
//!   整台机器消失则由 TCP keepalive 兜底（15s 空闲 + 5s 间隔）。
//! - **不能套 500ms 轮询超时**：那会在读到一半时把帧撕开，后面的字节全部错位。
//!   取消靠 `CancelToken::cancelled()` 的异步等待（取消即整条会话作废）。

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::{Error, Result};
use crate::identity::{Identity, SERVER_NAME};
use crate::net::tls;

/// TCP 回退通道的 TLS 握手上限。局域网里握手是毫秒级；超过这个数基本就是
/// 对方已经不服务了（端口还开着），或者中间有设备在吞包。
const TCP_HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 一条"车道"：TCP 回退下的双向字节流。
///
/// 用 `Box<dyn ...>` 而不是泛型，是因为服务端和客户端的 TLS 流是不同类型，
/// 而它们在下游（收发物品）完全一样——下游本来就只要求 `AsyncRead/AsyncWrite`。
pub struct Lane {
    pub send: Box<dyn AsyncWrite + Unpin + Send>,
    pub recv: Box<dyn AsyncRead + Unpin + Send>,
    /// 对端地址，只用于日志与错误提示
    pub peer: SocketAddr,
}

impl std::fmt::Debug for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lane").field("peer", &self.peer).finish()
    }
}

/// 主机侧的 TCP 监听器。
pub struct TcpHost {
    listener: TcpListener,
    acceptor: TlsAcceptor,
    port: u16,
}

impl TcpHost {
    /// 绑定端口。传 0 表示让系统挑一个空闲端口（`port()` 能拿回来）。
    pub async fn bind(port: u16, identity: &Identity) -> Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await.map_err(|e| {
            Error::protocol(format!("TCP 回退端口 {port} 绑定失败：{e}"))
        })?;
        let real_port = listener
            .local_addr()
            .map_err(|e| Error::protocol(format!("读取 TCP 回退端口失败：{e}")))?
            .port();
        let cfg = tls::server_config(identity)?;
        Ok(Self {
            listener,
            acceptor: TlsAcceptor::from(Arc::new(cfg)),
            port: real_port,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// 接一条车道：TCP accept + TLS 握手都在这里完成。
    pub async fn accept(&self) -> Result<Lane> {
        let (stream, peer) = self
            .listener
            .accept()
            .await
            .map_err(|e| Error::protocol(format!("TCP 回退通道接受连接失败：{e}")))?;
        tune(&stream);
        let tls = self
            .acceptor
            .accept(stream)
            .await
            .map_err(|e| Error::protocol(format!("TCP 回退通道的 TLS 握手失败（{peer}）：{e}")))?;
        Ok(split(tls, peer))
    }
}

/// 客户端：连一条车道。
///
/// 返回的第二个值是"指纹不符"的信号（和 QUIC 路径同一个校验器）：
/// 调用方据此把**安全问题和网络问题分开报**，绝不能靠重试掩盖。
pub async fn dial(
    addr: SocketAddr,
    fingerprint: &str,
) -> Result<(Lane, Arc<std::sync::atomic::AtomicBool>)> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| Error::protocol(format!("TCP 连接 {addr} 失败：{}", friendly(&e))))?;
    tune(&stream);

    let (cfg, flag) = tls::client_config(fingerprint)?;
    let connector = TlsConnector::from(Arc::new(cfg));
    let name = ServerName::try_from(SERVER_NAME.to_string())
        .map_err(|e| Error::protocol(format!("TLS 服务器名无效：{e}")))?;
    // 握手也要有上限：对端可能已经停止分享，端口却还开着（内核 backlog），
    // 或者被中间设备吞掉——没有超时的话这里会一直挂着，用户看到的是"卡住"。
    let tls = match tokio::time::timeout(TCP_HANDSHAKE_TIMEOUT, connector.connect(name, stream)).await {
        Ok(r) => r.map_err(|e| {
            Error::protocol(format!(
                "TCP 连接 {addr} 的加密握手失败：{}。最常见的原因是二维码已过期（对方重启过或换了会话）",
                friendly(&e)
            ))
        })?,
        Err(_) => {
            return Err(Error::protocol(format!(
                "TCP 连接 {addr} 的加密握手超时（{} 秒）。对方可能已经停止了分享",
                TCP_HANDSHAKE_TIMEOUT.as_secs()
            )))
        }
    };
    Ok((split(tls, addr), flag))
}

/// 把 TCP 连接的选项调到适合传大文件的状态。
fn tune(stream: &TcpStream) {
    // 关掉 Nagle：我们每次写的都是整帧，攒包只会增加延迟
    let _ = stream.set_nodelay(true);

    // 死连接检测。对端进程被强杀时内核会给出 FIN/RST，读会立刻返回；
    // 但对端整台机器消失（拔网线、断电）时不会有任何通知——keepalive
    // 让内核在几十秒内把连接判死，而不是永远挂着。
    // QUIC 那边对应的是 12 秒空闲超时。
    let sock = socket2::SockRef::from(stream);
    let ka = socket2::TcpKeepalive::new()
        .with_time(std::time::Duration::from_secs(15))
        .with_interval(std::time::Duration::from_secs(5));
    let _ = sock.set_tcp_keepalive(&ka);
}

fn split<S>(stream: S, peer: SocketAddr) -> Lane
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (recv, send) = tokio::io::split(stream);
    Lane {
        send: Box::new(send),
        recv: Box::new(recv),
        peer,
    }
}

/// 把 TCP/IO 错误翻译成人能看懂、能行动的话。
fn friendly(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => {
            "对方端口拒绝连接（可能对方是很老的版本、只支持 UDP，或 TCP 被防火墙拦了）".to_string()
        }
        std::io::ErrorKind::TimedOut => "连接超时".to_string(),
        std::io::ErrorKind::PermissionDenied => "被本机安全策略拒绝（防火墙/杀毒软件）".to_string(),
        _ => e.to_string(),
    }
}
