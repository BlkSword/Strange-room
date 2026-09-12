//! `sr` —— Strange Room 的命令行客户端。
//!
//! v1 故意先做 CLI 而不是图形界面：文件传输、断点续传、协议正确性
//! 这些最难的部分必须能被**自动化测试**和**两台真实机器**反复验证，
//! 而不是靠"打开界面点一下看看"。CLI 同时是长期的测试资产。
//!
//! ```text
//! 主机（要分享文件的人）:  sr send ./photos --port 45001
//! 接收端（要拿文件的人）:  sr receive srx1:xxxx --to ./downloads
//! ```

mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sr_core::net::quic::{HostOptions, HostSession, Receiver, ReceiverOptions};
use sr_core::progress::ProgressSender;

use render::ProgressRenderer;

#[derive(Parser, Debug)]
#[command(
    name = "sr",
    version,
    about = "Strange Room：让同一房间里的设备共享文件。本地直连，不留痕。",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 分享文件：生成二维码，等待对方扫码接收
    Send {
        /// 要分享的文件或目录（可以多个）
        #[arg(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,

        /// 监听端口。默认随机分配一个空闲端口
        #[arg(long, default_value_t = 0)]
        port: u16,

        /// 本机设备名，对方会在界面上看到
        #[arg(long)]
        name: Option<String>,

        /// 传完一个连接就退出。默认关闭：接收端断线后要能重连续传，
        /// 主机如果只接一次，续传在真实使用中就等于不存在
        #[arg(long, default_value_t = false)]
        once: bool,

        /// 不渲染二维码图形，只打印连接串（适合脚本或远程终端）
        #[arg(long, default_value_t = false)]
        no_qr: bool,
    },

    /// 接收文件：扫码得到连接串后，粘贴进来开始接收
    Receive {
        /// 二维码里的连接串（srx1: 开头），由主机提供
        payload: String,

        /// 保存到哪个目录
        #[arg(long, short = 't', default_value = ".")]
        to: PathBuf,

        /// 本机设备名
        #[arg(long)]
        name: Option<String>,

        /// 不续传：忽略已有进度，从头开始
        #[arg(long, default_value_t = false)]
        no_resume: bool,
    },
}

fn device_name() -> String {
    std::env::var("SR_DEVICE_NAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "未命名设备".to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("无法初始化运行时：{e}");
            return ExitCode::FAILURE;
        }
    };
    let result = rt.block_on(run(cli));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\n错误：{e}");
            // 打印错误链，方便定位（anyhow 的 context 层次）
            let mut source = e.source();
            while let Some(s) = source {
                eprintln!("  ← {s}");
                source = s.source();
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Send {
            paths,
            port,
            name,
            once,
            no_qr,
        } => send(paths, port, name, once, no_qr).await,
        Command::Receive {
            payload,
            to,
            name,
            no_resume,
        } => receive(payload, to, name, no_resume).await,
    }
}

async fn send(
    paths: Vec<PathBuf>,
    port: u16,
    name: Option<String>,
    once: bool,
    no_qr: bool,
) -> Result<()> {
    let name = name.unwrap_or_else(device_name);

    println!("正在扫描文件并计算校验和……（大文件需要一点时间，但只需算一次）");
    let plan = sr_core::plan_paths(&paths).context("展开待发送文件失败")?;
    sr_core::net::quic::validate_plan_paths(&plan)?;

    let summary = sr_core::net::quic::summarize_plan(&plan);
    println!("准备分享：{summary}");

    let session = HostSession::start(HostOptions {
        plan,
        device_name: name.clone(),
        listen_port: port,
        session_id: None,
        once,
    })
    .await
    .context("启动监听失败")?;

    let payload = session.qr_payload()?;
    let encoded = payload.encode()?;

    render::print_banner(session.device_name(), &summary, session.port());
    render::print_qr(&encoded, no_qr);

    println!("\n等待对方接收……（在此终端按 Ctrl+C 可停止）");

    let progress = ProgressSender::new();
    let renderer = ProgressRenderer::spawn(progress.subscribe());

    // 循环接受连接，而不是只接一次：接收端可能因为断网、崩溃或用户
    // 主动中断而断开，之后它会重新扫码连接以续传。如果主机只 accept
    // 一次就停下，续传能力在真实使用中就等于不存在。
    let mut result = session.accept_once(&progress).await;
    let mut rounds = 1;
    while !once && rounds < 32 {
        // 单个会话失败（最常见的是接收端中途被关掉）不代表主机该停止服务。
        // 主机是"分享方"，它应该一直举着二维码等人来连——这也是续传能成立的
        // 前提。所以这里只提示，不退出。
        if let Err(e) = &result {
            eprintln!("上一个会话没有正常结束（{e}）。继续等待新的连接……");
        }
        println!("\n等待接收端重新连接以续传……（Ctrl+C 结束）");
        result = session.accept_once(&progress).await;
        rounds += 1;
    }

    // 收尾顺序很关键：必须先 drop 掉 sender，渲染线程的 blocking_recv
    // 才会收到 ChannelClosed 并退出；否则 finish() 会永远等在那里，
    // 整个程序看起来就是"传完了却卡住不退出"。
    drop(progress);
    // 再等渲染线程把队列里的事件画完，然后才打印最终结果，
    // 否则进度条会把结果文字冲掉
    renderer.finish();

    session.close();

    match result {
        Ok(s) => {
            println!(
                "\n传输完成：成功 {} 个文件，共 {}",
                s.files_sent,
                sr_core::net::quic::human_bytes(s.bytes_sent)
            );
            if !s.failures.is_empty() {
                println!("有 {} 个文件失败：", s.failures.len());
                for (p, e) in &s.failures {
                    println!("  - {p}: {e}");
                }
                anyhow::bail!("部分文件未能发送");
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

async fn receive(
    payload: String,
    to: PathBuf,
    name: Option<String>,
    no_resume: bool,
) -> Result<()> {
    let name = name.unwrap_or_else(device_name);
    let payload = sr_core::QrPayload::decode(&payload)
        .context("解析连接串失败")?;

    let dest = std::path::absolute(&to).unwrap_or(to);
    println!("目标目录：{}", dest.display());
    println!("正在连接主机 {} ……", payload.name);

    let progress = ProgressSender::new();
    let renderer = ProgressRenderer::spawn(progress.subscribe());

    // Ctrl+C 走"优雅停止"而不是直接被杀：已收的部分会保留，下次还能续传。
    // 直接杀进程会留下过期的检查点，下次要么整段重传，要么更糟——
    // 把不完整的数据当成完整的。
    let cancel = sr_core::CancelToken::new();
    {
        let c = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!();
                eprintln!("收到中断，正在停止……已接收的部分会保留，下次可以接着传");
                c.cancel();
            }
        });
    }

    let result = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dest.clone(),
            device_name: name,
            continue_partial: !no_resume,
            cancel,
        },
        &progress,
    )
    .await;

    // 必须先 drop sender，渲染线程才能从 blocking_recv 里退出，
    // 否则 finish() 永远等下去（这是之前"传完了却卡住"的真凶）
    drop(progress);
    renderer.finish();

    match result {
        Ok(s) => {
            println!(
                "\n接收完成：成功 {} 个文件，共 {}",
                s.files_sent,
                sr_core::net::quic::human_bytes(s.bytes_sent)
            );
            if !s.failures.is_empty() {
                println!("有 {} 个文件失败（其他文件不受影响）：", s.failures.len());
                for (p, e) in &s.failures {
                    println!("  - {p}: {e}");
                }
            }
            println!("文件已保存到：{}", dest.display());
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

