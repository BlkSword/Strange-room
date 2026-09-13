//! 集成测试共用的辅助设施。
//!
//! 最重要的一条：**每个测试二进制进程使用独立的身份目录**。
//! 否则所有测试会争抢同一个身份文件，并发启动时可能读到"写了一半"的文件，
//! 各自生成不同指纹，握手时报"指纹不匹配"——表现为随机失败，极难排查。
//!
//! 其次：每个测试二进制都会单独编译本模块，并非每个 helper 都被用到，
//! 所以统一允许 dead_code，免得为"用了哪些 helper"反复调整。
#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<tempfile::TempDir> = OnceLock::new();

/// 初始化进程级隔离环境。每个测试都可以随便调用，只有第一次生效。
pub fn isolated_env() {
    DATA_DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("创建测试数据目录失败");
        // SAFETY：只在测试里调用，且通过 OnceLock 保证只执行一次，
        // 此时其它线程尚未开始读取该变量（测试函数都在初始化之后才启动端点）。
        unsafe {
            std::env::set_var("CHUAN_DATA_DIR", dir.path());
        }
        dir
    });
}

pub fn tmp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let path = dir.path().to_path_buf();
    (dir, path)
}

pub fn write_file(path: &std::path::Path, bytes: &[u8]) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

/// 可复现但不像"全零"的测试数据，避免压缩或去重把问题掩盖掉。
pub fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    for _ in 0..len {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        v.push((x >> 33) as u8);
    }
    v
}
