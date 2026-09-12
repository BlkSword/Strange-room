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

use sr_core::net::quic::{HostOptions, HostSession, Receiver, ReceiverOptions, TransferSummary};
use sr_core::progress::ProgressSender;
use sr_core::qr::QrPayload;
use sr_core::{plan_paths, TransferPlan};

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
        vec![sr_core::AddressHint {
            host: "127.0.0.1".to_string(),
            port: session.port(),
        }],
    )
}

fn spawn_host_task(session: HostSession) -> tokio::task::JoinHandle<sr_core::Result<TransferSummary>> {
    let progress = ProgressSender::new();
    tokio::spawn(async move {
        // 给接收端一点时间先启动，避免时序上主机 accept 不到连接（实际不会丢，
        // QUIC 会在握手重传里等待，但这里还是保持简单）
        session.accept_once(&progress).await
    })
}

// ==================== 测试 ====================

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
    payload.fp = "f".repeat(sr_core::identity::FINGERPRINT_LEN * 2);

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
