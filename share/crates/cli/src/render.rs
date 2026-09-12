//! 终端渲染：二维码、横幅、进度条。
//!
//! 这一层的存在意义是证明 `ProgressEvent` 这套与 UI 无关的进度模型够用。
//! 将来 Tauri 界面消费的是同一批事件，只是渲染方式不同。

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use sr_core::progress::{eta_seconds, percent, ProgressEvent};

/// 打印启动横幅，把"怎么用"直接写在屏幕上，减少口头解释。
pub fn print_banner(device_name: &str, summary: &str, port: u16) {
    println!();
    println!("┌─────────────────────────────────────────────┐");
    println!("│  Strange Room · 正在等待接收                │");
    println!("└─────────────────────────────────────────────┘");
    println!("  设备：{device_name}");
    println!("  内容：{summary}");
    println!("  端口：{port}");
    println!();
}

/// 打印二维码。`no_qr` 为真时只打印连接串（方便脚本 / 远程终端）。
pub fn print_qr(payload: &str, no_qr: bool) {
    if no_qr {
        println!("连接串（在接收端执行）：");
        println!("  sr receive {payload}");
        return;
    }
    match qrcode::QrCode::new(payload.as_bytes()) {
        Ok(code) => {
            // 用方块字符渲染，终端兼容性最好
            let rendered = code
                .render::<char>()
                .quiet_zone(true)
                .module_dimensions(2, 1)
                .dark_color('#')
                .light_color(' ')
                .build();
            println!("{rendered}");
            println!("  对方扫码，或在接收端执行：");
            println!("  sr receive {payload}");
        }
        Err(e) => {
            // 二维码渲染失败不该阻断传输：连接串照样能用
            eprintln!("（二维码生成失败：{e}；请使用下方连接串）");
            println!("  sr receive {payload}");
        }
    }
}

/// 后台线程消费进度事件并渲染进度条。
///
/// 刻意做成"独立线程 + channel"：渲染绝不阻塞传输主循环，
/// 慢终端不会拖慢文件传输。
pub struct ProgressRenderer {
    handle: Option<thread::JoinHandle<()>>,
}

impl ProgressRenderer {
    pub fn spawn(rx: tokio::sync::broadcast::Receiver<ProgressEvent>) -> Self {
        let handle = thread::spawn(move || render_loop(rx));
        Self {
            handle: Some(handle),
        }
    }

    /// 等待渲染线程自然结束（通道关闭后它会退出），确保最后的输出完整。
    pub fn finish(mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn render_loop(mut rx: tokio::sync::broadcast::Receiver<ProgressEvent>) {
    let multi = MultiProgress::new();
    let overall = multi.add(ProgressBar::new(0));
    if let Ok(style) = ProgressStyle::with_template(
        "{spinner} 总体 [{bar:32}] {percent}% · {bytes}/{total_bytes} · {msg}",
    ) {
        overall.set_style(style.progress_chars("=> "));
    }
    overall.enable_steady_tick(Duration::from_millis(200));

    let mut files: HashMap<String, ProgressBar> = HashMap::new();
    let mut started = Instant::now();
    let mut total_bytes = 0u64;

    // 通道关闭（所有发送端 drop）时 blocking_recv 返回 Err，循环自然退出；
    // Lagged 说明消费者太慢，丢的是过期进度，继续读最新值即可
    'drain: loop {
    let event = match rx.blocking_recv() {
        Ok(e) => e,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue 'drain,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break 'drain,
    };
    match event {
            ProgressEvent::SessionStarted {
                peer,
                total_files,
                total_bytes: tb,
            } => {
                total_bytes = tb;
                started = Instant::now();
                multi.suspend(|| println!("已连接到 {peer}，开始接收 {total_files} 个文件"));
                overall.set_length(tb);
            }
            ProgressEvent::FileStarted {
                file_id,
                relative_path,
                size,
                resumed_from,
            } => {
                let label = if resumed_from > 0 {
                    format!("{relative_path}（从 {} 续传）", human(resumed_from))
                } else {
                    relative_path
                };
                let bar = multi.add(ProgressBar::new(size));
                if let Ok(style) = ProgressStyle::with_template(
                    "  {msg:36} [{bar:26}] {percent}% {bytes}/{total_bytes}",
                ) {
                    bar.set_style(style.progress_chars("=> "));
                }
                bar.set_message(label);
                bar.set_position(resumed_from);
                files.insert(file_id, bar);
            }
            ProgressEvent::ChunkProgress {
                file_id,
                bytes_done,
                bytes_total,
            } => {
                if let Some(bar) = files.get(&file_id) {
                    bar.set_length(bytes_total);
                    bar.set_position(bytes_done);
                }
                // 总体进度 = 各文件当前进度之和
                let done: u64 = files.values().map(|b| b.position()).sum();
                overall.set_position(done);
                match eta_seconds(done, total_bytes, started.elapsed().as_secs_f64()) {
                    Some(eta) => overall.set_message(format!("剩 {:.0}s", eta)),
                    None => overall.set_message("估算中…"),
                }
            }
            ProgressEvent::FileFinished { file_id, .. } => {
                if let Some(bar) = files.remove(&file_id) {
                    bar.finish_and_clear();
                }
            }
            ProgressEvent::SessionFinished { files, bytes } => {
                overall.set_position(bytes);
                overall.finish_and_clear();
                let secs = started.elapsed().as_secs_f64();
                let rate = if secs > 0.0 { bytes as f64 / secs } else { 0.0 };
                multi.suspend(|| {
                    println!(
                        "完成 {files} 个文件，{}，用时 {:.1}s（平均 {}/s）",
                        human(bytes),
                        secs,
                        human(rate as u64)
                    )
                });
            }
            ProgressEvent::Warn(msg) => {
                multi.suspend(|| eprintln!("  ! {msg}"));
            }
        }
    }
    }

fn human(n: u64) -> String {
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
        format!("{v:.1} {}", UNITS[i])
    }
}

/// 纯函数，便于测试：把字节进度换算成百分比。
#[cfg_attr(not(test), allow(dead_code))]
pub fn overall_percent(done: u64, total: u64) -> f64 {
    percent(done, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_formats_units() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1023), "1023 B");
        assert_eq!(human(1024), "1.0 KB");
        assert_eq!(human(1024 * 1024), "1.0 MB");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn overall_percent_is_clamped() {
        assert_eq!(overall_percent(0, 0), 100.0);
        assert_eq!(overall_percent(50, 100), 50.0);
        assert_eq!(overall_percent(200, 100), 100.0);
    }

    #[test]
    fn renderer_exits_when_channel_closes() {
        // 通道关闭后渲染线程必须退出，否则 CLI 会挂住
        let (tx, rx) = tokio::sync::broadcast::channel::<ProgressEvent>(4);
        let renderer = ProgressRenderer::spawn(rx);
        drop(tx);
        renderer.finish();
    }
}
