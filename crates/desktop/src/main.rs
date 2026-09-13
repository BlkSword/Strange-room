//! Chuanmen 桌面端（Tauri v2）。
//!
//! 这一层刻意做得**很薄**：所有传输逻辑都在 `chuanmen_core` 里，桌面端只做三件事：
//! 1. 把用户选中的路径交给内核，拿到二维码；
//! 2. 把内核的 `ProgressEvent` 翻译成界面用的 JSON 事件；
//! 3. 处理取消。
//!
//! 之所以能这么薄，是因为内核从第一天起就把进度模型做成了与 UI 无关的
//! `ProgressEvent`——CLI 用它画进度条，这里用它画界面，内核一行都不用改。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use chuanmen_core::discovery::Advertisement;
use chuanmen_core::net::quic::{
    human_bytes, summarize_plan, HostOptions, HostSession, Receiver, ReceiverOptions,
};
use chuanmen_core::progress::{ProgressEvent, ProgressSender};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

/// 前后端唯一的事件通道。
const EVT: &str = "transfer";

/// 分享开始后返回给界面的信息。
#[derive(Serialize, Clone)]
struct ShareInfo {
    /// 连接串，给"复制"按钮用
    payload: String,
    /// 二维码（SVG 源码，界面直接塞进 DOM）
    qr_svg: String,
    /// 形如"3 个文件，共 1.2 GB"，让用户确认自己选对了没有
    summary: String,
    file_count: usize,
    total_bytes: u64,
    /// 本机被扫的地址，扫码失败时让用户能手输
    addresses: Vec<String>,
    /// 这次也收东西时，落在哪个目录（None = 只往外发）
    incoming: Option<String>,
    /// 有没有广播到局域网。没有广播时对方搜不到这台设备，只能用二维码
    /// 或连接串——界面要说清楚，不能让用户对着"搜索附近设备"干等。
    advertised: bool,
}

/// 推给界面的进度事件。
///
/// 字段刻意用界面视角命名（path/done/total），而不是直接透传内核结构——
/// 这样以后内核调整字段，界面不用跟着改。
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum UiEvent {
    PeerConnected {
        peer: String,
        total_files: usize,
        total_bytes: u64,
    },
    /// resumed_from > 0 表示这次是续传
    FileStarted {
        path: String,
        size: u64,
        resumed_from: u64,
    },
    /// overall_* 是整体进度，界面不需要自己累加
    Progress {
        done: u64,
        total: u64,
        overall_done: u64,
        overall_total: u64,
    },
    FileFinished {
        path: String,
    },
    Done {
        files: usize,
        /// 收到的文本段数（不落盘，界面单独显示）
        texts: usize,
        bytes: u64,
        human_bytes: String,
        /// 没传完的条目数：界面要说出来，否则被打断的一次会话
        /// 会显示成"0 个文件"，看着像什么都没发生
        failures: usize,
    },
    /// 收到一段文本（平台化第一级）。它不是文件：界面把它当"贴纸"显示，
    /// 并提供一个复制按钮——用户的下一步动作几乎一定是"粘贴到别处"。
    TextReceived {
        label: String,
        text: String,
    },
    /// message 是给用户看的，必须可操作
    Failed {
        message: String,
    },
    Warn {
        message: String,
    },
}

#[derive(Default)]
struct AppState {
    /// 当前分享会话。取消 = 关掉它，`accept()` 立刻返回，循环退出。
    host: Mutex<Option<Arc<HostSession>>>,
    /// 当前分享的 mDNS 广播。Drop 就等于撤销广播，所以它必须和会话同生共死：
    /// 只放在局部变量里的话，`start_share` 一返回广播就没了。
    advertisement: Mutex<Option<Advertisement>>,
    /// 正在进行的接收。取消后已下载的部分会保留，下次可以续传。
    transfer: Mutex<Option<chuanmen_core::CancelToken>>,
}

/// 本机设备名：让对方在界面上知道连的是谁。
fn device_name() -> String {
    std::env::var("CHUAN_DEVICE_NAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "未命名设备".to_string())
}

/// 把二维码渲染成 SVG 源码，交给界面直接显示。
///
/// 刻意在 Rust 侧渲染：内核里已经有 `qrcode` 依赖，界面就不必再引一个
/// JS 二维码库（少一个依赖，也少一处可能与编码结果不一致的地方）。
fn qr_svg(data: &str) -> Result<String, String> {
    let code = qrcode::QrCode::new(data.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(260, 260)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#1a1a1a"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

/// 订阅内核进度并翻译成界面事件。
///
/// 顺带在 Rust 侧累加整体进度：界面只负责画，不做算术。
fn spawn_forwarder(app: AppHandle, mut rx: tokio::sync::broadcast::Receiver<ProgressEvent>) {
    tauri::async_runtime::spawn(async move {
        let mut done_by_file: HashMap<String, u64> = HashMap::new();
        let mut total_bytes = 0u64;

        while let Ok(ev) = rx.recv().await {
            let ui = match ev {
                ProgressEvent::SessionStarted {
                    peer,
                    total_files,
                    total_bytes: tb,
                } => {
                    total_bytes = tb;
                    done_by_file.clear();
                    UiEvent::PeerConnected {
                        peer,
                        total_files,
                        total_bytes: tb,
                    }
                }
                ProgressEvent::FileStarted {
                    file_id,
                    relative_path,
                    size,
                    resumed_from,
                } => {
                    done_by_file.insert(file_id, resumed_from);
                    UiEvent::FileStarted {
                        path: relative_path,
                        size,
                        resumed_from,
                    }
                }
                ProgressEvent::ChunkProgress {
                    file_id,
                    bytes_done,
                    bytes_total,
                } => {
                    done_by_file.insert(file_id, bytes_done);
                    let overall: u64 = done_by_file.values().sum();
                    UiEvent::Progress {
                        done: bytes_done,
                        total: bytes_total,
                        overall_done: overall,
                        overall_total: total_bytes.max(overall),
                    }
                }
                ProgressEvent::FileFinished { relative_path, .. } => {
                    UiEvent::FileFinished { path: relative_path }
                }
                ProgressEvent::SessionFinished {
                    files,
                    texts,
                    bytes,
                    failures,
                } => UiEvent::Done {
                    files,
                    texts,
                    bytes,
                    human_bytes: human_bytes(bytes),
                    failures,
                },
                ProgressEvent::TextReceived { label, text } => UiEvent::TextReceived { label, text },
                ProgressEvent::Warn(message) => UiEvent::Warn { message },
            };
            let _ = app.emit(EVT, ui);
        }
    });
}

/// 开始分享：扫描文件、开监听、显示二维码，并在后台循环接受连接。
#[tauri::command]
async fn start_share(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    text: Option<String>,
    incoming_dir: Option<String>,
) -> Result<ShareInfo, String> {
    let text = text.filter(|t| !t.trim().is_empty());
    let incoming = incoming_dir.filter(|d| !d.trim().is_empty());
    if paths.is_empty() && text.is_none() {
        return Err("还没有要分享的内容。把文件拖进窗口、粘贴路径，或者写一段文字/链接。".into());
    }

    // 清单：文件走路径展开，文本直接在内存里（和 CLI 的 --text 是同一条路）
    let mut plan = if paths.is_empty() {
        chuanmen_core::TransferPlan::default()
    } else {
        let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        // 扫描 + 算哈希可能要几秒（大文件），界面要给出等待提示
        chuanmen_core::plan_paths(&path_bufs).map_err(|e| e.to_string())?
    };
    if let Some(t) = text.as_deref() {
        let label = if t.trim_start().starts_with("http") {
            "一个链接"
        } else {
            "一段文本"
        };
        chuanmen_core::transfer::plan::append_text(&mut plan, label, t)
            .map_err(|e| e.to_string())?;
    }
    let summary = summarize_plan(&plan);
    let file_count = plan.files.len();
    let total_bytes = plan.total_bytes;

    let session = HostSession::start(HostOptions {
        plan,
        device_name: device_name(),
        listen_port: 0,
        session_id: None,
        once: false,
        // 对方也能往这里放东西：落在用户选的目录里（没选就是只往外发）
        incoming_dir: incoming.clone().map(PathBuf::from),
    })
    .await
    .map_err(|e| e.to_string())?;

    let payload = session.qr_payload().map_err(|e| e.to_string())?;
    let encoded = payload.encode().map_err(|e| e.to_string())?;
    let svg = qr_svg(&encoded)?;
    let addresses = payload.addrs.iter().map(|a| a.display()).collect::<Vec<_>>();

    // 广播到局域网：对方运行 `chuan receive`（或手机端）就能直接看到这台设备，
    // 不用扫码。失败不影响用二维码，所以只记下来给界面提示、不让分享失败。
    //
    // 真机验收时这里缺过一次：CLI 分享会广播、桌面分享不广播，于是"搜索附近
    // 设备"永远搜不到桌面端——同样的能力只在一端存在，是最容易漏的一类 bug。
    let advertised = match Advertisement::start(
        session.device_name(),
        &payload.sid,
        &payload.fp,
        session.port(),
        &payload.addrs,
    ) {
        Ok(ad) => {
            *state.advertisement.lock().unwrap() = Some(ad);
            true
        }
        Err(e) => {
            eprintln!("[桌面] 局域网广播没起来：{e}");
            false
        }
    };

    let session = Arc::new(session);
    *state.host.lock().unwrap() = Some(session.clone());

    let progress = ProgressSender::new();
    spawn_forwarder(app.clone(), progress.subscribe());

    // 循环接受连接：接收端断线后会重连续传，所以不能只接一次
    let app_loop = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match session.accept_once(&progress).await {
                Ok(s) => {
                    let _ = app_loop.emit(
                        EVT,
                        UiEvent::Done {
                            files: s.files_sent,
                            texts: s.texts.len(),
                            bytes: s.bytes_sent,
                            human_bytes: human_bytes(s.bytes_sent),
                            failures: s.failures.len(),
                        },
                    );
                    if !s.failures.is_empty() {
                        let _ = app_loop.emit(
                            EVT,
                            UiEvent::Warn {
                                message: format!("有 {} 个文件没能发送", s.failures.len()),
                            },
                        );
                    }
                }
                Err(e) => {
                    // 取消分享会走到这里（监听已关闭），不该报成错误
                    let msg = e.to_string();
                    if !msg.contains("监听已关闭") && !msg.contains("closed") {
                        let _ = app_loop.emit(EVT, UiEvent::Failed { message: msg });
                    }
                    break;
                }
            }
        }
    });

    Ok(ShareInfo {
        payload: encoded,
        qr_svg: svg,
        summary,
        file_count,
        total_bytes,
        addresses,
        incoming,
        advertised,
    })
}

/// 界面里「附近的设备」的一项。
///
/// 返回连接串而不是拆开的字段是刻意的：界面点一下就把连接串交给 `start_receive`，
/// 和扫码走**完全相同**的接收路径——少一套路径就少一半 bug。
#[derive(Serialize, Clone)]
struct NearbyDevice {
    name: String,
    /// 与主机屏幕一致的 6 位验证码
    code: String,
    /// 形如 `192.168.1.9:51234`，让人知道连的是哪个地址
    address: String,
    payload: String,
}

/// 搜索附近正在分享的设备（mDNS）。
///
/// 这是「零准备」那条路：对方只要在分享，这边不扫码、不粘贴就能连上。
/// 搜不到是常态之一（AP 隔离、禁组播、防火墙），所以界面必须同时保留
/// 粘贴连接串的入口——发现失败不该让用户卡在这一屏。
#[tauri::command]
async fn discover_hosts(
    state: State<'_, AppState>,
    timeout_secs: Option<u64>,
) -> Result<Vec<NearbyDevice>, String> {
    // 自己正在分享时，别把自己列进去。
    // 注意锁不能跨 await（MutexGuard 不是 Send），所以先取出 sid 再释放。
    let own_sid = state
        .host
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|host| host.qr_payload().ok())
        .map(|payload| payload.sid);

    let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(3).clamp(1, 10));
    let cancel = chuanmen_core::CancelToken::new();
    let hosts = chuanmen_core::discover(timeout, own_sid.as_deref(), &cancel)
        .await
        .map_err(|e| e.to_string())?;

    Ok(hosts
        .into_iter()
        .filter_map(|host| {
            // 生成不出连接串的设备直接跳过：点进去也只会得到一个看不懂的错误
            let payload = host.payload().encode().ok()?;
            Some(NearbyDevice {
                name: host.device_name.clone(),
                code: host.code.clone(),
                address: host
                    .addrs
                    .first()
                    .map(|a| format!("{}:{}", a.host, a.port))
                    .unwrap_or_else(|| "地址未知".to_string()),
                payload,
            })
        })
        .collect())
}

/// 开始接收：连上主机并下载全部文件。
#[tauri::command]
async fn start_receive(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: String,
    dest: String,
    send_paths: Vec<String>,
    text: Option<String>,
) -> Result<(), String> {
    let payload = chuanmen_core::QrPayload::decode(&payload).map_err(|e| e.to_string())?;
    let dest_dir = PathBuf::from(dest);

    // 我也要往房间里放东西（可选）：和 CLI 的 --send / --text 一一对应。
    // 注意：主机没开接收目录时会立刻被拒绝（内核会报"对方这次只往外分享"），
    // 不是等传一半才失败。
    let text = text.filter(|t| !t.trim().is_empty());
    let outgoing = if send_paths.is_empty() && text.is_none() {
        None
    } else {
        let mut plan = if send_paths.is_empty() {
            chuanmen_core::TransferPlan::default()
        } else {
            let bufs: Vec<PathBuf> = send_paths.iter().map(PathBuf::from).collect();
            chuanmen_core::plan_paths(&bufs).map_err(|e| e.to_string())?
        };
        if let Some(t) = text.as_deref() {
            let label = if t.trim_start().starts_with("http") {
                "一个链接"
            } else {
                "一段文本"
            };
            chuanmen_core::transfer::plan::append_text(&mut plan, label, t)
                .map_err(|e| e.to_string())?;
        }
        Some(plan)
    };

    let progress = ProgressSender::new();
    spawn_forwarder(app.clone(), progress.subscribe());

    let cancel = chuanmen_core::CancelToken::new();
    *state.transfer.lock().unwrap() = Some(cancel.clone());

    let app_done = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = Receiver::run(
            ReceiverOptions {
                payload,
                dest_dir,
                device_name: device_name(),
                continue_partial: true,
                outgoing,
                cancel,
            },
            &progress,
        )
        .await;

        let ui = match result {
            Ok(s) => UiEvent::Done {
                files: s.files_sent,
                texts: s.texts.len(),
                bytes: s.bytes_sent,
                human_bytes: human_bytes(s.bytes_sent),
                failures: s.failures.len(),
            },
            Err(e) => UiEvent::Failed {
                message: e.to_string(),
            },
        };
        let _ = app_done.emit(EVT, ui);
    });

    Ok(())
}

/// 打开系统文件/文件夹选择器。
///
/// `kind`：`"files"` 选多个文件，`"folder"` 选一个文件夹。
///
/// 为什么"能选"很重要：让用户手动打字输入路径，等于把最容易出错的一步
/// 交给用户自己扛（Windows 路径里的反斜杠、中文、长路径都很容易打错），
/// 而错一次就会得到"路径不存在"，用户不知道是自己打错了还是软件坏了。
///
/// 保留"粘贴路径"这条退路，是因为某些环境（远程桌面、受限权限）系统对话框
/// 可能打不开，那时用户还不至于完全没法用。
#[tauri::command]
async fn pick_paths(app: AppHandle, kind: String) -> Result<Vec<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();

    if kind == "folder" {
        app.dialog().file().pick_folder(move |picked| {
            let out = picked
                .and_then(|f| f.into_path().ok())
                .map(|pb| vec![pb.display().to_string()])
                .unwrap_or_default();
            let _ = tx.send(out);
        });
    } else {
        app.dialog().file().pick_files(move |picked| {
            let out = picked
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f.into_path().ok())
                .map(|pb| pb.display().to_string())
                .collect();
            let _ = tx.send(out);
        });
    }

    rx.await
        .map_err(|_| "选择器没有返回结果（系统对话框可能不可用，请改用粘贴路径）".to_string())
}

/// 网络自检：连不上时用它判断问题出在哪。
///
/// 返回的是给用户直接看的整段报告（结论 + 按可能性排序的建议）。
/// 放在 Rust 侧渲染，是为了让 CLI 与界面说同样的话——
/// 排查建议一旦两处不一致，用户就会更困惑。
#[tauri::command]
async fn diagnose_payload(payload: String) -> Result<String, String> {
    let d = chuanmen_core::diag::diagnose_str(&payload)
        .await
        .map_err(|e| e.to_string())?;
    Ok(d.render())
}

/// 停止正在进行的接收。
///
/// 走的是内核的协作式取消，不是杀进程：循环会在下一个安全检查点退出，
/// 此时已写入的数据是完整的、检查点是最新的——用户下次打开能接着传。
#[tauri::command]
fn cancel_transfer(state: State<'_, AppState>) {
    if let Some(t) = state.transfer.lock().unwrap().take() {
        t.cancel();
    }
}

/// 取消分享：关掉监听，accept 循环自己退出。
#[tauri::command]
fn cancel_share(state: State<'_, AppState>) {
    // 先撤广播再关会话：广播守卫一 drop，局域网里的设备立刻就搜不到了
    *state.advertisement.lock().unwrap() = None;
    if let Some(s) = state.host.lock().unwrap().take() {
        s.close();
    }
}

/// 先校验连接串像不像样，让界面立刻给反馈，而不是等连接失败。
///
/// 顺带把主机名和地址取出来显示——用户看到"正在连接 张三的笔记本"，
/// 比看到"正在连接"安心得多。
#[tauri::command]
fn inspect_payload(payload: String) -> Result<serde_json::Value, String> {
    let p = chuanmen_core::QrPayload::decode(&payload).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "host": p.name,
        "addresses": p.addrs.iter().map(|a| a.display()).collect::<Vec<_>>(),
    }))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_share,
            start_receive,
            cancel_share,
            cancel_transfer,
            diagnose_payload,
            discover_hosts,
            pick_paths,
            inspect_payload
        ])
        .run(tauri::generate_context!())
        .expect("启动 Chuanmen 失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 界面读的是 camelCase 字段（`humanBytes`、`totalFiles`…）。
    ///
    /// 真机验收抓到的 bug：`rename_all = "camelCase"` 只改**枚举变体名**，
    /// 不改变体内部的字段名。字段实际发出去是 `human_bytes`，界面拿到
    /// `undefined`，收尾摘要显示成"1 个文件，共 undefined"。这条测试把
    /// 线格式钉住，免得以后又悄悄变回 snake_case。
    #[test]
    fn ui_events_are_serialized_with_camel_case_fields() {
        let done = UiEvent::Done {
            files: 1,
            texts: 2,
            bytes: 3,
            human_bytes: "3 B".to_string(),
            failures: 1,
        };
        let json = serde_json::to_string(&done).unwrap();
        assert!(json.contains("\"kind\":\"done\""), "{json}");
        assert!(json.contains("\"humanBytes\":\"3 B\""), "{json}");
        assert!(!json.contains("human_bytes"), "{json}");

        let peer = UiEvent::PeerConnected {
            peer: "x".into(),
            total_files: 2,
            total_bytes: 10,
        };
        let json = serde_json::to_string(&peer).unwrap();
        assert!(json.contains("\"totalFiles\":2"), "{json}");
        assert!(json.contains("\"totalBytes\":10"), "{json}");
    }
}
