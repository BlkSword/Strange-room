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

/// 总体进度模型：把事件流折算成"总体已完成字节"。
///
/// 为什么单独抽成纯结构：这里出过一个真实的显示 bug——某个文件传完时它的进度条
/// 被从表里删掉，"各文件进度之和"就少了一份，于是多文件传输时总体进度条会**倒退**，
/// 按这个数算出来的速度还会变成负数。抽出来才能被单元测试按住。
#[derive(Default)]
struct ProgressModel {
    /// 已经传完的文件累计字节
    finished_bytes: u64,
    /// 正在传的文件：file_id → 当前已传字节
    active: HashMap<String, u64>,
}

impl ProgressModel {
    /// 文件开始（可能是续传，起点不为 0）
    fn start(&mut self, file_id: &str, resumed_from: u64) {
        self.active.insert(file_id.to_string(), resumed_from);
    }

    fn advance(&mut self, file_id: &str, bytes_done: u64) {
        self.active.insert(file_id.to_string(), bytes_done);
    }

    /// 文件传完：把它的字节**留在**总数里，而不是跟着进度条一起消失
    fn finish(&mut self, file_id: &str) {
        if let Some(n) = self.active.remove(file_id) {
            self.finished_bytes += n;
        }
    }

    fn done(&self) -> u64 {
        self.finished_bytes + self.active.values().sum::<u64>()
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
    let mut model = ProgressModel::default();
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
                model = ProgressModel::default();
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
                model.start(&file_id, resumed_from);
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
                model.advance(&file_id, bytes_done);
                let done = model.done();
                overall.set_position(done);
                // 速度取"本次会话的平均值"：瞬时速度在终端里跳得厉害，
                // 平均值既稳定又和下面的 ETA 用的是同一个口径。
                let elapsed = started.elapsed().as_secs_f64();
                let rate = if elapsed > 0.0 { done as f64 / elapsed } else { 0.0 };
                match eta_seconds(done, total_bytes, elapsed) {
                    Some(eta) if rate >= 1024.0 => {
                        overall.set_message(format!("{}/s · 剩 {:.0}s", human(rate as u64), eta))
                    }
                    _ => overall.set_message("估算中…"),
                }
            }
            ProgressEvent::FileFinished { file_id, .. } => {
                model.finish(&file_id);
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
    fn overall_progress_does_not_go_backwards_when_a_file_finishes() {
        let mut m = ProgressModel::default();
        m.start("a", 0);
        m.advance("a", 100);
        m.start("b", 0);
        m.advance("b", 30);
        assert_eq!(m.done(), 130);
        // 传完一个：进度条会被移除，但字节必须留在总体进度里
        m.finish("a");
        assert_eq!(m.done(), 130, "文件传完后它的字节必须继续计入总体进度");
        m.advance("b", 50);
        assert_eq!(m.done(), 150);
    }

    #[test]
    fn resumed_files_count_from_their_offset() {
        let mut m = ProgressModel::default();
        m.start("a", 700); // 续传：起点不是 0
        assert_eq!(m.done(), 700);
        m.advance("a", 1000);
        assert_eq!(m.done(), 1000);
        m.finish("a");
        assert_eq!(m.done(), 1000);
    }

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
