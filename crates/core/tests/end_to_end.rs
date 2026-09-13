//! 本文件使用 tests/common 中的辅助设施（含进程级身份目录隔离）。

//! 端到端集成测试：起两个**真实**的 QUIC 端点，真的传文件。
//!
//! 这些测试是整个内核存在的证明。单元测试能验证路径安全和协议编解码，
//! 但只有这里能回答"两个进程之间到底能不能把 2GB 文件可靠地传过去"。
//!
//! 每个测试都用独立的随机端口和临时目录，所以可以并行跑。

use std::path::{Path, PathBuf};
use std::time::Duration;

mod common;

use coalesce_core::net::quic::{HostOptions, HostSession, Receiver, ReceiverOptions, TransferSummary};
use coalesce_core::progress::ProgressSender;
use coalesce_core::qr::QrPayload;
use coalesce_core::{plan_paths, TransferPlan};

/// 建一个临时目录，返回 (守卫, 路径)。守卫 drop 时目录自动删除。
fn tmp() -> (tempfile::TempDir, PathBuf) { common::tmp() }

fn write_file(path: &Path, bytes: &[u8]) { common::write_file(path, bytes) }

/// 生成可复现但内容不像"全零"的测试数据，避免压缩/去重把问题掩盖掉。
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> { common::pseudo_random(len, seed) }

/// 起一个主机会话，拿到可以在本机连上的二维码载荷。
async fn start_host(plan: TransferPlan) -> HostSession {
    let session = HostSession::start(HostOptions {
        plan,
        device_name: "测试主机".to_string(),
        listen_port: 0, // 随机端口，避免测试之间抢端口
        session_id: None,
        once: true,
    })
    .await
    .expect("主机启动失败");

    // 测试都在本机跑，直接连回环，不用管真实网卡枚举出来的地址
    let _ = session;
    session
}

fn local_payload(session: &HostSession) -> QrPayload {
    QrPayload::new(
        session.session_id.clone(),
        session.device_name().to_string(),
        session.fingerprint().to_string(),
        vec![coalesce_core::AddressHint {
            host: "127.0.0.1".to_string(),
            port: session.port(),
        }],
    )
}

fn spawn_host_task(session: HostSession) -> tokio::task::JoinHandle<coalesce_core::Result<TransferSummary>> {
    let progress = ProgressSender::new();
    tokio::spawn(async move {
        // 给接收端一点时间先启动，避免时序上主机 accept 不到连接（实际不会丢，
        // QUIC 会在握手重传里等待，但这里还是保持简单）
        session.accept_once(&progress).await
    })
}

// ==================== 测试 ====================

/// 文本条目：和文件走同一套传输，但**不落盘**。
///
/// 这条守的是"房间里不只有文件"这件事：文本必须完整到达（内容经 BLAKE3 校验），
/// 而且不能变成磁盘上的文件——它是贴纸，不是文件。
#[tokio::test(flavor = "multi_thread")]
async fn transfers_a_text_item_alongside_a_file() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let data = pseudo_random(200_000, 5150);
    let file = src.join("payload.bin");
    write_file(&file, &data);

    // 一次同时发：一个文件 + 一段文本（链接）
    let mut plan = plan_paths(&[file]).unwrap();
    let text = "https://example.com/一个链接?带参数=1\n第二行：中文也要原样到达";
    coalesce_core::transfer::plan::append_text(&mut plan, "一个链接", text).unwrap();
    assert_eq!(plan.files.len(), 2, "清单里应当有一个文件 + 一段文本");

    let session = start_host(plan).await;
    let payload = local_payload(&session);
    let host = spawn_host_task(session);

    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "测试接收端".to_string(),
            continue_partial: true,
            cancel: coalesce_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("接收失败");

    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.files_sent, 1, "文件算一个");
    assert_eq!(summary.texts.len(), 1, "文本单独记账");
    assert_eq!(summary.texts[0].0, "一个链接", "来源说明要传过来");
    assert_eq!(summary.texts[0].1, text, "文本内容必须逐字节一致");

    // 文件正常落盘
    assert_eq!(std::fs::read(dst.join("payload.bin")).unwrap(), data);

    // 文本不该在磁盘上留下任何东西——这是"贴纸"和"文件"的分界线
    let mut entries: Vec<String> = std::fs::read_dir(&dst)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["payload.bin".to_string()],
        "文本不该变成文件：{entries:?}"
    );

    host.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn transfers_a_single_file_and_verifies_hash() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let data = pseudo_random(3 * 1024 * 1024 + 137, 42);
    let file = src.join("hello.bin");
    write_file(&file, &data);

    let plan = plan_paths(&[file]).unwrap();
    let session = start_host(plan).await;
    let payload = local_payload(&session);

    let host = spawn_host_task(session);

    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "测试接收端".to_string(),
            continue_partial: true,
            cancel: coalesce_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("接收失败");

    assert_eq!(summary.files_sent, 1);
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);

    let got = std::fs::read(dst.join("hello.bin")).expect("目标文件不存在");
    assert_eq!(got.len(), data.len());
    assert_eq!(got, data, "传输后的内容与源文件不一致");

    host.await.unwrap().unwrap();
}

/// 一个文件的目标路径写不进去时，**必须只跳过它，不能把整个会话带沟里**。
///
/// 这条用例守的是一个真实踩过的坑：接收端原来是在发完 OFFER 之后才建目录、
/// 预分配，于是"目标目录里有个同名文件"这类问题会在主机已经开始推数据之后才暴露。
/// 接收端记一笔失败接着协商下一个文件，可主机还在按自己的节奏推上一个文件的数据，
/// 那些数据帧就被当成控制帧解析——单流会话直接失步，后面本来没问题的文件也一起失败。
///
/// 修法是把本地准备提到发 OFFER 之前（见 `prepare_target`）。所以这里断言两件事：
/// 失败的那个文件被记下来，以及**同一个会话里另一个文件照常传完且内容正确**。
#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_destination_skips_that_file_without_breaking_the_session() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    // 一份要正常传完的文件，一份注定写不进去的
    let root = src.join("data");
    let good = pseudo_random(1_500_000, 99);
    write_file(&root.join("good/a.bin"), &good);
    write_file(&root.join("blocked/b.bin"), &pseudo_random(200_000, 100));

    // 目标目录里先放一个**文件**占住 blocked 这个位置：
    // 接收端要建的是目录，必然失败（Windows 报 os error 183，Unix 报 NotADirectory）
    std::fs::create_dir_all(dst.join("data")).unwrap();
    write_file(&dst.join("data/blocked"), "占位".as_bytes());

    let plan = plan_paths(std::slice::from_ref(&root)).unwrap();
    assert_eq!(plan.files.len(), 2);

    let session = start_host(plan).await;
    let payload = local_payload(&session);
    let host = spawn_host_task(session);

    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "r".into(),
            continue_partial: true,
            cancel: coalesce_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("一个文件写不进去不该让整次接收报错返回");

    assert_eq!(summary.failures.len(), 1, "应当恰好有一个文件失败：{:?}", summary.failures);
    assert!(
        summary.failures[0].0.contains("blocked"),
        "失败名单应当是写不进去的那个文件：{:?}",
        summary.failures
    );
    assert_eq!(summary.files_sent, 1, "另一个文件必须照常传完");

    // 关键：没被牵连的那个文件内容必须完好
    let got = std::fs::read(dst.join("data/good/a.bin")).expect("正常文件没收到");
    assert_eq!(got, good, "受牵连的文件内容不一致——会话很可能失步了");

    host.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn transfers_a_folder_preserving_structure() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let root = src.join("proj");
    write_file(&root.join("readme.txt"), b"top level");
    write_file(&root.join("sub/a.bin"), &pseudo_random(100_000, 7));
    write_file(&root.join("sub/deep/b.bin"), &pseudo_random(250_000, 8));
    write_file(&root.join("空目录占位/带中文 名字.txt"), "中文内容".as_bytes());

    let plan = plan_paths(std::slice::from_ref(&root)).unwrap();
    assert_eq!(plan.files.len(), 4);

    let session = start_host(plan).await;
    let payload = local_payload(&session);
    let host = spawn_host_task(session);

    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "r".into(),
            continue_partial: true,
            cancel: coalesce_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("接收失败");

    assert_eq!(summary.files_sent, 4);
    assert!(dst.join("proj/readme.txt").exists());
    assert!(dst.join("proj/sub/deep/b.bin").exists());
    assert!(dst.join("proj/空目录占位/带中文 名字.txt").exists());
    assert_eq!(
        std::fs::read(dst.join("proj/sub/a.bin")).unwrap(),
        std::fs::read(root.join("sub/a.bin")).unwrap()
    );

    host.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_connect_when_fingerprint_does_not_match() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let file = src.join("x.bin");
    write_file(&file, b"secret");
    let plan = plan_paths(&[file]).unwrap();

    let session = start_host(plan).await;
    // 篡改指纹：模拟"二维码被人换过"或"连到了冒充的主机"
    let mut payload = local_payload(&session);
    payload.fp = "f".repeat(coalesce_core::identity::FINGERPRINT_LEN * 2);

    // 主机必须真的在 accept：否则握手根本走不完，我们测到的就只是"超时"
    // 而不是"指纹被拒"——那样这个安全断言等于没测。
    let host = spawn_host_task(session);

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        Receiver::run(
            ReceiverOptions {
                payload,
                dest_dir: dst.clone(),
                device_name: "r".into(),
                continue_partial: true,
                cancel: coalesce_core::CancelToken::new(),
            },
            &ProgressSender::new(),
        ),
    )
    .await;

    match result {
        Ok(Ok(_)) => panic!("指纹不匹配却连接成功了，这是严重的安全问题"),
        Ok(Err(e)) => {
            let msg = e.to_string();
            // 提示必须能指导用户：明确指出证书/指纹问题，而不是笼统的"连接超时"
            assert!(
                msg.contains("指纹") || msg.contains("证书"),
                "错误提示应明确指出指纹/证书不匹配，实际是: {msg}"
            );
            // 必须是"秒级失败"，不能靠 8 秒握手超时兜底——否则用户要白等，
            // 而且真实原因（二维码过期/被换）会被误报成网络问题
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "指纹不匹配应当立刻失败，实际耗时 {:?}",
                started.elapsed()
            );
        }
        Err(_) => panic!("指纹不匹配应立刻失败，而不是挂到超时"),
    }

    // 目标目录里不应留下任何成品文件
    let leftovers: Vec<_> = std::fs::read_dir(&dst)
        .map(|it| it.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        leftovers.iter().all(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n == "resume.json" || n.ends_with(".part")
        }),
        "指纹校验失败后不应落盘任何成品文件: {leftovers:?}"
    );

    // 主机侧应当以握手失败告终（它的 accept 循环会继续等，所以不会返回）
    drop(host);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_files_are_handled() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let file = src.join("empty.txt");
    write_file(&file, b"");

    let plan = plan_paths(&[file]).unwrap();
    let session = start_host(plan).await;
    let payload = local_payload(&session);
    let host = spawn_host_task(session);

    let summary = Receiver::run(
        ReceiverOptions {
            payload,
            dest_dir: dst.clone(),
            device_name: "r".into(),
            continue_partial: true,
            cancel: coalesce_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("接收空文件失败");

    assert_eq!(summary.files_sent, 1);
    assert!(dst.join("empty.txt").exists());
    assert_eq!(std::fs::metadata(dst.join("empty.txt")).unwrap().len(), 0);

    host.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_a_stale_session_id() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let file = src.join("x.bin");
    write_file(&file, b"data");
    let plan = plan_paths(&[file]).unwrap();
    let session = start_host(plan).await;

    let mut payload = local_payload(&session);
    payload.sid = "已经完全过期的会话".to_string();

    // 主机必须真的在 accept，否则客户端只会等到超时，
    // 我们也就验证不到"主机拒绝过期会话"这条路径。
    let host = spawn_host_task(session);

    let result = tokio::time::timeout(
        Duration::from_secs(30),
        Receiver::run(
            ReceiverOptions {
                payload,
                dest_dir: dst.clone(),
                device_name: "r".into(),
                continue_partial: true,
                cancel: coalesce_core::CancelToken::new(),
            },
            &ProgressSender::new(),
        ),
    )
    .await;

    // 过期会话必须失败，并且错误提示要能指导用户"重新扫码"
    match result {
        Ok(Err(e)) => {
            let msg = e.to_string();
            assert!(
                msg.contains("过期") || msg.contains("会话"),
                "错误提示不够可操作（应提示二维码过期/重新扫码）: {msg}"
            );
        }
        Ok(Ok(_)) => panic!("过期会话不该成功"),
        Err(_) => panic!("过期会话不该挂住不返回"),
    }

    // 主机侧也应当以拒绝结束，而不是当作成功
    let host_result = host.await.unwrap();
    assert!(host_result.is_err(), "主机不该把过期会话当成成功");

    // 关键安全断言：拒绝之后，目标目录里不能有任何成品文件
    let finished = ["x.bin"]
        .iter()
        .filter(|n| dst.join(n).exists())
        .count();
    assert_eq!(finished, 0, "过期会话被拒后不应落盘任何文件");
}

/// 自检：面对一台**真的在运行**的主机，必须判定为可达。
///
/// 这条用例的价值在于：自检的探测逻辑和真正连接时用的是同一套握手，
/// 所以它顺带证明了"自检说能连 = 真能连"，而不是两套逻辑各说各话。
#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_reports_reachable_for_a_live_host() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let file = src.join("x.bin");
    write_file(&file, b"hello");
    let plan = plan_paths(&[file]).unwrap();

    let session = start_host(plan).await;
    let payload = local_payload(&session);
    let host = spawn_host_task(session);

    let d = coalesce_core::diag::diagnose(&payload).await.expect("自检本身不该失败");
    assert_eq!(
        d.verdict,
        coalesce_core::Verdict::Reachable,
        "对运行中的主机自检应判定可达，实际 {:?}：{}",
        d.verdict,
        d.summary
    );
    assert!(
        d.probes.iter().any(|p| p.outcome == coalesce_core::ProbeOutcome::Reachable),
        "至少要有一个地址探测成功"
    );
    let _ = tokio::time::timeout(Duration::from_secs(20), host).await;
}

/// 自检：面对一个不存在的地址，必须给出"不是可达"的结论，并且**附带建议**。
///
/// 只断言"不是 Reachable"而不是具体哪一种：不同系统对"往没人听的端口发 UDP"
/// 的反应不一样（有的回 ICMP 端口不可达，有的直接丢掉），
/// 结论落点会不同。这里关心的是"别把坏的报成好的"以及"要说人话"。
#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_on_a_dead_address_is_not_optimistic() {
    common::isolated_env();
    let payload = coalesce_core::QrPayload::new(
        "no-such-session",
        "不存在的主机",
        "f".repeat(coalesce_core::identity::FINGERPRINT_LEN * 2),
        vec![coalesce_core::AddressHint {
            host: "127.0.0.1".to_string(),
            port: 1,
        }],
    );

    let d = coalesce_core::diag::diagnose(&payload).await.expect("自检本身不该失败");
    assert_ne!(d.verdict, coalesce_core::Verdict::Reachable, "不该把不通的报成可达");
    assert!(
        !d.probes.is_empty() && d.probes.iter().all(|p| p.outcome != coalesce_core::ProbeOutcome::Reachable),
        "不该有地址被判为可达"
    );
    assert!(!d.advice.is_empty(), "结论之外必须给出可操作的建议");
    let text = d.render();
    assert!(text.contains("结论"), "报告应包含结论段");
}
