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


use crate::error::{Error, Result};
use crate::fs_util;
use crate::progress::{ProgressEvent, ProgressSender};
use crate::protocol::*;
use crate::qr::{AddressHint, QrPayload};
use crate::transfer::plan::TransferPlan;
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
}

pub struct HostSession {
    endpoint: quinn::Endpoint,
    identity: crate::identity::Identity,
    /// 会话开始时确定的文件清单。整个会话期间不变——这是"发送前先算好
    /// 全部哈希"这个决定带来的直接好处：传输期间不需要再回头读盘。
    plan: TransferPlan,
    device_name: String,
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
        let endpoint = quinn::Endpoint::server(quinn_cfg, bind)
            .map_err(|e| Error::protocol(format!("无法监听 {bind}（{e}）。请检查端口是否被占用")))?;
        let port = endpoint
            .local_addr()
            .map_err(|e| Error::protocol(format!("获取本地端口失败: {e}")))?
            .port();

        Ok(Self {
            endpoint,
            identity,
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
    pub files_sent: usize,
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
        })
        .collect();
    let manifest = FileManifest {
        files: entries,
        total_bytes: plan.total_bytes,
    };
    write_frame(&mut send, &Frame::json(KIND_MANIFEST, &manifest)?).await?;

    // ---- 3. 逐个文件：接收端主导 ----
    let mut summary = TransferSummary::default();
    loop {
        // 收尾健壮性：接收端传完后可能直接关闭连接，此处的读会以
        // "连接丢失"结束。传输其实已经成功完成，不该当成错误——否则
        // 用户会遇到"文件明明收好了却报失败"。
        let frame = match read_frame_watchdog(&conn, &mut recv).await {
            Ok(Some(f)) => f,
            Ok(None) => break, // 对端正常关闭发送方向
            Err(e) if is_peer_gone(&e) => break,
            Err(e) => return Err(e),
        };
        match frame.kind {
            KIND_BYE => break,
            KIND_OFFER => {
                let offer: FileOffer = frame.decode_json()?;
                match send_one_file(&mut send, plan, &offer, progress).await {
                    Ok(bytes) => {
                        summary.files_sent += 1;
                        summary.bytes_sent += bytes;
                        let _ = write_frame(
                            &mut send,
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
                            &mut send,
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

    progress.send(ProgressEvent::SessionFinished {
        files: summary.files_sent,
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

    let mut file = tokio::fs::File::open(&planned.source_path)
        .await
        .map_err(|e| Error::io(&planned.source_path, e))?;
    if start_offset > 0 {
        file.seek(std::io::SeekFrom::Start(start_offset))
            .await
            .map_err(|e| Error::io(&planned.source_path, e))?;
    }

    // 发送数据块。`chunk` 复用，避免每块都分配。
    let mut buf = vec![0u8; chunk_size];
    let mut sent = start_offset;
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
}

pub struct Receiver;

impl Receiver {
    /// 连接主机、接收全部文件。返回摘要。
    pub async fn run(opts: ReceiverOptions, progress: &ProgressSender) -> Result<TransferSummary> {
        tls::ensure_crypto_provider();
        let mut last_err: Option<Error> = None;
        let mut conn = None;

        // 外层重试：接收端被强杀时，主机要等到空闲超时才会发现，之后才回到
        // `accept()`。这中间有几秒钟"主机还没准备好再接一个"的窗口。
        // 续传的价值恰恰在"断线后还能接上"，所以这里必须耐心等，而不是
        // 失败一次就告诉用户"连不上"。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut round = 0u32;
        while conn.is_none() {
            round += 1;
            if round > 1 {
                println!("主机暂时还连不上，正在重试（第 {round} 次）……如果主机刚结束上一个会话，请稍候几秒");
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

                let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())
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
        let (conn, _endpoint) = conn.ok_or_else(|| {
            last_err.unwrap_or_else(|| Error::protocol("无法连接到主机，二维码里的地址都试过了"))
        })?;
        let _ = conn;

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

        // ---- 3. 准备续传状态 ----
        tokio::fs::create_dir_all(&opts.dest_dir)
            .await
            .map_err(|e| Error::io(&opts.dest_dir, e))?;
        let mut state = if opts.continue_partial {
            ResumeState::load(&opts.dest_dir)
        } else {
            ResumeState::new(opts.payload.sid.clone())
        };
        if state.session_id.is_empty() {
            state.session_id = opts.payload.sid.clone();
        }

        let mut summary = TransferSummary::default();
        for entry in &manifest.files {
            let rel = fs_util::safe_relative_path(&entry.relative_path)?;
            let target = fs_util::join_checked(&opts.dest_dir, &rel);
            let part = fs_util::part_path(&target);

            // 已完成的文件直接跳过（这才是"断点续传"里省时间的部分：
            // 重连后不重传已经收好的文件）
            let have = state.resume_offset(&entry.file_id, &part, entry.size);
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
                    continue;
                }
                // 校验没过：删掉重来
                let _ = std::fs::remove_file(&part);
            }

            let offer = FileOffer {
                file_id: entry.file_id.clone(),
                relative_path: entry.relative_path.clone(),
                size: entry.size,
                blake3: entry.blake3.clone(),
                have_bytes: have,
                chunk_size: server_hello.max_chunk_size,
            };
            write_frame(&mut send, &Frame::json(KIND_OFFER, &offer)?).await?;

            let frame = read_frame(&mut recv)
                .await?
                .ok_or_else(|| Error::protocol("主机在协商阶段断开"))?;
            frame.expect_kind(KIND_ACK, "续传确认")?;
            let ack: OfferAck = frame.decode_json()?;
            let start_offset = ack.start_offset.min(entry.size);

            let received = match receive_one_file(
                &mut recv,
                &entry.file_id,
                &entry.relative_path,
                &target,
                &part,
                entry.size,
                start_offset,
                &entry.blake3,
                &mut state,
                &opts.dest_dir,
                progress,
            )
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    let msg = e.to_string();
                    summary.failures.push((entry.relative_path.clone(), msg.clone()));
                    let _ = write_frame(
                        &mut send,
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
                    continue;
                }
            };

            summary.files_sent += 1;
            summary.bytes_sent += received;

            let _ = write_frame(
                &mut send,
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
            drain_result(&mut recv, &entry.file_id).await?;
        }

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

        // 全部成功时把续传状态文件删掉：它只是"下次能少传一点"的辅助信息，
        // 传完后留着既没有用，也和"不留痕"的定位不符（用户目录里平白多出一个
        // 看不懂的 json）。有失败时保留，方便同一会话内重试续传。
        if summary.failures.is_empty() {
            let _ = std::fs::remove_file(opts.dest_dir.join(RESUME_FILE));
        }

        progress.send(ProgressEvent::SessionFinished {
            files: summary.files_sent,
            bytes: summary.bytes_sent,
        });

        // 主动关闭，不去等 conn.closed()：等它只会一直拖到 QUIC 空闲超时。
        conn.close(0u32.into(), b"done");
        Ok(summary)
    }
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
) -> Result<u64> {
    progress.send(ProgressEvent::FileStarted {
        file_id: file_id.to_string(),
        relative_path: relative_path.to_string(),
        size: total_size,
        resumed_from: start_offset,
    });

    // 预分配：磁盘满要在开始传之前就暴露，而不是传到 90%
    // 父目录必须在这里创建：发送端的相对路径可能带目录（如 proj/sub/a.bin），
    // 而此前只有 dest_dir 被创建过。漏掉这一步会报 os error 3（找不到路径），
    // 而且它会被当成"单文件失败"跳过，进而让整个会话的帧收发错位。
    if let Some(parent) = part.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::io(parent, e))?;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(part)
        .await
        .map_err(|e| Error::io(part, e))?;

    {
        let target_len = total_size;
        let meta_len = file.metadata().await.map_err(|e| Error::io(part, e))?.len();
        if meta_len < target_len {
            file.set_len(target_len)
                .await
                .map_err(|e| Error::io(part, e))?;
        }
        // 从 0 重传时先把长度归零再展开，确保没有上次留下的尾巴
        if start_offset == 0 {
            file.set_len(0).await.map_err(|e| Error::io(part, e))?;
            file.set_len(total_size)
                .await
                .map_err(|e| Error::io(part, e))?;
        }
        file.seek(std::io::SeekFrom::Start(start_offset))
            .await
            .map_err(|e| Error::io(part, e))?;
    }

    state.upsert(PartialFile {
        relative_path: relative_path.to_string(),
        file_id: file_id.to_string(),
        total_size,
        partial: start_offset,
        // 起点来自对端协商，其可信性已由 resume_offset 的前缀校验保证；
        // 优先复用状态里已有的哈希，拿不到就按磁盘内容重算
        partial_hash: state
            .get(file_id)
            .and_then(|e| e.partial_hash.clone())
            .or_else(|| ResumeState::with_prefix_hash(part, start_offset)),
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
                        partial_hash: ResumeState::with_prefix_hash(part, written),
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
            partial_hash: ResumeState::with_prefix_hash(part, written),
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
pub fn summarize_plan(plan: &TransferPlan) -> String {
    let files = plan.files.len();
    let bytes = plan.total_bytes;
    format!("{files} 个文件，共 {}", human_bytes(bytes))
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


