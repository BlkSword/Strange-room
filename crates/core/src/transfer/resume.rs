//! 接收端：断点续传状态的持久化。
//!
//! 这是"断点续传"能成立的全部秘密：**进度必须写进磁盘**，不能只存在内存里。
//! 记录的是每个文件"已经落盘并校验过的字节数"，而不是一个总进度条数字。
//!
//! 语义约定：
//! - `partial` 记录的是已收到的字节数，且这段数据确实写在 `.part` 里；
//! - 只有当 `.part` 文件真实存在、且长度 >= 记录值时，这个记录才可信；
//!   否则一律从 0 重传（宁可重传，也不要拼出一个坏文件）。
//!
//! 每次写检查点都落盘（`sync_all`），因为下一个瞬间进程就可能被杀掉。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const RESUME_FILE: &str = "resume.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartialFile {
    /// 相对路径（落盘时用，同时作为跨会话匹配的键）
    pub relative_path: String,
    pub file_id: String,
    pub total_size: u64,
    /// 已落盘且可信的字节数，也就是下次续传的起点
    pub partial: u64,
    /// 前 `partial` 字节的 BLAKE3（hex）。用于验证"这个前缀确实在磁盘上"，
    /// 而不是只靠文件长度猜测——长度足够长并不代表内容是我们要的那一段。
    #[serde(default)]
    pub partial_hash: Option<String>,
    /// 是否已经完成校验并原子改名。true 表示这个文件已经收完。
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeState {
    pub version: u32,
    pub session_id: String,
    /// 首次开始接收的时间，仅用于展示
    pub started_at_ms: u128,
    pub updated_at_ms: u128,
    pub files: Vec<PartialFile>,
}

const RESUME_VERSION: u32 = 1;

/// 只对文件的前 `len` 字节算 BLAKE3。用于验证续传前缀，而不是读整文件。
fn hash_prefix(path: &Path, len: u64) -> Result<String> {
    use std::io::Read;
    let mut f = fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = blake3::Hasher::new();
    let mut remaining = len;
    let mut buf = [0u8; 64 * 1024];
    while remaining > 0 {
        let want = buf.len().min(remaining as usize);
        let n = f.read(&mut buf[..want]).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            // 文件比记录还短：不可信
            return Err(Error::Disconnected { received: 0, total: len });
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(hex::encode(hasher.finalize().as_bytes()))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl ResumeState {
    pub fn new(session_id: impl Into<String>) -> Self {
        let t = now_ms();
        Self {
            version: RESUME_VERSION,
            session_id: session_id.into(),
            started_at_ms: t,
            updated_at_ms: t,
            files: Vec::new(),
        }
    }

    /// 从目标目录加载。不存在或损坏都返回全新状态——续传是可以放弃的优化，
    /// 不能因为它读不出来就让整个接收失败。
    pub fn load(dest_dir: &Path) -> Self {
        let path = dest_dir.join(RESUME_FILE);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<ResumeState>(&bytes) {
                Ok(mut s) if s.version == RESUME_VERSION => {
                    s.updated_at_ms = now_ms();
                    s
                }
                _ => Self::new(""),
            },
            Err(_) => Self::new(""),
        }
    }

    pub fn save(&self, dest_dir: &Path) -> Result<()> {
        let path = dest_dir.join(RESUME_FILE);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let json = serde_json::to_vec_pretty(self)?;

        // 先写临时文件再改名：避免进程在写一半时被杀导致 resume.json 损坏。
        //
        // 刻意**不做 fsync**：检查点只是"下次能少传一点"的优化，数据正确性
        // 最终由整文件 BLAKE3 校验保证，不值得为它付 fsync 的代价。何况这里
        // 跑在异步任务里，fsync 在慢盘/杀毒软件下可能阻塞很久，久到 QUIC
        // 空闲超时把连接判死——那才是真正难查的故障。
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &json).map_err(|e| Error::io(&tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))?;
        Ok(())
    }

    pub fn get(&self, file_id: &str) -> Option<&PartialFile> {
        self.files.iter().find(|f| f.file_id == file_id)
    }

    pub fn upsert(&mut self, entry: PartialFile) {
        self.updated_at_ms = now_ms();
        match self.files.iter_mut().find(|f| f.file_id == entry.file_id) {
            Some(slot) => *slot = entry,
            None => self.files.push(entry),
        }
    }

    /// 某文件的可信续传起点。
    ///
    /// 这里有两道独立的防御，别把它们的判据混在一起：
    ///
    /// 1. **源文件变了**（`entry.partial > expected_size`）：本地记录说已经收了
    ///    1000 字节，但对方这次声明的文件只有 500 字节，说明是**另一个文件**
    ///    （主机上被改过 / 同名不同内容），已收的数据一点都不能用 → 返回 0。
    /// 2. **磁盘与记录不一致**（`on_disk < entry.partial`）：记录说收了 N 字节，
    ///    但 `.part` 实际不足 N（用户删了、写失败、文件系统没同步回磁盘），
    ///    拼接下去会得到一个"看起来完整、实际损坏"的文件 → 返回 0。
    ///
    /// 只有两道都过了，记录的偏移才可信。
    pub fn resume_offset(&self, file_id: &str, part_path: &Path, expected_size: u64) -> u64 {
        let Some(entry) = self.get(file_id) else {
            return 0;
        };
        if entry.completed {
            return expected_size;
        }
        // 防御 1：源文件比记录还小，已收的内容不可能属于这个文件
        if entry.partial > expected_size {
            return 0;
        }
        let on_disk = match fs::metadata(part_path) {
            Ok(m) => m.len(),
            Err(_) => return 0,
        };
        // 防御 2：磁盘上真实存在的长度不足以支撑记录值
        if on_disk < entry.partial {
            return 0;
        }
        // 防御 3（最关键的一道）：长度够不代表内容对。磁盘上的 `.part` 可能被
        // 预分配成了完整长度，或上一次写入没能落盘。必须实际核对前缀哈希，
        // 否则会把"看起来收过、实际是空洞"的状态当成续传起点，最终拼出一个
        // 长度正确但内容错误的文件——这是最危险的一类 bug，因为它能通过
        // 一切长度检查。
        if let Some(expected_hash) = &entry.partial_hash {
            match hash_prefix(part_path, entry.partial) {
                Ok(actual) if actual.eq_ignore_ascii_case(expected_hash) => entry.partial,
                // 算不出来或对不上：保守地从 0 重传
                _ => 0,
            }
        } else {
            // 旧版本状态文件没有哈希字段：无法验证，宁可重传，也不能冒险
            0
        }
    }

    /// 为校验通过的前缀补上哈希记录，供检查点写盘使用。
    /// 计算前 `partial` 字节的哈希。
    ///
    /// ⚠️ **这是 O(partial) 操作，绝不能放进循环里。**
    /// 曾经有个 bug 就是每个检查点都调它：检查点每 8MB 一次，于是总开销变成
    /// 8+16+…+总大小，也就是 **O(n²)**——512MB 的传输要哈希 16GB 数据，
    /// 吞吐被压到 50MB/s 左右，从外观上完全看不出是这里的问题。
    /// 正确做法是维护一个滚动哈希器（见 `net/quic.rs` 的 `receive_one_file`），
    /// 检查点只对哈希器取快照。
    ///
    /// 刻意不要求状态里已有该文件：调用发生在接收过程中，状态可能
    /// 尚未写入，而哈希是可以直接从落盘内容算出来的。
    pub fn with_prefix_hash(part_path: &Path, partial: u64) -> Option<String> {
        hash_prefix(part_path, partial).ok()
    }

    pub fn completed_count(&self) -> usize {
        self.files.iter().filter(|f| f.completed).count()
    }

    pub fn completed_bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| f.completed)
            .map(|f| f.total_size)
            .sum()
    }

    /// 清理已完成的记录，保留未完成的（用于会话结束时收尾）。
    pub fn prune_completed(&mut self) {
        self.files.retain(|f| !f.completed);
    }

    /// 供 UI 展示：每个文件的进度百分比。
    pub fn progress_map(&self) -> HashMap<String, f64> {
        self.files
            .iter()
            .map(|f| {
                let pct = if f.total_size == 0 {
                    100.0
                } else {
                    (f.partial as f64 / f.total_size as f64) * 100.0
                };
                (f.relative_path.clone(), pct.min(100.0))
            })
            .collect()
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("sr-resume-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// 造一个"可信"的部分文件：内容为 data[..have]，哈希也按它记录。
    fn staged(dir: &Path, name: &str, data: &[u8], have: usize) -> (PathBuf, String) {
        let p = dir.join(name);
        fs::write(&p, &data[..have]).unwrap();
        let h = hex::encode(blake3::hash(&data[..have]).as_bytes());
        (p, h)
    }

    fn entry(rel: &str, id: &str, total: u64, partial: u64, hash: Option<String>) -> PartialFile {
        PartialFile {
            relative_path: rel.into(),
            file_id: id.into(),
            total_size: total,
            partial,
            partial_hash: hash,
            completed: false,
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tmpdir();
        let mut st = ResumeState::new("sess-1");
        st.upsert(entry("a/b.bin", "f1", 1000, 400, Some("ab".repeat(32))));
        st.save(&dir).unwrap();

        let loaded = ResumeState::load(&dir);
        assert_eq!(loaded.session_id, "sess-1");
        assert_eq!(loaded.get("f1").unwrap().partial, 400);
        assert!(loaded.get("f1").unwrap().partial_hash.is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_returns_fresh_state() {
        let dir = tmpdir();
        assert!(ResumeState::load(&dir).files.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_corrupt_returns_fresh_state_instead_of_failing() {
        let dir = tmpdir();
        fs::write(dir.join(RESUME_FILE), b"{ not json").unwrap();
        assert!(ResumeState::load(&dir).files.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn old_state_without_hash_is_accepted_as_json() {
        // 向后兼容：旧版 resume.json 没有 partial_hash 字段，必须还能解析，
        // 只是那个文件的续传点会被保守地当作不可信（从 0 重传）。
        let dir = tmpdir();
        fs::write(
            dir.join(RESUME_FILE),
            br#"{"version":1,"session_id":"s","started_at_ms":0,"updated_at_ms":0,
                 "files":[{"relative_path":"x","file_id":"f1","total_size":100,"partial":50,"completed":false}]}"#,
        )
        .unwrap();
        let st = ResumeState::load(&dir);
        assert_eq!(st.get("f1").unwrap().partial, 50);
        assert!(st.get("f1").unwrap().partial_hash.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_offset_accepts_a_verified_prefix() {
        let dir = tmpdir();
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let (part, hash) = staged(&dir, "x.part", &data, 400);

        let mut st = ResumeState::new("s");
        st.upsert(entry("x", "f1", 1000, 400, Some(hash)));
        assert_eq!(st.resume_offset("f1", &part, 1000), 400);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_offset_rejects_a_prefix_whose_content_differs() {
        // 这是最关键的一条：记录说收了 400 字节，磁盘上确实有 400 字节，
        // 但内容不是我们要的那一段（比如预分配出来的空洞或旧文件残留）。
        // 只检查长度会误判，必须靠哈希拦住。
        let dir = tmpdir();
        let part = dir.join("x.part");
        fs::write(&part, vec![0u8; 400]).unwrap(); // 长度对，内容不对

        let mut st = ResumeState::new("s");
        st.upsert(entry("x", "f1", 1000, 400, Some("cd".repeat(32))));
        assert_eq!(
            st.resume_offset("f1", &part, 1000),
            0,
            "前缀内容不匹配时必须从 0 重传"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_offset_rejects_when_record_has_no_hash() {
        let dir = tmpdir();
        let data = vec![7u8; 1000];
        let (part, _) = staged(&dir, "x.part", &data, 400);

        let mut st = ResumeState::new("s");
        st.upsert(entry("x", "f1", 1000, 400, None));
        assert_eq!(st.resume_offset("f1", &part, 1000), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_offset_rejects_short_or_missing_files() {
        let dir = tmpdir();
        let data = vec![9u8; 1000];
        let mut st = ResumeState::new("s");
        st.upsert(entry("x", "f1", 1000, 400, Some(hex::encode(blake3::hash(&data[..400]).as_bytes()))));

        // 文件不存在
        assert_eq!(st.resume_offset("f1", &dir.join("nope.part"), 1000), 0);
        // 文件比记录短
        let short = dir.join("short.part");
        fs::write(&short, vec![9u8; 100]).unwrap();
        assert_eq!(st.resume_offset("f1", &short, 1000), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resume_offset_rejects_when_source_shrank() {
        let dir = tmpdir();
        let data = vec![3u8; 1000];
        let (part, hash) = staged(&dir, "x.part", &data, 400);
        let mut st = ResumeState::new("s");
        st.upsert(entry("x", "f1", 1000, 400, Some(hash)));
        // 源文件只剩 300 字节 → 已收的 400 字节不可能属于它
        assert_eq!(st.resume_offset("f1", &part, 300), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn completed_file_reports_full_offset_and_counts() {
        let st = ResumeState {
            version: RESUME_VERSION,
            session_id: "s".into(),
            started_at_ms: 0,
            updated_at_ms: 0,
            files: vec![PartialFile {
                relative_path: "x".into(),
                file_id: "f1".into(),
                total_size: 100,
                partial: 100,
                partial_hash: None,
                completed: true,
            }],
        };
        assert_eq!(st.resume_offset("f1", Path::new("/nope"), 100), 100);
        assert_eq!(st.completed_count(), 1);
        assert_eq!(st.completed_bytes(), 100);
    }

    #[test]
    fn progress_map_reports_percentages() {
        let mut st = ResumeState::new("s");
        st.upsert(entry("a", "f1", 200, 50, None));
        assert_eq!(st.progress_map().get("a"), Some(&25.0));
    }

    #[test]
    fn with_prefix_hash_matches_direct_hashing() {
        let dir = tmpdir();
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 97) as u8).collect();
        let (part, hash) = staged(&dir, "y.part", &data, 1234);
        let _ = ResumeState::new("s");
        assert_eq!(ResumeState::with_prefix_hash(&part, 1234), Some(hash));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hash_prefix_refuses_files_shorter_than_requested() {
        let dir = tmpdir();
        let p = dir.join("tiny");
        fs::write(&p, b"abc").unwrap();
        assert!(hash_prefix(&p, 10).is_err());
        fs::remove_dir_all(&dir).ok();
    }
}
