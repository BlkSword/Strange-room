//! 文件系统工具：路径安全、空间预留、原子改名。
//!
//! 这一层是 v1 最容易翻车的地方。三条硬规则：
//! 1. 接收端落盘的**任何**路径都必须先过 `safe_relative_path`；
//! 2. 落盘前必须先预留空间，磁盘满要在传之前就失败；
//! 3. 永远先写 `.part`，校验通过后再原子改名，用户看不到半个文件。

use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Windows 上禁止的保留名（不区分大小写，带扩展名也算）。
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 单个路径组件的最大长度。Windows 上限 255，这里留余量。
const MAX_COMPONENT_LEN: usize = 200;

/// 校验并规范化一个来自**网络**的相对路径，返回只由 `Normal` 组件构成的干净路径。
///
/// 这是防路径穿越的**唯一入口**，调用方不得绕过。拒绝：
/// 绝对路径、任何 `..`、根/前缀组件、Windows 保留设备名、空组件、过长组件、控制字符。
pub fn safe_relative_path(raw: &str) -> Result<PathBuf> {
    if raw.is_empty() {
        return Err(Error::UnsafePath("<空路径>".into()));
    }
    // 网络上来的路径可能来自任意平台，统一分隔符
    let normalized = raw.replace('\\', "/");

    // 绝对路径必须在 split 之前拦掉。否则 `/etc/passwd` 被 split 后
    // 开头的空片段会被忽略，就"洗白"成了相对路径 etc/passwd —— 这是
    // 一个真实的路径穿越漏洞，不是理论问题（见本模块测试）。
    if normalized.starts_with('/') {
        return Err(Error::UnsafePath(raw.into()));
    }
    // Windows 盘符（C:、c:）以及紧随其后的 ./ 形式
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
        return Err(Error::UnsafePath(raw.into()));
    }

    let mut out = PathBuf::new();

    for comp in normalized.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            return Err(Error::UnsafePath(raw.into()));
        }
        if comp.len() > MAX_COMPONENT_LEN {
            return Err(Error::UnsafePath(format!("{raw}（路径片段过长）")));
        }
        if comp.chars().any(|c| c.is_control()) {
            return Err(Error::UnsafePath(format!("{raw}（含控制字符）")));
        }
        // 保留名判定要覆盖 CON、NUL、COM1.txt 这类带扩展名的形式
        let stem = comp.split('.').next().unwrap_or(comp);
        if WINDOWS_RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
            return Err(Error::UnsafePath(format!("{raw}（系统保留名 {stem}）")));
        }
        out.push(comp);
    }

    if out.as_os_str().is_empty() {
        return Err(Error::UnsafePath(raw.into()));
    }

    // 双保险：显式确认结果里只有 Normal 组件
    for c in out.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err(Error::UnsafePath(raw.into()));
        }
    }
    Ok(out)
}

/// 把经校验的相对路径拼到目标目录下。
pub fn join_checked(base: &Path, rel: &Path) -> PathBuf {
    debug_assert!(!rel.as_os_str().is_empty());
    base.join(rel)
}

/// 预留 `size` 字节的落盘空间，并创建父目录。
///
/// 策略：创建文件并把长度扩展到 `size`。扩展长度不会真的写数据，但会让
/// 文件系统立刻为后续写入分配簇，避免"传到 90% 才发现磁盘满"。
/// 这也是跨平台唯一不需要特权 API 的做法。
pub fn preallocate(path: &Path, size: u64) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| Error::io(path, e))?;

    let current = file.metadata().map_err(|e| Error::io(path, e))?.len();
    if current >= size {
        return Ok(());
    }
    if size == 0 {
        return Ok(());
    }

    // 扩展文件长度：磁盘满会在这里立刻暴露，而不是传到一半才炸
    file.seek(SeekFrom::Start(size - 1))
        .map_err(|e| Error::io(path, e))?;
    file.write_all(&[0u8]).map_err(|e| Error::io(path, e))?;
    file.flush().map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// 原子改名：`from` → `to`。目标已存在时先移除。
///
/// 这是"用户永远看不到半个文件"的最后一步：只有校验通过才调用。
pub fn atomic_rename(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    // Windows 上 rename 到已存在的目标会失败，先删掉
    if to.exists() {
        fs::remove_file(to).map_err(|e| Error::io(to, e))?;
    }
    fs::rename(from, to).map_err(|e| Error::io(to, e))
}

/// 确保唯一文件名：`a.txt` 已存在时返回 `a (1).txt`。
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (file_name.to_string(), String::new()),
    };
    for n in 1..10_000u32 {
        let c = dir.join(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    candidate
}

/// 追加一个 `.part` 后缀，用于未完成文件。
pub fn part_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

/// 流式哈希文件，不把文件读进内存。发送前算校验和、接收后验校验和都用它。
pub fn hash_file(path: &Path) -> Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    let mut file = fs::File::open(path).map_err(|e| Error::io(path, e))?;
    io::copy(&mut file, &mut hasher).map_err(|e| Error::io(path, e))?;
    Ok(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("sr-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn accepts_normal_relative_paths() {
        for p in [
            "a.txt",
            "dir/a.txt",
            "a/b/c/d.bin",
            "带中文/文件.txt",
            "dir.with.dots/x",
        ] {
            assert!(safe_relative_path(p).is_ok(), "应接受: {p}");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for p in [
            "../etc/passwd",
            "a/../../b",
            "..",
            "a/..",
            "/etc/passwd",
            "C:\\Windows\\System32",
            "\\\\server\\share\\x",
        ] {
            assert!(safe_relative_path(p).is_err(), "应拒绝: {p}");
        }
    }

    #[test]
    fn rejects_windows_reserved_names() {
        for p in ["CON", "nul", "COM1.txt", "aux.log"] {
            assert!(safe_relative_path(p).is_err(), "应拒绝: {p}");
        }
    }

    /// 回归测试：`/etc/passwd` 曾被 `split('/')` 洗成相对路径 `etc/passwd`，
    /// 因为开头的空片段被当作"多余的分隔符"忽略了。这是真实的路径穿越漏洞，
    /// 不是理论问题，必须永远拦住。
    #[test]
    fn rejects_absolute_paths_that_would_normalize_into_relative() {
        for p in [
            "/etc/passwd",
            "C:/Windows/x",
            "c:foo",
            "//server/share/x",
            "\\\\server\\share",
        ] {
            assert!(safe_relative_path(p).is_err(), "绝对路径必须被拒绝: {p}");
        }
    }

    #[test]
    fn rejects_control_chars_and_long_components() {
        assert!(safe_relative_path("a\u{0}b").is_err());
        let long = "x".repeat(MAX_COMPONENT_LEN + 1);
        assert!(safe_relative_path(&long).is_err());
    }

    #[test]
    fn normalizes_windows_separators() {
        let p = safe_relative_path("dir\\sub\\file.txt").unwrap();
        assert_eq!(p, PathBuf::from("dir/sub/file.txt"));
    }

    #[test]
    fn preallocate_then_atomic_rename_roundtrip() {
        let dir = tmpdir();
        let part = dir.join("f.part");
        preallocate(&part, 4096).unwrap();
        assert_eq!(fs::metadata(&part).unwrap().len(), 4096);
        let final_path = dir.join("f.bin");
        atomic_rename(&part, &final_path).unwrap();
        assert!(final_path.exists() && !part.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preallocate_does_not_truncate_larger_file() {
        let dir = tmpdir();
        let p = dir.join("f.part");
        preallocate(&p, 8192).unwrap();
        preallocate(&p, 1024).unwrap();
        assert_eq!(fs::metadata(&p).unwrap().len(), 8192);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unique_path_avoids_collision() {
        let dir = tmpdir();
        fs::write(dir.join("a.txt"), b"x").unwrap();
        let p = unique_path(&dir, "a.txt");
        assert_eq!(p.file_name().unwrap(), "a (1).txt");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn part_path_appends_suffix() {
        let p = part_path(Path::new("a/b.bin"));
        assert!(p.to_string_lossy().ends_with("b.bin.part"), "{p:?}");
    }

    #[test]
    fn hash_file_matches_blake3_of_bytes() {
        let dir = tmpdir();
        let p = dir.join("x.bin");
        let data = vec![7u8; 100_000];
        fs::write(&p, &data).unwrap();
        assert_eq!(hash_file(&p).unwrap(), blake3::hash(&data));
        fs::remove_dir_all(&dir).ok();
    }
}
