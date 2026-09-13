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

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

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
    /// 不给子命令时按「接收」处理：这是下载了客户端之后最自然的动作
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 分享文件：屏幕出现二维码，同时向局域网广播（对方扫码、直接发现、或从引导页下载客户端都行）
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

    /// 接收文件：带上连接串就直接连，不带就先列出附近正在分享的设备让你挑
    Receive {
        /// 二维码/连接串（srx1: 开头）。省略时自动搜索同一 WiFi 下正在分享的设备
        payload: Option<String>,

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

    /// 看看附近有谁在分享（排查发现不到设备时用它；5 秒内出结果）
    Discover {
        /// 搜索多少秒。mDNS 的查询是退避重发的，太短会漏设备
        #[arg(long, default_value_t = 3)]
        timeout: u64,
    },

    /// 网络自检：连不上时用它判断问题出在哪（只发握手，不传输任何文件）
    Diagnose {
        /// 二维码里的连接串（srx1: 开头），由主机提供
        payload: String,
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
    let Some(command) = cli.command else {
        // 引导页下载下来的客户端就是靠这条路径「双击即可用」：
        // 不带参数 = 发现附近正在分享的设备并接收，不需要记任何命令。
        println!("（没有参数：按「接收」处理。想看全部用法用 sr --help）");
        return receive(None, PathBuf::from("."), None, false).await;
    };
    match command {
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
        Command::Discover { timeout } => discover(timeout).await,
        Command::Diagnose { payload } => diagnose(payload).await,
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

    // 广播到局域网：同一 WiFi 下的人运行 sr receive 就能直接看到这台设备，不用扫码。
    // 广播不出去不算致命（二维码/连接串照样能用），所以只提示、不中断。
    // 返回的守卫必须在整个分享期间活着：drop 就等于撤销广播。
    let _advertisement = match sr_core::discovery::Advertisement::start(
        session.device_name(),
        &payload.sid,
        &payload.fp,
        session.port(),
        &payload.addrs,
    ) {
        Ok(ad) => {
            println!(
                "
已广播到局域网：对方运行 sr receive 就能看到「{}」（验证码 {}，两边应当一致）",
                session.device_name(),
                sr_core::discovery::verification_code(&payload.fp)
            );
            Some(ad)
        }
        Err(e) => {
            println!("
（没能广播出去：{e}。不影响使用——让对方扫码或用连接串。）");
            None
        }
    };

    // 引导页：对方**还没有客户端**时的那条路。它服务的就是「当前这个可执行文件」
    // 本身——客户端自己分发自己，不需要额外的分发渠道，也不用联网下载。
    // 起不来不算致命（对方可能已经有客户端了），所以只提示。
    let _bootstrap = match std::env::current_exe().ok().and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => match sr_core::BootstrapServer::start(encoded.clone(), name.clone(), bytes).await {
            Ok(server) => {
                println!("
对方还没有客户端？让他在浏览器里打开：{}", server.url());
                println!("（页面里有下载按钮；下载后双击运行就是接收）");
                Some(server)
            }
            Err(e) => {
                println!("
（引导页没能起来：{e}。不影响传输：让对方用连接串。）");
                None
            }
        },
        None => None,
    };

    println!("\n等待对方接收……（在此终端按 Ctrl+C 可停止）");

    let progress = ProgressSender::new();
    let renderer = ProgressRenderer::spawn(progress.subscribe(), render::Role::Send);

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

/// 网络自检。
///
/// 存在的理由：局域网产品最常见的失败不是代码 bug，而是环境（不在同一 WiFi、
/// 访客网络开了 AP 隔离、防火墙拦了 UDP）。这些代码解决不了，只能给出
/// 可操作的指引——否则用户只看到一句"连接超时"就放弃了。
async fn diagnose(payload: String) -> Result<()> {
    println!("正在自检……会对二维码里的每个地址发一次握手，不传输任何文件
");
    let d = sr_core::diag::diagnose_str(&payload).await.context("自检失败")?;
    print!("{}", d.render());
    Ok(())
}

/// 看看附近有谁在分享。
///
/// 这既是功能，也是**排查工具**：mDNS 在真实网络里失效的方式太多（AP 隔离、
/// 禁组播、防火墙），与其猜，不如让它 5 秒给个明确结果。
async fn discover(timeout_secs: u64) -> Result<()> {
    let timeout = Duration::from_secs(timeout_secs.max(1));
    println!("正在搜索附近正在分享的设备（{} 秒）……", timeout.as_secs());

    let cancel = sr_core::CancelToken::new();
    let hosts = sr_core::discover(timeout, None, &cancel)
        .await
        .context("搜索附近设备失败")?;

    if hosts.is_empty() {
        // 找不到时给出的必须是"下一步做什么"，而不是一句"没找到"
        println!("附近没有找到正在分享的设备。");
        println!("mDNS 需要同时满足三条：两台设备在同一网段、网络允许组播、防火墙放行 UDP 5353。");
        println!("访客 WiFi 的 AP 隔离、部分企业网络、以及开着 VPN 时都会破坏其中一条。");
        println!("本机若有别的程序占着 UDP 5353（抓包工具、某些 VPN 客户端）也会导致搜不到。");
        println!("这些情况下请让对方把连接串发给你，用 `sr receive <连接串>` 接收。");
        anyhow::bail!("没有发现任何设备");
    }

    println!("找到 {} 台：", hosts.len());
    for host in &hosts {
        println!("  {}", host.display_line());
    }
    println!();
    println!("提示：验证码应当和对方屏幕上显示的一致；对不上就不要连。");
    Ok(())
}

/// 扫描附近设备并让用户挑一台（用于 `sr receive` 不带连接串的情况）。
///
/// 交互约定刻意做成"能自动就自动"：只找到一台就直接用——那是最常见的场景，
/// 没必要让人多按一次回车；找到多台才让人输序号。这样脚本里 `sr receive` 在
/// 只有一台设备时也能直接用，而多台时会明确报错，绝不"猜一个"然后传错机器。
async fn pick_nearby_host() -> Result<sr_core::NearbyHost> {
    let timeout = sr_core::discovery::DEFAULT_DISCOVERY_TIMEOUT;
    println!("正在搜索同一 WiFi 下正在分享的设备（{} 秒）……", timeout.as_secs());

    let cancel = sr_core::CancelToken::new();
    let hosts = sr_core::discover(timeout, None, &cancel)
        .await
        .context("搜索附近设备失败")?;

    if hosts.is_empty() {
        println!("没有找到正在分享的设备。可以检查：");
        println!("  · 两台设备是否连的是同一个网络（访客网络常开了 AP 隔离）");
        println!("  · 对方是否还在分享状态（`sr send` 关掉就不再广播）");
        println!("  · 防火墙是否放行 UDP 5353（mDNS）；本机有没有别的程序占着这个端口");
        println!("仍然不行时，让对方把连接串发给你，用 `sr receive <连接串>` 接收。");
        anyhow::bail!("没有找到正在分享的设备");
    }

    if hosts.len() == 1 {
        let host = hosts.into_iter().next().expect("已经判断过长度为 1");
        println!("找到一台：{}", host.display_line());
        return Ok(host);
    }

    println!("找到 {} 台设备：", hosts.len());
    for (i, host) in hosts.iter().enumerate() {
        println!("  {}) {}", i + 1, host.display_line());
    }
    print!("\n输入要连接的序号（直接回车取消）：");
    io::stdout().flush().ok();

    let mut line = String::new();
    io::stdin().read_line(&mut line).context("读取输入失败")?;
    let choice: usize = line
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("没有选择设备（需要输入序号）"))?;
    if choice == 0 {
        anyhow::bail!("序号从 1 开始");
    }
    hosts
        .into_iter()
        .nth(choice - 1)
        .ok_or_else(|| anyhow::anyhow!("没有第 {choice} 台设备"))
}

async fn receive(
    payload: Option<String>,
    to: PathBuf,
    name: Option<String>,
    no_resume: bool,
) -> Result<()> {
    let name = name.unwrap_or_else(device_name);

    // 没给连接串就走「发现」这条路：这是整个产品里最接近零准备的一步
    // （对方只要运行 sr receive，不用扫码、不用手输任何东西）。
    let payload = match payload {
        Some(p) => p,
        None => pick_nearby_host().await?.payload().encode()?,
    };

    let payload = sr_core::QrPayload::decode(&payload)
        .context("解析连接串失败")?;

    let dest = std::path::absolute(&to).unwrap_or(to);
    println!("目标目录：{}", dest.display());
    println!("正在连接主机 {} ……", payload.name);

    let progress = ProgressSender::new();
    let renderer = ProgressRenderer::spawn(progress.subscribe(), render::Role::Receive);

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
            println!("文件已保存到：{}", dest.display());
            if !s.failures.is_empty() {
                println!("有 {} 个文件失败（其他文件不受影响）：", s.failures.len());
                for (p, e) in &s.failures {
                    println!("  - {p}: {e}");
                }
                // 有文件没收到就必须以非零状态退出，否则脚本与自动化会把
                // "半失败"当成成功——上面那句"接收完成"也容易让人看漏。
                println!("已收到的部分保留在目标目录里；再运行一次同样的命令会接着传剩下的。");
                anyhow::bail!("部分文件未能接收");
            }
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

