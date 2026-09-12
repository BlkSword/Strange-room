//! Strange Room 桌面端（Tauri v2）。
//!
//! 这一层刻意做得**很薄**：所有传输逻辑都在 `sr_core` 里，桌面端只做三件事：
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
use sr_core::net::quic::{
    human_bytes, summarize_plan, HostOptions, HostSession, Receiver, ReceiverOptions,
};
use sr_core::progress::{ProgressEvent, ProgressSender};
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
}

/// 推给界面的进度事件。
///
/// 字段刻意用界面视角命名（path/done/total），而不是直接透传内核结构——
/// 这样以后内核调整字段，界面不用跟着改。
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase")]
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
        bytes: u64,
        human_bytes: String,
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
    /// 正在进行的接收。取消后已下载的部分会保留，下次可以续传。
    transfer: Mutex<Option<sr_core::CancelToken>>,
}

/// 本机设备名：让对方在界面上知道连的是谁。
fn device_name() -> String {
    std::env::var("SR_DEVICE_NAME")
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
                ProgressEvent::SessionFinished { files, bytes } => UiEvent::Done {
                    files,
                    bytes,
                    human_bytes: human_bytes(bytes),
                },
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
) -> Result<ShareInfo, String> {
    if paths.is_empty() {
        return Err("还没有选择要分享的文件。把文件拖进窗口，或直接粘贴路径。".into());
    }
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();

    // 扫描 + 算哈希可能要几秒（大文件），界面要给出等待提示
    let plan = sr_core::plan_paths(&path_bufs).map_err(|e| e.to_string())?;
    let summary = summarize_plan(&plan);
    let file_count = plan.files.len();
    let total_bytes = plan.total_bytes;

    let session = HostSession::start(HostOptions {
        plan,
        device_name: device_name(),
        listen_port: 0,
        session_id: None,
        once: false,
    })
    .await
    .map_err(|e| e.to_string())?;

    let payload = session.qr_payload().map_err(|e| e.to_string())?;
    let encoded = payload.encode().map_err(|e| e.to_string())?;
    let svg = qr_svg(&encoded)?;
    let addresses = payload.addrs.iter().map(|a| a.display()).collect::<Vec<_>>();

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
                            bytes: s.bytes_sent,
                            human_bytes: human_bytes(s.bytes_sent),
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
    })
}

/// 开始接收：连上主机并下载全部文件。
#[tauri::command]
async fn start_receive(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: String,
    dest: String,
) -> Result<(), String> {
    let payload = sr_core::QrPayload::decode(&payload).map_err(|e| e.to_string())?;
    let dest_dir = PathBuf::from(dest);

    let progress = ProgressSender::new();
    spawn_forwarder(app.clone(), progress.subscribe());

    let cancel = sr_core::CancelToken::new();
    *state.transfer.lock().unwrap() = Some(cancel.clone());

    let app_done = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = Receiver::run(
            ReceiverOptions {
                payload,
                dest_dir,
                device_name: device_name(),
                continue_partial: true,
                cancel,
            },
            &progress,
        )
        .await;

        let ui = match result {
            Ok(s) => UiEvent::Done {
                files: s.files_sent,
                bytes: s.bytes_sent,
                human_bytes: human_bytes(s.bytes_sent),
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
    let d = sr_core::diag::diagnose_str(&payload)
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
    let p = sr_core::QrPayload::decode(&payload).map_err(|e| e.to_string())?;
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
            pick_paths,
            inspect_payload
        ])
        .run(tauri::generate_context!())
        .expect("启动 Strange Room 失败");
}
