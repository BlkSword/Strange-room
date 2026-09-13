//! QUIC 传输与会话驱动。
//!
//! 角色约定（很重要，别搞反）：
//! - **主机（host）**：想分享文件的人。开监听、显示二维码、发送文件。
//! - **接收端（receiver）**：想拿文件的人。主动连接主机、接收文件。
//!
//! 所以"主动连接的一方是接收方"。协议里 receiver 先发 ClientHello。
//!
//! v1 用**单条双向流**跑完整会话：握手、清单、逐文件协商、数据块全部复用
//! 它。这样最不容易出错；QUIC 的多流并行留给 v1.5（协议已经预留了形态）。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};


use crate::cancel::CancelToken;
use crate::error::{Error, Result};
use crate::fs_util;
use crate::progress::{ProgressEvent, ProgressSender};
use crate::protocol::*;
use crate::qr::{AddressHint, QrPayload};
use crate::transfer::plan::{ItemKind, TransferPlan};
use crate::transfer::resume::{PartialFile, ResumeState, RESUME_FILE};

use super::tls;

/// 单次握手超时。QUIC 在证书被拒时会静默重试，没有这个超时用户只会看到"卡住"。
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// QUIC 空闲超时。
///
/// 为什么需要显式设置：接收端进程被强杀时**不会**发送连接关闭帧，主机侧只能
/// 靠空闲超时才发现对端已经不在。默认值通常 30 秒，意味着主机要等半分钟才能
/// 回到 `accept()`，期间接收端重连续传根本连不上。
/// 局域网内数据是连续流动的，十几秒没有任何流量即可判定对端已消失。
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

/// UDP 套接字的收发缓冲区大小。
///
/// **为什么必须显式设置**：Windows 的 UDP 默认缓冲区只有 64 KB，而 QUIC 允许的
/// 在途数据量是流窗口（默认 1.25 MB）级别。缓冲区一满，内核**直接丢包**，
/// 于是进入"丢包 → 拥塞窗口缩回 → 重传"的循环。实测：即使在毫无真实损耗的
/// 回环上，默认缓冲区下也有 1.8% 的丢包、17 次拥塞事件，吞吐被死死按在
/// 160 MB/s；丢包还会在接收端堆积出大量碎片，超过 quinn 的 1024 段上限后
/// 连接会被判为 INTERNAL_ERROR 直接掐断（"too many gaps in stream buffer"）。
///
/// 4 MB 是折中：足够覆盖几兆的在途数据，又不会让每台设备付出太大内存。
/// 高带宽链路（万兆局域网）下如果还不够，可以再调大。
const UDP_SOCKET_BUFFER: usize = 4 * 1024 * 1024;

/// 绑定一个收发缓冲区已调大的 UDP 套接字。
///
/// 与 `quinn::Endpoint::client` 内部做的事保持一致（IPv6 用双栈），只是多设了
/// 缓冲区。设缓冲区失败不算致命：退回系统默认值，功能正常，只是慢一些。
fn bind_udp(addr: SocketAddr) -> std::io::Result<std::net::UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    if addr.is_ipv6() {
        let _ = socket.set_only_v6(false);
    }
    let _ = socket.set_recv_buffer_size(UDP_SOCKET_BUFFER);
    let _ = socket.set_send_buffer_size(UDP_SOCKET_BUFFER);
    socket.bind(&addr.into())?;
    Ok(socket.into())
}

/// 构造带空闲超时的传输配置，主机与接收端共用。
///
/// 只调空闲超时，**不动流控窗口**：默认窗口已经足够局域网吞吐，
/// 而手动调窗口一旦与环境不匹配，就会出现"传到一半连接莫名消失"这种
/// 极难定位的问题。少改一个参数，就少一类这种问题。
fn transport_config() -> Arc<quinn::TransportConfig> {
    let mut tc = quinn::TransportConfig::default();
    tc.max_idle_timeout(Some(
        IDLE_TIMEOUT
            .try_into()
            .expect("空闲超时必须能转换成 quinn 的 IdleTimeout"),
    ));
    Arc::new(tc)
}

/// 本机所有可用的局域网地址。二维码里会带上全部，让接收端挨个试。
///
/// v1 刻意不做 mDNS：地址直接在二维码里，发现不是必要条件。
/// 用 UDP connect 探测默认出口网卡，不会真的发包。
pub fn local_address_hints(port: u16) -> Vec<AddressHint> {
    let mut hints: Vec<AddressHint> = Vec::new();

    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = sock.local_addr() {
                let ip = local.ip();
                if !ip.is_loopback() {
                    hints.push(AddressHint {
                        host: ip.to_string(),
                        port,
                    });
                }
            }
        }
    }

    // 兜底：至少给一个回环地址，同一台机器上测试时能连
    if hints.is_empty() {
        hints.push(AddressHint {
            host: "127.0.0.1".to_string(),
            port,
        });
    }
    hints
}

pub struct HostOptions {
    pub plan: TransferPlan,
    pub device_name: String,
    pub listen_port: u16,
    /// 会话 ID。为 None 时随机生成。
    pub session_id: Option<String>,
    /// 只接受这一次连接就结束（v1 的 CLI 语义）。
    pub once: bool,
    /// 对方往房间里放东西时，收进这个目录（None = 不接受对方放东西）。
    ///
    /// 这是第二级「双向共享空间」的开关：房间不该只是我单向往外掏东西。
    pub incoming_dir: Option<PathBuf>,
}

pub struct HostSession {
    endpoint: quinn::Endpoint,
    identity: crate::identity::Identity,
    /// 会话开始时确定的文件清单。整个会话期间不变——这是"发送前先算好
    /// 全部哈希"这个决定带来的直接好处：传输期间不需要再回头读盘。
    plan: TransferPlan,
    device_name: String,
    /// 对方放东西的落点（见 HostOptions::incoming_dir）
    incoming_dir: Option<PathBuf>,
    pub session_id: String,
    pub port: u16,
}

impl HostSession {
    /// 启动监听。返回后即可展示二维码。
    pub async fn start(opts: HostOptions) -> Result<Self> {
        tls::ensure_crypto_provider();
        let identity = crate::identity::Identity::load_or_create(&crate::identity::Identity::default_dir())?;

        let session_id = opts
            .session_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let server_cfg = tls::server_config(&identity)?;
        let mut quinn_cfg = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_cfg)
                .map_err(|e| Error::protocol(format!("QUIC 服务端配置失败: {e}")))?,
        ));
        quinn_cfg.transport_config(transport_config());

        let bind: SocketAddr = format!("0.0.0.0:{}", opts.listen_port).parse().unwrap();
        let sock = bind_udp(bind)
            .map_err(|e| Error::protocol(format!("无法监听 {bind}（{e}）。请检查端口是否被占用")))?;
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(quinn_cfg),
            sock,
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| Error::protocol(format!("无法监听 {bind}（{e}）。请检查端口是否被占用")))?;
        let port = endpoint
            .local_addr()
            .map_err(|e| Error::protocol(format!("获取本地端口失败: {e}")))?
            .port();

        Ok(Self {
            endpoint,
            identity,
            incoming_dir: opts.incoming_dir.clone(),
            plan: opts.plan,
            device_name: opts.device_name,
            session_id,
            port,
        })
    }

    pub fn fingerprint(&self) -> &str {
        &self.identity.fingerprint
    }

    /// 本机设备名，用于二维码和握手。
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// 本次会话要分享的清单（只读）。
    pub fn plan(&self) -> &TransferPlan {
        &self.plan
    }

    pub fn qr_payload(&self) -> Result<QrPayload> {
        Ok(QrPayload::new(
            self.session_id.clone(),
            self.device_name.clone(),
            self.identity.fingerprint.clone(),
            local_address_hints(self.port),
        ))
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }

    /// 等待一次连接并完成传输。返回本次传输的结果摘要。
    pub async fn accept_once(&self, progress: &ProgressSender) -> Result<TransferSummary> {
        // 循环直到拿到一个**握手成功**的连接。
        //
        // 关键点：握手失败（例如接收端刚连上就被强杀、或对方中途放弃）是正常现象，
        // 不是主机的故障。这类错误绝不能冒泡出去让调用方停止服务——否则一个坏掉的
        // 客户端就能让主机再也不接受任何连接，而用户完全不知道为什么"连不上了"。
        loop {
            let incoming = self
                .endpoint
                .accept()
                .await
                .ok_or_else(|| Error::protocol("监听已关闭"))?;

            match incoming.await {
                Ok(conn) => {
                    return serve_connection(
                        conn,
                        &self.session_id,
                        &self.plan,
                        &self.device_name,
                        self.incoming_dir.as_deref(),
                        progress,
                    )
                    .await;
                }
                Err(e) => {
                    eprintln!("[主机] 忽略一次未完成的握手（对端可能刚连上就退出了）：{e}");
                    continue;
                }
            }
        }
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"done");
    }
}

#[derive(Debug, Default, Clone)]
pub struct TransferSummary {
    /// 本次会话处理过的文件数（发出去的 + 从对方收到的）。
    /// 单向传输时它就是"成功传完的文件数"。
    pub files_sent: usize,
    /// 其中**从对方那里收到**的文件数（第二级：双向共享空间）。
    /// 主机用它区分"我发出去的"和"对方放进来的"。
    pub received_files: usize,
    pub received_bytes: u64,
    /// 收到/发出的文本条目：（来源说明，内容）。
    /// 只在接收端填充——文本的落点在界面，不在磁盘上。
    pub texts: Vec<(String, String)>,
    pub bytes_sent: u64,
    pub failures: Vec<(String, String)>,
}

/// 主机侧：处理一条连接上的完整会话。
/// 拒绝握手：把原因发给对端，并**等对端读完**再关闭连接。
///
/// 不能在 `send.finish()` 后立刻 drop 连接——那样错误帧可能还没送达，
/// 用户看到的就是泛化的"连接超时"，而不是"二维码已过期"。
/// 这类错误提示的可操作性直接决定首次体验的成败，值得多等一小会儿。
async fn reject_handshake(
    conn: quinn::Connection,
    mut send: quinn::SendStream,
    msg: String,
) -> Result<TransferSummary> {
    if let Ok(frame) = Frame::json(KIND_ERROR, &ErrorMsg { message: msg.clone() }) {
        let _ = write_frame(&mut send, &frame).await;
    }
    let _ = send.finish();
    // 给对端一点时间把错误读走，但绝不无限等
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), conn.closed()).await;
    Err(Error::protocol(msg))
}

async fn serve_connection(
    conn: quinn::Connection,
    session_id: &str,
    plan: &TransferPlan,
    device_name: &str,
    incoming_dir: Option<&Path>,
    progress: &ProgressSender,
) -> Result<TransferSummary> {
    let (mut send, mut recv) = conn
        .accept_bi()
        .await
        .map_err(|e| Error::protocol(format!("接收端未能建立数据流: {e}")))?;

    // ---- 1. 握手 ----
    let frame = read_frame(&mut recv)
        .await?
        .ok_or_else(|| Error::protocol("接收端在握手前就断开了"))?;
    let hello: ClientHello = frame.decode_json()?;
    if hello.protocol_version != PROTOCOL_VERSION {
        let msg = format!(
            "对端协议版本 {} 与本机 {PROTOCOL_VERSION} 不匹配，请双方使用同一版本",
            hello.protocol_version
        );
        return reject_handshake(conn, send, msg).await;
    }
    if hello.session_id != session_id {
        let msg = "二维码已过期（主机已经切换到新的会话），请重新扫描屏幕上的二维码".to_string();
        return reject_handshake(conn, send, msg).await;
    }

    progress.send(ProgressEvent::SessionStarted {
        peer: hello.device_name.clone(),
        total_files: plan.files.len(),
        total_bytes: plan.total_bytes,
    });

    let server_hello = ServerHello {
        protocol_version: PROTOCOL_VERSION,
        session_id: session_id.to_string(),
        device_name: device_name.to_string(),
        max_chunk_size: DEFAULT_CHUNK_SIZE,
        accepts_incoming: incoming_dir.is_some(),
    };
    write_frame(&mut send, &Frame::json(KIND_HELLO, &server_hello)?).await?;

    // ---- 2. 清单 ----
    let entries: Vec<FileEntry> = plan
        .files
        .iter()
        .map(|f| FileEntry {
            file_id: f.file_id.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
            blake3: f.blake3.clone(),
            kind: f.kind,
        })
        .collect();
    let manifest = FileManifest {
        files: entries,
        total_bytes: plan.total_bytes,
    };
    write_frame(&mut send, &Frame::json(KIND_MANIFEST, &manifest)?).await?;

    let mut summary = TransferSummary::default();

    // ---- 3. 两条流：A 用来送（我方清单），B 用来取（对方清单）----
    //
    // 与接收端对称：A 上对方取我们的东西，B 上我们取对方的东西。两条流**并发**跑，
    // 所以"我还在发大文件"的时候，对方也能把它的东西塞过来——不用排队。
    let mut sent = TransferSummary::default();
    let mut fetched = TransferSummary::default();

    let serve = serve_items(&conn, &mut send, &mut recv, plan, progress, &mut sent);
    let fetch = async {
        // 对端可能只开一条流（老版本，或这次没有东西要放）。版本号已经跟着涨了，
        // 这里等一小会儿只是为了：没有第二条流时不要空等到 QUIC 空闲超时。
        let accepted =
            tokio::time::timeout(std::time::Duration::from_secs(3), conn.accept_bi()).await;
        let (mut send_b, mut recv_b) = match accepted {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => {
                return Err(Error::protocol(format!("第二条数据流建立失败: {e}")));
            }
            // 对方没开第二条流：这次只有单向，正常收尾
            Err(_) => return Ok(()),
        };

        let frame = read_frame(&mut recv_b)
            .await?
            .ok_or_else(|| Error::protocol("对方在发送清单前就断开了"))?;
        frame.expect_kind(KIND_MANIFEST, "对方清单")?;
        let theirs: FileManifest = frame.decode_json()?;

        if !theirs.files.is_empty() {
            let Some(dir) = incoming_dir else {
                // 没指定接收目录时明确报错，而不是默默把对方的东西丢掉
                let msg =
                    "对方想往这里放东西，但这次分享没有指定接收目录（启动时用 --to 指定）"
                        .to_string();
                let _ = write_frame(
                    &mut send_b,
                    &Frame::json(KIND_ERROR, &ErrorMsg { message: msg.clone() })?,
                )
                .await;
                return Err(Error::protocol(msg));
            };
            // 主机这边没有协作式取消（取消走的是关连接），给一个不会被触发的
            let cancel = CancelToken::new();
            pull_items(
                &conn,
                &mut send_b,
                &mut recv_b,
                &theirs.files,
                dir,
                session_id,
                true,
                DEFAULT_CHUNK_SIZE,
                progress,
                &cancel,
                &mut fetched,
            )
            .await?;

            // 收完把续传状态文件删掉：它只是"下次能少传一点"的辅助信息，
            // 留在对方的目录里就是一道痕迹，和"不留痕"的承诺不符。
            let _ = std::fs::remove_file(dir.join(RESUME_FILE));
        }

        // 告诉对方"这个方向我取完了"：它的发送循环靠这句话收尾
        let _ = write_frame(
            &mut send_b,
            &Frame::json(
                KIND_BYE,
                &Bye {
                    reason: Some("room items taken".into()),
                },
            )?,
        )
        .await;
        let _ = send_b.finish();
        Ok(())
    };

    // join 而不是 try_join：一个方向出错不该把另一个方向掐掉
    let (serve_result, fetch_result) = tokio::join!(serve, fetch);
    merge_summary(&mut summary, sent);
    merge_summary(&mut summary, fetched);
    serve_result?;
    fetch_result?;
    let _ = write_frame(
        &mut send,
        &Frame::json(
            KIND_BYE,
            &Bye {
                reason: Some("transfer complete".into()),
            },
        )?,
    )
    .await;
    let _ = send.finish();

    // 文本条目单独计数：界面上"0 个文件"和"1 段文本"是两件事
    let texts = summary.texts.len();

    progress.send(ProgressEvent::SessionFinished {
        files: summary.files_sent,
        texts,
        bytes: summary.bytes_sent,
    });

    // 收尾：明确关闭连接，并且**不等** `conn.closed()` 就返回。
    //
    // 这里的"等"是要害：连接不会自己立刻结束，`conn.closed()` 往往要等到
    // QUIC 空闲超时（数十秒）才返回。如果主机在这里等，它就一直回不到
    // `accept()`，接收端断线后重连时根本没人接——续传功能在真实使用中
    // 直接失效。所以：发完 BYE、关掉连接，立刻回去准备接下一个。
    conn.close(0u32.into(), b"done");
    Ok(summary)
}

/// 把一次会话里两个方向的战果合并起来。
///
/// 两边各记各的账是有原因的：并发跑两个方向时，同一个 `&mut TransferSummary`
/// 会被借用两次（Rust 不允许），也说不清"这个数字是谁的"。
fn merge_summary(into: &mut TransferSummary, other: TransferSummary) {
    into.files_sent += other.files_sent;
    into.bytes_sent += other.bytes_sent;
    into.received_files += other.received_files;
    into.received_bytes += other.received_bytes;
    into.texts.extend(other.texts);
    into.failures.extend(other.failures);
}

/// 把自己清单里的东西发出去：等对方逐个 OFFER，然后送数据。
///
/// 返回 `(战果, 需要调用方处理的帧)`。把最后一帧交回去是有意的：
/// - 收到 BYE：会话该结束了；
/// - 收到 MANIFEST：对方也要往房间里放东西（第二级：双向共享空间）。
/// 这两个决定由调用方做——主机和接收端各有自己的下一步。
async fn serve_items(
    conn: &quinn::Connection,
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    plan: &TransferPlan,
    progress: &ProgressSender,
    summary: &mut TransferSummary,
) -> Result<Option<Frame>> {
    loop {
        // 收尾健壮性：接收端传完后可能直接关闭连接，此处的读会以
        // "连接丢失"结束。传输其实已经成功完成，不该当成错误——否则
        // 用户会遇到"文件明明收好了却报失败"。
        let frame = match read_frame_watchdog(conn, recv).await {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(None), // 对端正常关闭发送方向
            Err(e) if is_peer_gone(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        match frame.kind {
            // 需要调用方决定的两帧：BYE 该收尾、MANIFEST 说明对方也要放东西
            KIND_BYE | KIND_MANIFEST => return Ok(Some(frame)),
            KIND_OFFER => {
                let offer: FileOffer = frame.decode_json()?;
                match send_one_file(send, plan, &offer, progress).await {
                    Ok(bytes) => {
                        summary.files_sent += 1;
                        summary.bytes_sent += bytes;
                        let _ = write_frame(
                            send,
                            &Frame::json(
                                KIND_RESULT,
                                &TransferResult {
                                    file_id: offer.file_id,
                                    ok: true,
                                    error: None,
                                },
                            )?,
                        )
                        .await;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        // 立即把失败原因打出来给操作者看。只在会话结束时
                        // 汇总打印是不够的——主机现在会长期驻留接受重连，
                        // 用户根本看不到那份汇总，出问题时只能干瞪眼。
                        eprintln!("[主机] 发送 {} 失败：{msg}", offer.relative_path);
                        summary.failures.push((offer.relative_path.clone(), msg.clone()));
                        let _ = write_frame(
                            send,
                            &Frame::json(
                                KIND_RESULT,
                                &TransferResult {
                                    file_id: offer.file_id,
                                    ok: false,
                                    error: Some(msg),
                                },
                            )?,
                        )
                        .await;
                    }
                }
            }
            KIND_ERROR => {
                let e: ErrorMsg = frame.decode_json()?;
                return Err(Error::protocol(format!("接收端报告错误：{}", e.message)));
            }
            // 接收端对上一个文件的确认。正常路径下它是成对出现的，
            // 收到就继续等下一个 OFFER；这里不当作错误，否则会话末尾会
            // 因为多一个确认帧而整体失败（文件其实已经传完了）。
            KIND_RESULT => {
                let r: TransferResult = frame.decode_json()?;
                if !r.ok {
                    summary.failures.push((
                        r.file_id,
                        r.error.unwrap_or_else(|| "接收端未说明原因".into()),
                    ));
                }
            }
            other => {
                return Err(Error::protocol(format!("会话中收到意外的帧类型 {other}")));
            }
        }
    }


}

/// 发送单个文件。`offer.have_bytes` 是接收端声明的续传起点。
async fn send_one_file(
    send: &mut quinn::SendStream,
    plan: &TransferPlan,
    offer: &FileOffer,
    progress: &ProgressSender,
) -> Result<u64> {
    let planned = plan
        .files
        .iter()
        .find(|f| f.file_id == offer.file_id)
        .ok_or_else(|| Error::protocol(format!("接收端请求了未知文件 {}", offer.relative_path)))?;

    if planned.size != offer.size {
        return Err(Error::protocol(format!(
            "{} 的大小不一致（本机 {} 字节，接收端记录 {} 字节），可能源文件已被修改",
            planned.relative_path, planned.size, offer.size
        )));
    }

    // 接收端声明的起点不能超过文件大小，否则视为不可信，从 0 重传
    let start_offset = if offer.have_bytes > planned.size {
        0
    } else {
        offer.have_bytes
    };


    write_frame(
        send,
        &Frame::json(
            KIND_ACK,
            &OfferAck {
                file_id: offer.file_id.clone(),
                start_offset,
            },
        )?,
    )
    .await?;

    progress.send(ProgressEvent::FileStarted {
        file_id: planned.file_id.clone(),
        relative_path: planned.relative_path.clone(),
        size: planned.size,
        resumed_from: start_offset,
    });
    progress.send(ProgressEvent::ChunkProgress {
        file_id: planned.file_id.clone(),
        bytes_done: start_offset,
        bytes_total: planned.size,
    });

    let chunk_size = if offer.chunk_size == 0 || offer.chunk_size > DEFAULT_CHUNK_SIZE {
        DEFAULT_CHUNK_SIZE
    } else {
        offer.chunk_size
    } as usize;

    // 数据来源：文件从磁盘读，文本直接在内存里。
    // 其余部分（分块、进度、结束帧、校验）完全一样——这就是"复用文件那套"的意思。
    let mut buf = vec![0u8; chunk_size];
    let mut sent = start_offset;
    match planned.kind {
        ItemKind::Text => {
            let text = planned.text.as_deref().unwrap_or_default().as_bytes();
            let from = (start_offset as usize).min(text.len());
            for part in text[from..].chunks(chunk_size) {
                write_frame(send, &Frame::new(KIND_DATA, part.to_vec())).await?;
                sent += part.len() as u64;
                progress.send(ProgressEvent::ChunkProgress {
                    file_id: planned.file_id.clone(),
                    bytes_done: sent,
                    bytes_total: planned.size,
                });
            }
        }
        ItemKind::File => {
            let mut file = tokio::fs::File::open(&planned.source_path)
                .await
                .map_err(|e| Error::io(&planned.source_path, e))?;
            if start_offset > 0 {
                file.seek(std::io::SeekFrom::Start(start_offset))
                    .await
                    .map_err(|e| Error::io(&planned.source_path, e))?;
            }
            loop {
                let n = file
                    .read(&mut buf)
                    .await
                    .map_err(|e| Error::io(&planned.source_path, e))?;
                if n == 0 {
                    break;
                }
                write_frame(send, &Frame::new(KIND_DATA, buf[..n].to_vec())).await?;
                sent += n as u64;
                progress.send(ProgressEvent::ChunkProgress {
                    file_id: planned.file_id.clone(),
                    bytes_done: sent,
                    bytes_total: planned.size,
                });
            }
        }
    }

    // 结束时告知对端哈希，接收端据此校验
    write_frame(
        send,
        &Frame::json(
            KIND_FILE_END,
            &FileEnd {
                file_id: planned.file_id.clone(),
                blake3: planned.blake3.clone(),
            },
        )?,
    )
    .await?;

    progress.send(ProgressEvent::FileFinished {
        file_id: planned.file_id.clone(),
        relative_path: planned.relative_path.clone(),
    });

    Ok(sent - start_offset)
}

pub struct ReceiverOptions {
    pub payload: QrPayload,
    pub dest_dir: PathBuf,
    pub device_name: String,
    pub continue_partial: bool,
    /// 我方也要放进房间里的东西（None = 只取不放）。
    ///
    /// 这是第二级「双向共享空间」的接收端开关：对方取我的东西，我也往对方那里放。
    pub outgoing: Option<TransferPlan>,
    /// 取消信号。传 `CancelToken::new()` 表示不取消。
    pub cancel: CancelToken,
}

pub struct Receiver;

impl Receiver {
    /// 连接主机、接收全部文件。返回摘要。
    pub async fn run(opts: ReceiverOptions, progress: &ProgressSender) -> Result<TransferSummary> {
        tls::ensure_crypto_provider();

        // 会话中断就自动重连续传，而不是把"重跑一次命令"推给用户。
        //
        // 为什么必须有：WiFi 抖一下、笔记本合盖、主机那边点了停止，都会让连接断掉。
        // 断点续传的价值恰恰在"断线之后"才体现——如果每次断线都要用户手动重来，
        // 那这项能力只算做了一半。
        //
        // 只重试"握手成功之后才断"的情形：握手阶段的失败走不到这里（见 open_session），
        // 会话正常收尾（哪怕有文件因为目标写不进去被跳过）也不会进到这个分支。
        const MAX_ATTEMPTS: u32 = 4;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let (conn, _endpoint) = connect_to_host(&opts, progress).await?;
            let start = open_session(&conn, &opts, progress).await?;

            match transfer_files(&conn, start, &opts, progress).await {
                Ok(summary) => return Ok(summary),
                Err(e) if attempt < MAX_ATTEMPTS && is_retryable_interruption(&e) => {
                    // 退避 1s、2s、4s：短暂抖动一秒就够，真断了也不会让人干等
                    let wait = std::time::Duration::from_secs(1 << (attempt - 1));
                    progress.send(ProgressEvent::Warn(format!(
                        "连接中断（{e}）。{} 秒后自动重连续传（第 {attempt}/{MAX_ATTEMPTS} 次尝试）",
                        wait.as_secs()
                    )));
                    if !sleep_unless_cancelled(wait, &opts.cancel).await {
                        return Err(Error::Cancelled);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// 连接到主机：轮询二维码里的候选地址，直到连上或者等够时间。
///
/// 单独抽成函数，是因为**每次自动重连都要再走一遍**：主机被强杀时它要等空闲超时
/// 才会回到 `accept()`，这中间有几秒钟"主机还没准备好再接一个"的窗口。断线续传
/// 的价值恰恰在"断了还能接上"，所以这里必须耐心等，而不是失败一次就报"连不上"。
async fn connect_to_host(
    opts: &ReceiverOptions,
    progress: &ProgressSender,
) -> Result<(quinn::Connection, quinn::Endpoint)> {
    let mut last_err: Option<Error> = None;
    let mut conn = None;

    // 外层重试：接收端被强杀时，主机要等到空闲超时才会发现，之后才回到
    // `accept()`。这中间有几秒钟"主机还没准备好再接一个"的窗口。
    // 续传的价值恰恰在"断线后还能接上"，所以这里必须耐心等，而不是
    // 失败一次就告诉用户"连不上"。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut round = 0u32;
    while conn.is_none() {
        opts.cancel.check()?;
        round += 1;
        if round > 1 {
            progress.send(ProgressEvent::Warn(format!(
                "主机暂时还连不上，正在重试（第 {round} 次）……如果主机刚结束上一个会话，请稍候几秒"
            )));
        }

        // 主机可能给了多个候选地址（多网卡），挨个试
        for hint in &opts.payload.addrs {
            let addr = match tls::resolve_addr(&hint.host, hint.port).await {
                Ok(a) => a,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };

            let (client_cfg, fp_rejected) = tls::client_config(&opts.payload.fp)?;
            let mut quinn_cfg = quinn::ClientConfig::new(Arc::new(
                quinn::crypto::rustls::QuicClientConfig::try_from(client_cfg)
                    .map_err(|e| Error::protocol(format!("QUIC 客户端配置失败: {e}")))?,
            ));
            quinn_cfg.transport_config(transport_config());

            let sock = bind_udp("0.0.0.0:0".parse().unwrap())
                .map_err(|e| Error::protocol(format!("创建本地 UDP 端点失败: {e}")))?;
            let mut endpoint = quinn::Endpoint::new(
                quinn::EndpointConfig::default(),
                None,
                sock,
                Arc::new(quinn::TokioRuntime),
            )
            .map_err(|e| Error::protocol(format!("创建本地 UDP 端点失败: {e}")))?;
            endpoint.set_default_client_config(quinn_cfg);

            match endpoint.connect(addr, crate::identity::SERVER_NAME) {
                Ok(connecting) => {
                    // 三路竞速：连上、校验器判定指纹不符、整体超时。
                    //
                    // 为什么要单独听 `fp_rejected`：QUIC 在证书被拒时不会立刻
                    // 报错，而是静默重试到握手超时。只靠超时的话，一个"二维码
                    // 过期"就要让用户干等十几秒，还只能看到笼统的"连接超时"。
                    let flag_rx = fp_rejected.clone();
                    let outcome = tokio::select! {
                        r = connecting => Some(r),
                        _ = async {
                            loop {
                                if flag_rx.load(std::sync::atomic::Ordering::SeqCst) {
                                    break;
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                            }
                        } => None,
                        _ = tokio::time::sleep(CONNECT_TIMEOUT) => {
                            last_err = Some(Error::protocol(format!(
                                "连接 {addr} 超时（{} 秒）。可能原因：双方不在同一局域网、主机已停止分享、或防火墙拦截了 UDP",
                                CONNECT_TIMEOUT.as_secs()
                            )));
                            None
                        }
                    };

                    match outcome {
                        None if fp_rejected.load(std::sync::atomic::Ordering::SeqCst) => {
                            // 指纹不符是安全问题，绝不能静默重试下一个地址
                            return Err(Error::FingerprintMismatch {
                                expected: opts.payload.fp.clone(),
                                actual: "（主机出示的证书与二维码不一致）".into(),
                            });
                        }
                        None => {}
                        Some(Ok(c)) => {
                            conn = Some((c, endpoint));
                            break;
                        }
                        Some(Err(e)) => {
                            if is_fingerprint_mismatch(&e) {
                                return Err(Error::FingerprintMismatch {
                                    expected: opts.payload.fp.clone(),
                                    actual: "（主机出示的证书与二维码不一致）".into(),
                                });
                            }
                            last_err = Some(tls::friendly_connect_error(addr, &e));
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(Error::protocol(format!("无法发起连接 {addr}：{e}")));
                }
            }
        }

        if conn.is_none() {
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
// ---- SPLICE MARKER ----
    conn.ok_or_else(|| {
        last_err.unwrap_or_else(|| Error::protocol("无法连接到主机，二维码里的地址都试过了"))
    })
}

/// 会话开场：交换问候、拿到文件清单、把续传状态准备好。
struct SessionStart {
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    manifest: FileManifest,
    max_chunk_size: u32,
}

/// 开场（握手 + 清单 + 续传状态）。
///
/// 这里的失败**不自动重试**：二维码过期、协议版本不符、主机停了分享，重试多少次
/// 结果都一样，不如立刻把话说清楚，让用户去重新扫码。
async fn open_session(
    conn: &quinn::Connection,
    opts: &ReceiverOptions,
    progress: &ProgressSender,
) -> Result<SessionStart> {
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| Error::protocol(format!("无法建立数据流: {e}")))?;

    // ---- 1. 握手 ----
    let hello = ClientHello {
        protocol_version: PROTOCOL_VERSION,
        session_id: opts.payload.sid.clone(),
        device_name: opts.device_name.clone(),
    };
    write_frame(&mut send, &Frame::json(KIND_HELLO, &hello)?).await?;

    let frame = read_frame(&mut recv)
        .await?
        .ok_or_else(|| Error::protocol("主机在握手阶段断开了连接"))?;
    if frame.kind == KIND_ERROR {
        let e: ErrorMsg = frame.decode_json()?;
        return Err(Error::protocol(e.message));
    }
    frame.expect_kind(KIND_HELLO, "握手响应")?;
    let server_hello: ServerHello = frame.decode_json()?;
    if server_hello.protocol_version != PROTOCOL_VERSION {
        return Err(Error::protocol(format!(
            "主机协议版本 {} 与本机 {PROTOCOL_VERSION} 不匹配",
            server_hello.protocol_version
        )));
    }

    // 我带着东西来，对方却没开接收目录：这是用法错误，立刻说清楚。
    // （不这样做的后果实测过：东西推到一半被拒，然后按"连接中断"重试四轮，
    // 每轮都重复同一句用法错误，用户只看到一串莫名其妙的重试。）
    if opts.outgoing.is_some() && !server_hello.accepts_incoming {
        return Err(Error::OfferRejected {
            name: "我方要放进房间的东西".to_string(),
            reason: "对方这次只往外分享，没有开接收目录（他启动时要加 --to 目录）".to_string(),
        });
    }

    // ---- 2. 清单 ----
    let frame = read_frame(&mut recv)
        .await?
        .ok_or_else(|| Error::protocol("主机没有发送文件清单"))?;
    frame.expect_kind(KIND_MANIFEST, "文件清单")?;
    let manifest: FileManifest = frame.decode_json()?;

    progress.send(ProgressEvent::SessionStarted {
        peer: server_hello.device_name.clone(),
        total_files: manifest.files.len(),
        total_bytes: manifest.total_bytes,
    });


    Ok(SessionStart {
        send,
        recv,
        manifest,
        max_chunk_size: server_hello.max_chunk_size,
    })
}

/// 逐个文件接收，直到清单走完。
///
/// 这里开始的失败**才值得自动重连**：连接是在传输途中断的，而进度已经落在磁盘上，
/// 重连一次就能只补差额。
async fn transfer_files(
    conn: &quinn::Connection,
    start: SessionStart,
    opts: &ReceiverOptions,
    progress: &ProgressSender,
) -> Result<TransferSummary> {
    let SessionStart {
        mut send,
        mut recv,
        manifest,
        max_chunk_size,
    } = start;

    let mut summary = TransferSummary::default();
    // ---- 两条流：A 用来取、B 用来放（第三级：并发投放）----
    //
    // 为什么要两条流：一条流上"谁先说话"必须写死，两边同时放东西就会互相干等
    // （第二级是靠"顺序交换 + 我取完了的 BYE"绕过去的，代价是必须排队）。
    // QUIC 的流很便宜，给它两条：A 上对方送、我们取；B 上我们送、对方取。
    // 两个方向各跑各的，谁也不用等谁——这才是"房间"该有的样子。
    //
    // 兼容性：老版本只开一条流，所以**协议版本号要跟着涨**（两边必须同版本）。
    let (mut send_b, mut recv_b) = conn
        .open_bi()
        .await
        .map_err(|e| Error::protocol(format!("无法建立第二条数据流: {e}")))?;

    // 空清单也要发：对方据此知道"这个方向没有东西可搬"，而不是一直等
    let outgoing_entries: Vec<FileEntry> = opts
        .outgoing
        .as_ref()
        .map(|plan| {
            plan.files
                .iter()
                .map(|f| FileEntry {
                    file_id: f.file_id.clone(),
                    relative_path: f.relative_path.clone(),
                    size: f.size,
                    blake3: f.blake3.clone(),
                    kind: f.kind,
                })
                .collect()
        })
        .unwrap_or_default();
    let out_manifest = FileManifest {
        files: outgoing_entries,
        total_bytes: opts.outgoing.as_ref().map(|p| p.total_bytes).unwrap_or(0),
    };
    write_frame(&mut send_b, &Frame::json(KIND_MANIFEST, &out_manifest)?).await?;

    // 两个方向各记各的账，最后合并（同一个 summary 会被借用两次，Rust 也不允许）
    let mut taken = TransferSummary::default();
    let mut given = TransferSummary::default();
    let empty_plan = TransferPlan::default();
    let outgoing_plan = opts.outgoing.as_ref().unwrap_or(&empty_plan);

    let take = async {
        let r = pull_items(
            conn,
            &mut send,
            &mut recv,
            &manifest.files,
            &opts.dest_dir,
            &opts.payload.sid,
            opts.continue_partial,
            max_chunk_size,
            progress,
            &opts.cancel,
            &mut taken,
        )
        .await;
        // 取得差不多就告诉对方"我取完了"：对方的发送循环靠这句话收尾，
        // 别等到另一个方向也结束——那样对方会白等（它还在等我们的 OFFER）。
        let _ = write_frame(
            &mut send,
            &Frame::json(
                KIND_BYE,
                &Bye {
                    reason: Some("receiver done".into()),
                },
            )?,
        )
        .await;
        let _ = send.finish();
        r
    };
    let give = serve_items(
        conn,
        &mut send_b,
        &mut recv_b,
        outgoing_plan,
        progress,
        &mut given,
    );
    // join 而不是 try_join：一个方向出错不该把另一个方向掐掉，
    // "我这边收不到"和"我这边发不出"是两件事，让它们各自跑完再一起报。
    let (taken_result, given_result) = tokio::join!(take, give);
    merge_summary(&mut summary, taken);
    merge_summary(&mut summary, given);

    let _ = send_b.finish();

    if let Err(e) = taken_result {
        return Err(e);
    }
    if let Err(e) = given_result {
        return Err(e);
    }

    // 全部成功时把续传状态文件删掉：它只是"下次能少传一点"的辅助信息，
    // 传完后留着既没有用，也和"不留痕"的定位不符（用户目录里平白多出一个
    // 看不懂的 json）。有失败时保留，方便同一会话内重试续传。
    if summary.failures.is_empty() {
        let _ = std::fs::remove_file(opts.dest_dir.join(RESUME_FILE));
    }

    progress.send(ProgressEvent::SessionFinished {
        files: summary.files_sent,
        texts: summary.texts.len(),
        bytes: summary.bytes_sent,
    });

    // 主动关闭，不去等 conn.closed()：等它只会一直拖到 QUIC 空闲超时。
    conn.close(0u32.into(), b"done");
    Ok(summary)
}

/// 这个错误值不值得自动重连？
///
/// 能走到这里说明握手已经成功，所以"二维码过期"这类死路已经排除了。剩下要挡的是
/// 重试也不会变好的几种：用户主动取消、证书指纹不符（安全问题，绝不能靠重试掩盖）、
/// 路径不安全、磁盘空间不足。
fn is_retryable_interruption(e: &Error) -> bool {
    !matches!(
        e,
        Error::Cancelled
            | Error::FingerprintMismatch { .. }
            | Error::UnsafePath(_)
            | Error::InsufficientSpace { .. }
            // 对端明确拒绝（比如"我没开接收目录"）：这是用法问题，重试不会变好
            | Error::OfferRejected { .. }
    )
}

/// 睡一段时间，期间可以被取消打断。返回 false 表示用户取消了。
///
/// 不用一个 `sleep` 是因为取消要能**立刻**生效：退避最长 4 秒，用户按了停止却还要
/// 等满 4 秒才退出，会让人以为程序卡住了。
async fn sleep_unless_cancelled(dur: std::time::Duration, cancel: &CancelToken) -> bool {
    const STEP: std::time::Duration = std::time::Duration::from_millis(100);
    let mut left = dur;
    while left > std::time::Duration::ZERO {
        if cancel.is_cancelled() {
            return false;
        }
        let slice = left.min(STEP);
        tokio::time::sleep(slice).await;
        left = left.saturating_sub(slice);
    }
    !cancel.is_cancelled()
}

/// 接收一个文件之前的本地准备：建父目录、真预分配、查磁盘空间。
///
/// **必须在发 OFFER 之前调用。** 主机一旦收到 OFFER 就会一路把文件推过来，中途
/// 不会听指挥；如果这里失败了而我们又已经发过 OFFER，主机推来的数据就没人接，
/// 接收端接下来读到的会是数据帧、却被当成控制帧解析——整个会话失步，连后面
/// 本来没问题的文件也一起完蛋。放在发 OFFER 之前失败，就只是"这个文件跳过"。
///
/// `start_offset == 0` 时先归零再分配：这样上次留下的、比目标更长的残缺文件
/// 不会把尾巴带进最终结果。
async fn prepare_target(part: &Path, total_size: u64, start_offset: u64) -> Result<()> {
    let part = part.to_path_buf();
    // 预分配是阻塞式系统调用（而且可能要真的分配几个 GB），扔进阻塞线程池，
    // 不要让它在 async 运行时上卡住 QUIC 的定时器。
    tokio::task::spawn_blocking(move || {
        if start_offset == 0 {
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&part) {
                let _ = f.set_len(0);
            }
        }
        // 空间不够会返回 InsufficientSpace（带"还需要/只剩"），
        // 而不是等传到 90% 才炸
        fs_util::preallocate(&part, total_size)
    })
    .await
    .map_err(|e| Error::protocol(format!("预分配任务失败：{e}")))??;
    Ok(())
}

/// 收下一批条目（文件落盘、文本进内存）。
///
/// 主机的"发"和接收端的"收"原来写死在各自那一侧；双向房间要求**两边都能收**，
/// 所以这段被抽出来复用：谁收东西谁调它，与它在会话里扮演什么角色无关。
/// 战果累加到调用方传进来的 `summary` 上（一次会话可能两个方向都有收获）。
#[allow(clippy::too_many_arguments)]
async fn pull_items(
    conn: &quinn::Connection,
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    entries: &[FileEntry],
    dest_dir: &Path,
    session_id: &str,
    continue_partial: bool,
    max_chunk_size: u32,
    progress: &ProgressSender,
    cancel: &CancelToken,
    summary: &mut TransferSummary,
) -> Result<()> {
    // ---- 3. 准备续传状态 ----
    tokio::fs::create_dir_all(&dest_dir)
    .await
    .map_err(|e| Error::io(&dest_dir, e))?;
    let mut state = if continue_partial {
    ResumeState::load(&dest_dir)
    } else {
    ResumeState::new(session_id.to_string())
    };
    if state.session_id.is_empty() {
    state.session_id = session_id.to_string();
    }

    for entry in entries {
        cancel.check()?;
            // 文本条目：不落盘，收完交给界面/终端。
            // 它和文件走同一套 OFFER/ACK/DATA/RESULT，只是没有 .part、没有续传状态。
            // 必须放在路径处理**之前**：文本的"名字"是给人看的说明（可能带斜杠等
            // 路径里不允许的字符），不该拿它去做路径校验。
            if entry.kind == ItemKind::Text {
                match receive_text_item(
                    &conn,
                    send,
                    recv,
                    entry,
                    max_chunk_size,
                    progress,
                    &cancel,
                )
                .await
                {
                    Ok(text) => {
                        summary.bytes_sent += entry.size;
                        summary.received_bytes += entry.size;
                        progress.send(ProgressEvent::TextReceived {
                            label: entry.relative_path.clone(),
                            text: text.clone(),
                        });
                        summary.texts.push((entry.relative_path.clone(), text));
                        let _ = write_frame(
                            send,
                            &Frame::json(
                                KIND_RESULT,
                                &TransferResult {
                                    file_id: entry.file_id.clone(),
                                    ok: true,
                                    error: None,
                                },
                            )?,
                        )
                        .await;
                        drain_result(recv, &entry.file_id).await?;
                        continue;
                    }
                    Err(e) => {
                        if matches!(e, Error::Cancelled) {
                            return Err(e);
                        }
                        // 和文件同样的策略：传一半失败必须结束整次会话，避免单流失步
                        let msg = e.to_string();
                        progress.send(ProgressEvent::Warn(format!(
                            "{} 没能收到：{msg}",
                            entry.relative_path
                        )));
                        summary.failures.push((entry.relative_path.clone(), msg.clone()));
                        let _ = write_frame(
                            send,
                            &Frame::json(
                                KIND_RESULT,
                                &TransferResult {
                                    file_id: entry.file_id.clone(),
                                    ok: false,
                                    error: Some(msg),
                                },
                            )?,
                        )
                        .await;
                        return Err(e);
                    }
                }
            }

        let rel = fs_util::safe_relative_path(&entry.relative_path)?;
        let target = fs_util::join_checked(&dest_dir, &rel);
        let part = fs_util::part_path(&target);

        // 目标文件**已经在最终位置**且内容对得上：这次什么都不用做。
        //
        // 常见场景是"同一个文件再发一次到同一个目录"，或者上次会话其实已经传完、
        // 只是收尾时出了别的问题。不检查这一步的话，会白下载一整遍：
        // 跳过逻辑只看 .part，而成功的文件早就改名成最终名字了。
        //
        // 代价是要把最终文件完整哈希一遍（1.4GB/s 量级），相比之下重传一遍要慢得多，
        // 所以这个交换是划算的。哈希对不上就照常走下面的流程（重新收）。
        if entry.kind == ItemKind::File {
            let target_ok = fs_util::hash_file(&target)
                .map(|h| hex::encode(h.as_bytes()).eq_ignore_ascii_case(&entry.blake3))
                .unwrap_or(false);
            if target_ok {
                state.upsert(PartialFile {
                    relative_path: entry.relative_path.clone(),
                    file_id: entry.file_id.clone(),
                    total_size: entry.size,
                    partial: entry.size,
                    partial_hash: None,
                    completed: true,
                });
                progress.send(ProgressEvent::FileFinished {
                    file_id: entry.file_id.clone(),
                    relative_path: entry.relative_path.clone(),
                });
                summary.files_sent += 1;
                summary.bytes_sent += entry.size;
                summary.received_files += 1;
                summary.received_bytes += entry.size;
                continue;
            }
        }

        // 已完成的文件直接跳过（这才是"断点续传"里省时间的部分：
        // 重连后不重传已经收好的文件）
        let mut have = state.resume_offset(&entry.file_id, &part, entry.size);
        if have >= entry.size && entry.size > 0 {
            // 已经从上次会话收完了？重新校验一遍再确认，不能只看状态文件
            let part_hash_ok = match fs_util::hash_file(&part) {
                Ok(h) => hex::encode(h.as_bytes()).eq_ignore_ascii_case(&entry.blake3),
                Err(_) => false,
            };
            if part_hash_ok {
                fs_util::atomic_rename(&part, &target)?;
                state.upsert(PartialFile {
                    relative_path: entry.relative_path.clone(),
                    file_id: entry.file_id.clone(),
                    total_size: entry.size,
                    partial: entry.size,
                    partial_hash: None,
                    completed: true,
                });
                progress.send(ProgressEvent::FileFinished {
                    file_id: entry.file_id.clone(),
                    relative_path: entry.relative_path.clone(),
                });
                summary.files_sent += 1;
                summary.bytes_sent += entry.size;
                summary.received_files += 1;
                summary.received_bytes += entry.size;
                continue;
            }
            // 校验没过：删掉重来，并且**把起点归零**。
            //
            // 忘了归零就会出现这样一串怪事：状态说"已完成"、磁盘上的 .part
            // 其实已经被上次成功改名（或删掉）了，于是拿"完整长度"当续传起点去
            // 要一个空文件，最后报一个用户完全看不懂的"BLAKE3 不一致"，而且每次
            // 重试都重演一遍（实测能一直重试到次数耗尽）。
            let _ = std::fs::remove_file(&part);
            have = 0;
        }

        // 本地准备必须在**发 OFFER 之前**做完，理由见 prepare_target 的注释：
        // 这一步一旦失败而又已经发过 OFFER，主机推来的数据就没人接，
        // 整个单流会话会失步（后面所有文件都跟着完蛋）。
        // 放在前面失败，最坏也只是"这个文件跳过"，会话继续。
        if let Err(e) = prepare_target(&part, entry.size, have).await {
            let msg = e.to_string();
            progress.send(ProgressEvent::Warn(format!(
                "跳过 {}：{msg}",
                entry.relative_path
            )));
            summary.failures.push((entry.relative_path.clone(), msg));
            continue;
        }

        let offer = FileOffer {
            file_id: entry.file_id.clone(),
            relative_path: entry.relative_path.clone(),
            size: entry.size,
            blake3: entry.blake3.clone(),
            have_bytes: have,
            chunk_size: max_chunk_size,
        };
        write_frame(send, &Frame::json(KIND_OFFER, &offer)?).await?;

        let frame = read_frame(recv)
            .await?
            .ok_or_else(|| Error::protocol("主机在协商阶段断开"))?;
        frame.expect_kind(KIND_ACK, "续传确认")?;
        let ack: OfferAck = frame.decode_json()?;
        let start_offset = ack.start_offset.min(entry.size);

        let received = match receive_one_file(
            recv,
            &entry.file_id,
            &entry.relative_path,
            &target,
            &part,
            entry.size,
            start_offset,
            &entry.blake3,
            &mut state,
            &dest_dir,
            progress,
            &cancel,
        )
        .await
        {
            Ok(n) => n,
            Err(e) => {
                // 取消不是"某个文件失败了"，而是整次接收停止：直接退出，
                // 绝不能记成失败后继续下一个文件
                if matches!(e, Error::Cancelled) {
                    return Err(e);
                }
                // 传一半才失败（磁盘写错、对端断开、校验不符……）必须**结束整次会话**，
                // 不能"记一笔失败，接着协商下一个文件"。
                //
                // 原因：主机此刻还在按自己的节奏推这个文件的数据，接收端如果跳去谈
                // 下一个文件，流上剩下的数据帧会被当成控制帧解析，单流会话就此失步
                // （实测报错是"期望续传确认（帧类型 3），实际收到帧类型 4"）。
                // 结束会话是安全的一侧：已收到的部分和检查点都留着，用户重跑一次
                // 就能从这个文件的断点续上，而不是留下一个静默错乱的状态。
                let msg = e.to_string();
                progress.send(ProgressEvent::Warn(format!(
                    "{} 中断：{msg}；已收到的部分已保留，重新运行可续传",
                    entry.relative_path
                )));
                summary.failures.push((entry.relative_path.clone(), msg.clone()));
                // 尽力通知主机（它此刻多半在写，看不到这条，但正常收尾时用得上）
                let _ = write_frame(
                    send,
                    &Frame::json(
                        KIND_RESULT,
                        &TransferResult {
                            file_id: entry.file_id.clone(),
                            ok: false,
                            error: Some(msg),
                        },
                    )?,
                )
                .await;
                return Err(e);
            }
        };

        summary.files_sent += 1;
        summary.bytes_sent += received;
        summary.received_files += 1;
        summary.received_bytes += received;

        let _ = write_frame(
            send,
            &Frame::json(
                KIND_RESULT,
                &TransferResult {
                    file_id: entry.file_id.clone(),
                    ok: true,
                    error: None,
                },
            )?,
        )
        .await;

        // 必须等主机对 RESULT 的回应，否则会话会失去同步：
        // 主机在文件发完后会读下一个 OFFER，而我们如果直接进入下一个
        // 文件或发 BYE，主机就会读到"意外帧"。这里同步一次，让双方的
        // 收发永远成对。
        drain_result(recv, &entry.file_id).await?;
    }
    Ok(())
}

/// 接收一段文本：走同一套 OFFER/ACK/DATA/FILE_END 流程，但**不落盘**。
///
/// 为什么复用文件那一套而不是另开"文本通道"：文本条目在协议上就是一个小文件，
/// 复用意味着校验（BLAKE3）、分块、进度、结果确认只有一份实现。
/// 区别只有：没有 `.part`、没有续传状态、内容进内存后交给界面。
async fn receive_text_item(
    conn: &quinn::Connection,
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    entry: &FileEntry,
    chunk_size: u32,
    progress: &ProgressSender,
    cancel: &CancelToken,
) -> Result<String> {
    let offer = FileOffer {
        file_id: entry.file_id.clone(),
        relative_path: entry.relative_path.clone(),
        size: entry.size,
        blake3: entry.blake3.clone(),
        // 文本不续传：它本来就小，"半个文本"对用户也没有意义
        have_bytes: 0,
        chunk_size,
    };
    write_frame(send, &Frame::json(KIND_OFFER, &offer)?).await?;

    let frame = read_frame(recv)
        .await?
        .ok_or_else(|| Error::protocol("主机在文本协商阶段断开"))?;
    frame.expect_kind(KIND_ACK, "续传确认")?;
    let _ack: OfferAck = frame.decode_json()?;

    progress.send(ProgressEvent::FileStarted {
        file_id: entry.file_id.clone(),
        relative_path: entry.relative_path.clone(),
        size: entry.size,
        resumed_from: 0,
    });

    // 上限比发送端宽松，但必须有：对端要是坏了，我们不能无限吃内存
    let cap = crate::transfer::plan::MAX_TEXT_BYTES as u64 + 4096;
    let mut out: Vec<u8> = Vec::with_capacity(entry.size as usize);
    let mut hasher = blake3::Hasher::new();

    loop {
        cancel.check()?;
        let Some(frame) = read_frame_watchdog(conn, recv).await? else {
            return Err(Error::Disconnected {
                received: out.len() as u64,
                total: entry.size,
            });
        };
        match frame.kind {
            KIND_DATA => {
                if out.len() as u64 + frame.payload.len() as u64 > cap {
                    return Err(Error::protocol("对端发来的文本超过大小上限，已中止"));
                }
                hasher.update(&frame.payload);
                out.extend_from_slice(&frame.payload);
                progress.send(ProgressEvent::ChunkProgress {
                    file_id: entry.file_id.clone(),
                    bytes_done: out.len() as u64,
                    bytes_total: entry.size,
                });
            }
            KIND_FILE_END => {
                let end: FileEnd = frame.decode_json()?;
                if !end.blake3.eq_ignore_ascii_case(&entry.blake3) {
                    return Err(Error::protocol(format!(
                        "{} 的校验基准不一致，拒绝接收（请让主机重新分享）",
                        entry.relative_path
                    )));
                }
                break;
            }
            KIND_ERROR => {
                let e: ErrorMsg = frame.decode_json()?;
                return Err(Error::protocol(format!("主机报告错误：{}", e.message)));
            }
            other => {
                return Err(Error::protocol(format!(
                    "接收文本时收到意外的帧类型 {other}"
                )));
            }
        }
    }

    if out.len() as u64 != entry.size {
        return Err(Error::Disconnected {
            received: out.len() as u64,
            total: entry.size,
        });
    }
    let actual = hex::encode(hasher.finalize().as_bytes());
    if !actual.eq_ignore_ascii_case(&entry.blake3) {
        return Err(Error::ChecksumMismatch {
            path: PathBuf::from(&entry.relative_path),
        });
    }
    String::from_utf8(out)
        .map_err(|_| Error::protocol("对端发来的内容不是有效文本（UTF-8 解码失败）"))
}

/// 接收单个文件：协商好的偏移开始，流式写 `.part`，每块之后写检查点。
#[allow(clippy::too_many_arguments)]
async fn receive_one_file(
    recv: &mut quinn::RecvStream,
    file_id: &str,
    relative_path: &str,
    target: &Path,
    part: &Path,
    total_size: u64,
    start_offset: u64,
    expected_blake3: &str,
    state: &mut ResumeState,
    dest_dir: &Path,
    progress: &ProgressSender,
    cancel: &CancelToken,
) -> Result<u64> {
    progress.send(ProgressEvent::FileStarted {
        file_id: file_id.to_string(),
        relative_path: relative_path.to_string(),
        size: total_size,
        resumed_from: start_offset,
    });

    // 本地准备（建父目录、真预分配、空间检查）在上面发 OFFER 之前已经做过一次，
    // 这里再调一遍是兜底：重复调用是幂等的（`preallocate` 见到长度已经够就直接返回）。
    // 这里用的是对端 ACK 里的 start_offset，所以"主机要求从 0 重传"也能正确截断。
    prepare_target(part, total_size, start_offset).await?;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(part)
        .await
        .map_err(|e| Error::io(part, e))?;
    file.seek(std::io::SeekFrom::Start(start_offset))
        .await
        .map_err(|e| Error::io(part, e))?;

    // 边收边算的滚动哈希器；检查点只对它取快照，不再从头重算。
    //
    // 这里曾经写的是 ResumeState::with_prefix_hash(part, written)：每次检查点
    // 都把整段前缀重读重算一遍。检查点每 8MB 一次，总开销就是
    // 8+16+…+n = O(n²)——512MB 的传输要哈希约 16GB 数据，把吞吐死死压在
    // 50MB/s 上下，而且从耗时归因上完全看不出是哈希干的。
    let mut hasher = blake3::Hasher::new();
    if start_offset > 0 {
        // 续传：先把磁盘上已有的前缀喂进去。这一步是 O(起点)，每个文件只做一次。
        fs_util::feed_prefix(&mut hasher, part, start_offset)?;
    }

    state.upsert(PartialFile {
        relative_path: relative_path.to_string(),
        file_id: file_id.to_string(),
        total_size,
        partial: start_offset,
        // 起点来自对端协商，其可信性已由 resume_offset 的前缀校验保证
        partial_hash: Some(hex::encode(hasher.finalize().as_bytes())),
        completed: false,
    });
    // 检查点写失败不该中断传输：丢掉的只是"下次能少传一点"这个优化，
    // 数据正确性最终由 BLAKE3 校验保证。所以要容忍，而不是让整次传输失败。
    let _ = state.save(dest_dir);

    progress.send(ProgressEvent::ChunkProgress {
        file_id: file_id.to_string(),
        bytes_done: start_offset,
        bytes_total: total_size,
    });

    let mut written = start_offset;
    // 检查点不必每块都写（那是磁盘风暴），按固定间隔写一次
    let checkpoint_every = 8 * 1024 * 1024u64;
    let mut next_checkpoint = start_offset + checkpoint_every;


    loop {
        // 流结束或对端已经走了：把已收到的字节数报上去，让上层保留检查点
        // 以便下次续传，而不是把已传的进度整个丢掉。
        let frame = match read_frame(recv).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                return Err(Error::Disconnected {
                    received: written,
                    total: total_size,
                })
            }
            Err(e) if is_peer_gone(&e) => {
                return Err(Error::Disconnected {
                    received: written,
                    total: total_size,
                })
            }
            Err(e) => return Err(e),
        };

        // 每收到一块检查一次：大文件传输时，这是唯一能及时停下的位置
        cancel.check()?;

        match frame.kind {
            KIND_DATA => {
                if written + frame.payload.len() as u64 > total_size {
                    return Err(Error::protocol(format!(
                        "{relative_path} 收到了超出声明大小的数据，已中止（可能是对端故障）"
                    )));
                }
                file.write_all(&frame.payload)
                    .await
                    .map_err(|e| Error::io(part, e))?;
                hasher.update(&frame.payload);
                written += frame.payload.len() as u64;
                progress.send(ProgressEvent::ChunkProgress {
                    file_id: file_id.to_string(),
                    bytes_done: written,
                    bytes_total: total_size,
                });

                if written >= next_checkpoint {
                    file.flush().await.map_err(|e| Error::io(part, e))?;
                    // 不在这里做 sync_data：每 8MB 一次 fsync 在慢盘/杀毒软件下可能阻塞很久，
                    // 久到 QUIC 空闲超时把连接判死。数据正确性由最终整文件校验保证。
                    state.upsert(PartialFile {
                        relative_path: relative_path.to_string(),
                        file_id: file_id.to_string(),
                        total_size,
                        partial_hash: Some(hex::encode(hasher.finalize().as_bytes())),
                        partial: written,
                        completed: false,
                    });
                    state.save(dest_dir)?;
                    next_checkpoint = written + checkpoint_every;
                }
            }
            KIND_FILE_END => {
                let end: FileEnd = frame.decode_json()?;
                if !end.blake3.eq_ignore_ascii_case(expected_blake3) {
                    return Err(Error::protocol(format!(
                        "{relative_path} 的校验基准不一致，拒绝写入（请让主机重新分享）"
                    )));
                }
                break;
            }
            KIND_ERROR => {
                let e: ErrorMsg = frame.decode_json()?;
                return Err(Error::protocol(format!("主机报告错误：{}", e.message)));
            }
            other => {
                return Err(Error::protocol(format!(
                    "接收 {relative_path} 时收到意外的帧类型 {other}"
                )));
            }
        }
    }

    file.flush().await.map_err(|e| Error::io(part, e))?;
    file.sync_all().await.map_err(|e| Error::io(part, e))?;
    drop(file);

    if written != total_size {
        // 对端提前结束但没报错：保留检查点，下次续传
        state.upsert(PartialFile {
            relative_path: relative_path.to_string(),
            file_id: file_id.to_string(),
            partial_hash: Some(hex::encode(hasher.finalize().as_bytes())),
            total_size,
            partial: written,
            completed: false,
        });
        state.save(dest_dir)?;
        return Err(Error::Disconnected {
            received: written,
            total: total_size,
        });
    }

    // ---- 校验：只有哈希对得上才改名 ----
    let actual = fs_util::hash_file(part)?;
    let actual_hex = hex::encode(actual.as_bytes());
    if !actual_hex.eq_ignore_ascii_case(expected_blake3) {
        // 校验失败就删掉，不要留下一个"看起来完整"的坏文件
        let _ = std::fs::remove_file(part);
        state.upsert(PartialFile {
            relative_path: relative_path.to_string(),
            file_id: file_id.to_string(),
            total_size,
            partial_hash: None,
            partial: 0,
            completed: false,
        });
        state.save(dest_dir)?;
        return Err(Error::ChecksumMismatch {
            path: PathBuf::from(relative_path),
        });
    }

    fs_util::atomic_rename(part, target)?;
    state.upsert(PartialFile {
        relative_path: relative_path.to_string(),
        file_id: file_id.to_string(),
        partial_hash: None,
        total_size,
        partial: total_size,
        completed: true,
    });
    state.save(dest_dir)?;

    progress.send(ProgressEvent::FileFinished {
        file_id: file_id.to_string(),
        relative_path: relative_path.to_string(),
    });

    Ok(written - start_offset)
}

/// 带"对端消失"检查的读帧。
///
/// 为什么需要它：接收端进程被杀时，主机端的 `read_frame` 会一直阻塞在流的
/// 读上，**收不到**对端离开的通知（QUIC 的空闲超时要几十秒）。而主机只有
/// 从 `serve_connection` 返回后才能回到 `accept()` 接受新的连接。
/// 结果就是：接收端想重连续传，主机却在原地等三十多秒——续传等于不可用。
///
/// 解法：每次读最多等 500ms，超时就检查一次连接是否已经关闭，是则立刻返回。
async fn read_frame_watchdog(
    conn: &quinn::Connection,
    recv: &mut quinn::RecvStream,
) -> Result<Option<Frame>> {
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(500), read_frame(recv)).await {
            Ok(r) => return r,
            Err(_) => {
                if conn.close_reason().is_some() {
                    // 对端已经走了：让调用方按"正常收尾"处理
                    return Ok(None);
                }
            }
        }
    }
}
/// 判断一个错误是否只是"对端已经走了"。
///
/// 正常收尾（对端关闭连接/发送方向）在读侧表现为 connection lost 之类的
/// 传输层错误。把这种情况和真正的协议错误区分开，是"传输成功却报失败"
/// 这类令人困惑问题的根源之一。
fn is_peer_gone(e: &Error) -> bool {
    let text = e.to_string();
    text.contains("connection lost")
        || text.contains("ConnectionLost")
        || text.contains("closed")
        || text.contains("reset")
}
/// 判断一个连接错误是否源于"证书指纹不符"。
///
/// 特意做得宽松：不同 TLS 栈/版本把同一个原因表述成不同文字
/// （certificate / 证书 / invalid peer certificate / General(...)…），
/// 漏判的代价是要让用户白等一轮超时，而且会被当成"网络问题"重试。
/// 另外调用方还会用校验器的标记位兜底（那个比字符串可靠），两者取或。
fn is_fingerprint_mismatch(e: &quinn::ConnectionError) -> bool {
    let text = format!("{e}");
    text.contains("指纹")
        || text.contains("fingerprint")
        || text.contains("certificate")
        || text.contains("证书")
}

/// 等待主机对某个文件结果的确认，保持会话收发同步。
///
/// 主机在发完一个文件后会回一个 RESULT。接收端必须读掉它——否则下一个
/// OFFER 会和这个未读的 RESULT 错位，表现为"收到意外的帧类型"。
async fn drain_result(recv: &mut quinn::RecvStream, file_id: &str) -> Result<()> {
    loop {
        // 对端已经走了（文件其实已经收完，主机直接关了连接）：按正常结束处理
        let frame = match read_frame(recv).await {
            Ok(Some(f)) => f,
            Ok(None) => return Ok(()),
            Err(e) if is_peer_gone(&e) => return Ok(()),
            Err(e) => return Err(e),
        };

        match frame.kind {
            KIND_RESULT => {
                let r: TransferResult = frame.decode_json()?;
                if r.file_id != file_id {
                    return Err(Error::protocol(format!(
                        "确认帧的文件 ID 不匹配（期望 {file_id}，收到 {}）",
                        r.file_id
                    )));
                }
                return Ok(());
            }
            KIND_ERROR => {
                let e: ErrorMsg = frame.decode_json()?;
                return Err(Error::protocol(format!("主机报告错误：{}", e.message)));
            }
            // 主机在传完最后一个文件后可能直接发 BYE，这是合法的结束
            KIND_BYE => return Ok(()),
            other => {
                return Err(Error::protocol(format!(
                    "等待确认时收到意外的帧类型 {other}"
                )));
            }
        }
    }
}

/// 供发送端在会话前"预热"计划（扫描 + 哈希），CLI 用它显示总大小。
/// 一句话概括清单里有什么。文件和文本分开数——"1 个文件"和"1 段文本"对用户
/// 是两件很不一样的事，混着说会让人以为收到了个文件。
pub fn summarize_plan(plan: &TransferPlan) -> String {
    let files = plan
        .files
        .iter()
        .filter(|f| f.kind == ItemKind::File)
        .count();
    let texts = plan.files.len() - files;
    let bytes = plan.total_bytes;
    match (files, texts) {
        (0, t) => format!("{t} 段文本，共 {}", human_bytes(bytes)),
        (f, 0) => format!("{f} 个文件，共 {}", human_bytes(bytes)),
        (f, t) => format!("{f} 个文件 + {t} 段文本，共 {}", human_bytes(bytes)),
    }
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[i])
    }
}


/// 生成一个可粘贴的地址列表（放进终端输出，方便扫码失败时手输）。
pub fn addrs_display(addrs: &[AddressHint]) -> String {
    addrs
        .iter()
        .map(|a| a.display())
        .collect::<Vec<_>>()
        .join("  ")
}

/// 检查计划里是否有文件名会被接收端拒绝（提前发现，别等连上才失败）。
pub fn validate_plan_paths(plan: &TransferPlan) -> Result<()> {
    for f in plan.files.iter() {
        fs_util::safe_relative_path(&f.relative_path)?;
    }
    Ok(())
}



