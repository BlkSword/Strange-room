//! 本文件使用 tests/common 中的辅助设施（含进程级身份目录隔离）。

//! 断点续传的端到端验证。
//!
//! 这是 v1 验收标准里的核心能力之一，必须用真实的 QUIC 传输来证明，
//! 而不是只测 `ResumeState` 的纯逻辑（那部分在单元测试里已覆盖）。
//!
//! 测试手法：先手工把目标目录布置成"上次传了一半"的状态（`.part` 文件
//! 里放着正确的前缀数据 + `resume.json` 记录偏移），然后发起一次完整会话，
//! 断言：
//! 1. 主机只补传了缺少的那一段（`bytes_sent == size - have`）；
//! 2. 已经有过的前缀没有被重写（用一个"不可能被重新生成"的模式来验证）；
//! 3. 最终文件内容与源文件完全一致。

use std::path::PathBuf;

mod common;

use chuanmen_core::net::quic::{HostOptions, HostSession, Receiver, ReceiverOptions};
use chuanmen_core::progress::{ProgressEvent, ProgressSender};
use chuanmen_core::qr::QrPayload;
use chuanmen_core::transfer::resume::{PartialFile, ResumeState};
use chuanmen_core::plan_paths;

fn tmp() -> (tempfile::TempDir, PathBuf) { common::tmp() }

fn pseudo_random(len: usize, seed: u64) -> Vec<u8> { common::pseudo_random(len, seed) }

async fn start_host(plan: chuanmen_core::TransferPlan) -> HostSession {
    HostSession::start(HostOptions {

        tcp_port: None,

        plan,
        device_name: "主机".to_string(),
        listen_port: 0,
        session_id: None,
        once: true,
        incoming_dir: None,
    })
    .await
    .expect("主机启动失败")
}

fn payload_for(session: &HostSession) -> QrPayload {
    QrPayload::new(
        session.session_id().to_string(),
        session.device_name().to_string(),
        session.fingerprint().to_string(),
        vec![chuanmen_core::AddressHint {
            host: "127.0.0.1".to_string(),
            port: session.port(),
        }],
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_from_the_offset_recorded_on_disk() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let total = 3 * 1024 * 1024usize; // 3 MiB
    let data = pseudo_random(total, 1234);
    let file = src.join("big.bin");
    std::fs::write(&file, &data).unwrap();

    let plan = plan_paths(&[file]).unwrap();
    // 先把需要的字段取出来，后面要把 plan 的所有权交给主机会话
    let rel_path = plan.files[0].relative_path.clone();
    let file_id = plan.files[0].file_id.clone();
    let entry_size = plan.files[0].size;
    // 见上面的 rel_path / file_id / entry_size

    // ---- 布置"上次传了一半"的现场 ----
    let have = 1024 * 1024usize; // 已收 1 MiB
    let target = dst.join(&rel_path);
    let part = chuanmen_core::fs_util::part_path(&target);
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();

    // 先写正确的前缀，再把后面填成"不可能被源数据生成"的字节（0xAB）。
    // 这样如果实现错误地从头重传，我们能立刻发现——因为前缀会被覆盖成源数据。
    let mut staged = vec![0u8; total];
    staged[..have].copy_from_slice(&data[..have]);
    for b in staged[have..].iter_mut() {
        *b = 0xAB;
    }
    std::fs::write(&part, &staged).unwrap();

    let mut state = ResumeState::new("previous-session");
    state.upsert(PartialFile {
        relative_path: rel_path.clone(),
        file_id: file_id.clone(),
        total_size: entry_size,
        partial: have as u64,
        // 记录前缀的哈希。接收端会实际核对磁盘上的前 have 字节，
        // 只有对得上才会从这里续传。
        partial_hash: Some(hex::encode(blake3::hash(&data[..have]).as_bytes())),
        completed: false,
    });
    state.save(&dst).unwrap();

    // ---- 发起一次真实会话 ----
    let session = start_host(plan).await;
    let payload = payload_for(&session);

    let host_progress = ProgressSender::new();
    let host_session = session;
    let host_task = tokio::spawn(async move { host_session.accept_once(&host_progress).await });

    // 观察接收端的 FileStarted 事件，确认它确实从 1 MiB 开始
    let cli_progress = ProgressSender::new();
    let mut cli_events = cli_progress.subscribe();
    let observer = tokio::spawn(async move {
        let mut resumed_from = None;
        while let Ok(ev) = cli_events.recv().await {
            if let chuanmen_core::ProgressEvent::FileStarted { resumed_from: r, .. } = ev {
                resumed_from = Some(r);
            }
        }
        resumed_from
    });

    // 注意：观察者任务持有 ProgressSender 的克隆，所以必须在接收结束后
    // 显式 drop 掉我们的 sender，否则 broadcast 通道永不关闭，observer 永不退出。
    let summary = Receiver::run(
        ReceiverOptions {force_tcp: false,

            payload,
            dest_dir: dst.clone(),
            device_name: "接收端".into(),
            continue_partial: true,
            outgoing: None,
            cancel: chuanmen_core::CancelToken::new(),
        },
        &cli_progress,
    )
    .await
    .expect("续传失败");
    drop(cli_progress);

    eprintln!("[t] 读文件前 exists={} len={:?}", target.exists(), std::fs::metadata(&target).map(|m| m.len()));
    let host_summary = host_task.await.unwrap().expect("主机侧失败");

    // ---- 断言 ----

    // 1. 最终内容必须与源文件逐字节一致
    let got = std::fs::read(&target).expect("目标文件不存在");
    assert_eq!(got.len(), total);
    assert_eq!(got, data, "续传后的文件内容与源文件不一致");

    // 2. 主机只补传了缺少的那一段
    let expected_delta = (total - have) as u64;
    assert_eq!(
        host_summary.bytes_sent, expected_delta,
        "主机应只补传 {expected_delta} 字节，实际传输了 {}",
        host_summary.bytes_sent
    );

    // 3. 接收端确实是从断点开始的，而不是从 0
    let observed = observer.await.unwrap();
    assert_eq!(
        observed,
        Some(have as u64),
        "接收端没有从已记录的断点 {have} 开始续传"
    );

    // 4. 临时文件已改名为正式文件；全部成功后不应残留续传状态文件
    //    （它只是辅助信息，留着既没用，也不符合"不留痕"的定位）
    assert!(!part.exists(), ".part 文件应在成功后改名为正式文件");
    assert!(
        !dst.join(chuanmen_core::transfer::resume::RESUME_FILE).exists(),
        "全部文件成功后不应残留续传状态文件"
    );
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
}

/// 检查点不可信时必须整文件重传。
///
/// 手法：记录声称已收 400 KiB，但磁盘上的 `.part` 是空的（或内容对不上），
/// 而且记录的"前缀哈希"与磁盘内容不一致。实现必须因此**从 0 重传**。
///
/// 这条用例守的是一个很容易被写错的地方：只比较"记录长度 vs 磁盘长度"是不够的，
/// 因为接收端会把 `.part` 预分配成完整大小——长度永远够。必须真的核对前缀内容。
#[tokio::test(flavor = "multi_thread")]
async fn distrusts_a_checkpoint_that_does_not_match_the_disk() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let total = 512 * 1024usize;
    let data = pseudo_random(total, 77);
    let file = src.join("f.bin");
    std::fs::write(&file, &data).unwrap();

    let plan = plan_paths(&[file]).unwrap();
    let rel_path = plan.files[0].relative_path.clone();
    let file_id = plan.files[0].file_id.clone();
    let entry_size = plan.files[0].size;

    // 布置现场：`.part` 长度足够（预分配过），但内容不是我们要的
    let target = dst.join(&rel_path);
    let part = chuanmen_core::fs_util::part_path(&target);
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, vec![0u8; total]).unwrap();

    let mut state = ResumeState::new("s");
    state.upsert(PartialFile {
        relative_path: rel_path.clone(),
        file_id: file_id.clone(),
        total_size: entry_size,
        partial: 400 * 1024,
        // 故意给一个与磁盘内容不符的哈希
        partial_hash: Some("ab".repeat(32)),
        completed: false,
    });
    state.save(&dst).unwrap();

    let session = start_host(plan).await;
    let payload = payload_for(&session);
    let host_task =
        tokio::spawn(async move { session.accept_once(&ProgressSender::new()).await });

    let summary = Receiver::run(
        ReceiverOptions {force_tcp: false,

            payload,
            dest_dir: dst.clone(),
            device_name: "r".into(),
            continue_partial: true,
            outgoing: None,
            cancel: chuanmen_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("接收失败");

    // 记录不可信 → 必须整文件重传，且结果正确
    assert_eq!(
        std::fs::read(&target).unwrap(),
        data,
        "不可信检查点必须导致重传，且最终内容正确"
    );
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);

    let host_summary = host_task.await.unwrap().unwrap();
    assert_eq!(
        host_summary.bytes_sent, total as u64,
        "不可信检查点必须导致整文件重传"
    );
}

/// 取消：必须立刻停下，而且**不能毁掉已收的进度**。
///
/// 这条用例守的是取消功能的真正价值。如果取消只是"把进程停掉"，用户会留下
/// 半截 `.part` 和过期检查点，下次要么整段重传、要么更糟——把不完整的数据
/// 当成完整的。所以这里同时断言三件事：返回得快、留下检查点、还能续传成功。
#[tokio::test(flavor = "multi_thread")]
async fn cancel_stops_promptly_and_keeps_progress_for_resume() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    // 造一个足够大的文件，保证取消发生在传输途中而不是结束后
    let total = 96 * 1024 * 1024usize;
    let data = pseudo_random(total, 4242);
    let file = src.join("big.bin");
    std::fs::write(&file, &data).unwrap();

    let plan = plan_paths(std::slice::from_ref(&file)).unwrap();
    let file_id = plan.files[0].file_id.clone();
    let rel_path = plan.files[0].relative_path.clone();

    let session = start_host(plan).await;
    let payload = payload_for(&session);
    let host = tokio::spawn(async move { session.accept_once(&ProgressSender::new()).await });

    // 150ms 后取消（此时应已传了一部分，但远没传完）
    let cancel = chuanmen_core::CancelToken::new();
    let cancel_later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        cancel_later.cancel();
    });

    let started = std::time::Instant::now();
    let result = Receiver::run(
        ReceiverOptions {force_tcp: false,

            payload,
            dest_dir: dst.clone(),
            device_name: "接收端".into(),
            continue_partial: true,
            outgoing: None,
            cancel,
        },
        &ProgressSender::new(),
    )
    .await;
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(chuanmen_core::Error::Cancelled)),
        "取消后应返回 Cancelled 错误，实际: {:?}",
        result.as_ref().err().map(|e| e.to_string())
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "取消应当很快生效，实际耗时 {elapsed:?}"
    );

    // 关键断言：进度必须留下，而且状态里记录的是"可信的前缀"
    let target = dst.join(&rel_path);
    let part = chuanmen_core::fs_util::part_path(&target);
    assert!(part.exists(), "取消后必须保留 .part，否则下次只能整段重传");

    let state = ResumeState::load(&dst);
    let entry = state.get(&file_id).expect("取消后必须留下检查点");
    assert!(!entry.completed, "取消时文件不该被标记为完成");

    // 检查点里的 `partial_hash` 必须**正好等于磁盘上前 `partial` 字节的 BLAKE3**。
    //
    // 续传能"只补传缺的那一段"，全靠这个等式成立：接收端下次会重算前缀哈希，
    // 和检查点里记的对得上才肯从半路接着收。这里曾经出过一次严重的性能事故——
    // 每个检查点都把整段前缀重读重算一遍（O(n²)，512MB 的文件要哈希 16GB 数据，
    // 吞吐被压到 50MB/s）。算法改成"边收边算、检查点只取快照"之后，这个等式
    // 就是正确性的关节：快照错一位，续传要么整段重传，要么更糟。
    let part_bytes = std::fs::read(&part).expect("读取 .part");
    let mut prefix = blake3::Hasher::new();
    prefix.update(&part_bytes[..entry.partial as usize]);
    assert_eq!(
        entry.partial_hash.as_deref(),
        Some(prefix.finalize().to_string().as_str()),
        "检查点哈希必须等于磁盘前缀的 BLAKE3，否则下次续传会被判定为不可信"
    );

    // 拿主机的这份任务收掉（它会因为对端断开而结束）
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), host).await;

    // ── 重新开一个会话续传 ──
    // 注意：换会话也能续，因为续传是按"相对路径派生出的 file_id"匹配的，
    // 而不是按会话 ID。这正是当初选择路径派生 ID 的原因。
    let session2 = start_host(plan_paths(std::slice::from_ref(&file)).unwrap()).await;
    let payload2 = payload_for(&session2);
    let host2 = tokio::spawn(async move { session2.accept_once(&ProgressSender::new()).await });

    let summary = Receiver::run(
        ReceiverOptions {force_tcp: false,

            payload: payload2,
            dest_dir: dst.clone(),
            device_name: "接收端".into(),
            continue_partial: true,
            outgoing: None,
            cancel: chuanmen_core::CancelToken::new(),
        },
        &ProgressSender::new(),
    )
    .await
    .expect("取消后应当还能续传成功");

    assert_eq!(
        std::fs::read(&target).unwrap(),
        data,
        "续传后的内容必须与源文件逐字节一致"
    );
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert!(!part.exists(), "续传成功后 .part 应已改名");

    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), host2).await;
}

/// 链路真的断掉时，接收端必须**自己接上续传**，而不是把"重跑一遍命令"丢给用户。
///
/// 这条守的是断点续传的"自动"那一半：进度留在盘上只是必要条件，用户还得手动重来
/// 就不算真的能用——WiFi 抖一下、笔记本合盖，谁都可能遇到。
///
/// 手法：接收端确实落盘 8MB 之后，主机**直接关掉自己**（等价于主机进程被杀、
/// 网线被拔），然后在同一个端口、同一个会话号上重新举二维码。断言：
/// 1. 没有任何失败；2. 最终文件逐字节一致；
/// 3. **补传的字节数小于文件总大小**——说明真的从断点续上了，而不是从头再传一遍。
///
/// 关于"同一个会话号"：真实重启不可能保留会话号，那时接收端必须重新扫码（已知限制）。
/// 这条测试盯的是**接收端那一侧**：连接被真的掐断之后，它会不会自己重连、会不会
/// 用上检查点。主机以同一个会话号重新起来，只是让这个场景可断言。
#[tokio::test(flavor = "multi_thread")]
async fn reconnects_and_resumes_after_a_real_interruption() {
    common::isolated_env();
    let (_src_guard, src) = tmp();
    let (_dst_guard, dst) = tmp();

    let total = 48 * 1024 * 1024usize;
    let data = pseudo_random(total, 31337);
    let file = src.join("big.bin");
    std::fs::write(&file, &data).unwrap();

    let plan = plan_paths(std::slice::from_ref(&file)).unwrap();
    let session = std::sync::Arc::new(start_host(plan).await);
    let payload = payload_for(&session);
    let port = session.port();
    let sid = session.session_id().to_string();

    // 第一台主机：后台接受连接。接受循环由第一次 accept_once 启动，
    // 所以这一句必须发出去——哪怕我们不关心它的结果。
    let first = tokio::spawn({
        let session = session.clone();
        async move { session.accept_once(&ProgressSender::new()).await }
    });

    // 关键的观察点：接收端**自己**的落盘进度。
    //
    // 不能看主机的发送进度：主机"已经写进 QUIC 流"不代表接收端已经读到——
    // 连接一断，还在缓冲区里的数据就没了，检查点会是 0，那样重连之后就是从 0
    // 重传，测的也不再是续传。阈值 8MB 既保证落盘，又给续传留足余额（总共 48MB）。
    let progress = ProgressSender::new();
    let mut watcher = progress.subscribe();
    let killer = tokio::spawn({
        let session = session.clone();
        async move {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                match tokio::time::timeout_at(deadline, watcher.recv()).await {
                    Ok(Ok(ProgressEvent::ChunkProgress { bytes_done, .. }))
                        if bytes_done >= 8 * 1024 * 1024 =>
                    {
                        break
                    }
                    Ok(Ok(_)) => continue,
                    _ => break,
                }
            }
            // 拔网线：不是优雅收尾，连接会直接断掉
            session.close();
        }
    });

    // 测试自己不再持有主机：`HostShared` 还活着的话，UDP 端口就不会释放，
    // 后面那台"同端口重新起来"的主机会 bind 失败（真实使用里 HostSession 被
    // drop 掉，端口自然就还回去了）。
    drop(session);

    // 第二台主机：同端口、同会话号，接着服务。
    // 端口刚释放时可能还被内核占着一小会儿，所以这里退避重试几次。
    let host2 = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        for _ in 0..30 {
            let plan = plan_paths(std::slice::from_ref(&file)).unwrap();
            let started = HostSession::start(HostOptions {
                plan,
                device_name: "测试主机".to_string(),
                listen_port: port,
                session_id: Some(sid.clone()),
                once: true,
                incoming_dir: None,
                tcp_port: None,
            })
            .await;
            match started {
                Ok(host) => return Ok(host.accept_once(&ProgressSender::new()).await),
                Err(e) => eprintln!("[test] 第二台主机启动失败：{e}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Err(chuanmen_core::Error::protocol(
            "端口一直没释放，测试起不了第二台主机",
        ))
    });

    let started = std::time::Instant::now();
    let summary = Receiver::run(
        ReceiverOptions {
            force_tcp: false,
            payload,
            dest_dir: dst.clone(),
            device_name: "接收端".into(),
            continue_partial: true,
            outgoing: None,
            cancel: chuanmen_core::CancelToken::new(),
        },
        &progress,
    )
    .await
    .expect("断线之后应当自动重连续传，而不是直接失败");
    let elapsed = started.elapsed();

    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.files_sent, 1, "应当是同一个文件被续传完成");
    assert!(
        summary.bytes_sent < total as u64,
        "重连之后应该只补差额，实际又传了 {} 字节（总共 {total} 字节）——说明没有真的续传，而是从头再传",
        summary.bytes_sent
    );
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "自动重连不该拖这么久（实际 {elapsed:?}）"
    );

    let got = std::fs::read(dst.join("big.bin")).expect("目标文件不存在");
    assert_eq!(got.len(), data.len(), "续传后的文件长度不对");
    assert_eq!(got, data, "重连续传后的内容必须逐字节一致");

    killer.await.ok();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), first).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), host2).await;
}
