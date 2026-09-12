//! 传输吞吐基准。
//!
//! 目的：测出**实现本身**的上限（走回环，去掉 WiFi / 交换机这些变量）。
//!
//! 为什么需要它：当你在真实网速下觉得慢时，得先知道是"代码慢"还是"网络慢"。
//! 没有这个数，所有优化都是猜。它同时也是长期的回归护栏——
//! 以后任何一次改动如果让吞吐掉了，这里立刻能看出来。
//!
//! 用法：
//!   cargo run --release --example bench            # 默认 512 MiB
//!   cargo run --release --example bench -- 2048    # 指定 MiB

use std::time::Instant;

use sr_core::net::quic::{HostOptions, HostSession, Receiver, ReceiverOptions};
use sr_core::progress::ProgressSender;

fn human_mb_per_s(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        return 0.0;
    }
    (bytes as f64 / (1024.0 * 1024.0)) / secs
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let size_mib: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);

    let root = std::env::temp_dir().join(format!("sr-bench-{}", std::process::id()));
    let src = root.join("src");
    let dst = root.join("dst");
    std::fs::create_dir_all(&src)?;

    // 造数据。刻意用**不可压缩**的内容：如果全是零，压缩或去重会让结果虚高。
    let file = src.join("payload.bin");
    let total = size_mib * 1024 * 1024;
    println!("准备 {size_mib} MiB 测试数据…");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&file)?;
        let mut buf = vec![0u8; 1024 * 1024];
        let mut x: u64 = 0x243F_6A88_85A3_08D3;
        let mut written = 0u64;
        while written < total {
            for b in buf.iter_mut() {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *b = (x >> 33) as u8;
            }
            let n = buf.len().min((total - written) as usize);
            f.write_all(&buf[..n])?;
            written += n as u64;
        }
        f.sync_all()?;
    }

    // ── 阶段一：扫描与哈希（这一阶段不涉及网络，纯 CPU + 磁盘）──
    let t_plan = Instant::now();
    let plan = sr_core::plan_paths(std::slice::from_ref(&file))?;
    let plan_secs = t_plan.elapsed().as_secs_f64();
    println!(
        "扫描 + BLAKE3 哈希：{plan_secs:.2}s  →  {:.0} MiB/s（单线程）",
        human_mb_per_s(total, plan_secs)
    );

    // ── 阶段二：走回环传输 ──
    let session = HostSession::start(HostOptions {
        plan,
        device_name: "bench-host".into(),
        listen_port: 0,
        session_id: None,
        once: true,
    })
    .await?;
    let payload = sr_core::QrPayload::new(
        session.session_id.clone(),
        session.device_name().to_string(),
        session.fingerprint().to_string(),
        vec![sr_core::AddressHint {
            host: "127.0.0.1".into(),
            port: session.port(),
        }],
    );

    let host_progress = ProgressSender::new();
    let host = tokio::spawn(async move { session.accept_once(&host_progress).await });

    let progress = ProgressSender::new();
    let started = Instant::now();
    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "bench-recv".into(),
            continue_partial: true,
            cancel: sr_core::CancelToken::new(),
        },
        &progress,
    )
    .await?;
    let xfer_secs = started.elapsed().as_secs_f64();

    let sent = host.await??.bytes_sent;
    let rate = human_mb_per_s(sent, xfer_secs);
    println!(
        "回环传输：{xfer_secs:.2}s  →  {rate:.0} MiB/s（{:.2} GB/s）",
        rate / 1024.0
    );
    println!("（接收端校验通过：{} 个文件，{} 字节）", summary.files_sent, summary.bytes_sent);
    for (path, msg) in &summary.failures {
        println!("（失败：{path} —— {msg}）");
    }

    std::fs::remove_dir_all(&root).ok();
    Ok(())
}
