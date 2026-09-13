//! 发送端：把用户选中的路径展开成一份可传输的文件清单。
//!
//! 清单在传输**之前**就完全确定（含每个文件的 BLAKE3），这样：
//! - 接收端能先知道全貌（总大小、文件数），好显示进度和判断空间是否够；
//! - 接收端能对每个文件独立续传；
//! - 校验基准是源文件的哈希，而不是"发送端说了算"。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fs_util;

/// 条目种类。平台化的第一步：房间里的东西不只有文件。
///
/// 文本条目**刻意复用文件那一整套**（有 size、有 BLAKE3、一样分块传），
/// 所以传输、校验、进度这些代码一行都不用改——区别只有两处：
/// 来源在内存而不是磁盘，落点在界面而不是目录。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    #[default]
    File,
    /// 一段文本或链接（"贴纸"）
    Text,
}

/// 清单里的一个条目（文件或文本）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedFile {

    /// 稳定 ID：由相对路径推导，保证接收端重连后仍能对上同一个文件。
    pub file_id: String,
    /// 相对路径（相对用户选中的根），用 `/` 分隔
    pub relative_path: String,
    /// 源文件绝对路径（只在发送端使用，不上网）
    pub source_path: PathBuf,
    pub size: u64,
    /// BLAKE3 hex
    pub blake3: String,
    /// 条目种类。旧清单里没有这个字段，反序列化时按"文件"处理。
    #[serde(default)]
    pub kind: ItemKind,
    /// 文本条目的内容（只存在于发送端内存，不上网也不落盘）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}


#[derive(Debug, Clone, Default)]
pub struct TransferPlan {
    pub files: Vec<PlannedFile>,
    /// 用户选中的根目录名。接收端会新建这个目录，避免把多个根混在一起。
    pub root_name: String,
    pub total_bytes: u64,
}

impl TransferPlan {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// 一次最多能发的文本大小。
///
/// 刻意限制得比较小：文本走的是"贴纸"那条路，不是用来传文件的。
/// 真要发大段内容，用户应该发文件——那样才有落盘和续传。
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

/// 造一份"只包含一段文本"的清单。
///
/// `label` 是接收端看到的来源说明（例如"来自 小黑的剪贴板"）。
pub fn plan_text(label: &str, content: &str) -> Result<TransferPlan> {
    let bytes = content.as_bytes();
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(Error::protocol(format!(
            "文本太长了（{} 字节，上限 {}）——大段内容请当作文件发送",
            bytes.len(),
            MAX_TEXT_BYTES
        )));
    }
    let label = label.trim();
    let label = if label.is_empty() { "一段文本" } else { label };
    Ok(TransferPlan {
        files: vec![PlannedFile {
            // 用内容派生 ID：同样的文本得到同样的 ID，重发不会让接收端困惑
            file_id: file_id_for(&format!("text:{label}:{content}")),
            relative_path: label.to_string(),
            source_path: PathBuf::new(),
            size: bytes.len() as u64,
            blake3: hex::encode(blake3::hash(bytes).as_bytes()),
            kind: ItemKind::Text,
            text: Some(content.to_string()),
        }],
        root_name: String::new(),
        total_bytes: bytes.len() as u64,
    })
}

/// 把文本条目接到一份已有的文件清单后面（房间里既能放文件也能贴文本）。
pub fn append_text(plan: &mut TransferPlan, label: &str, content: &str) -> Result<()> {
    let mut extra = plan_text(label, content)?;
    if let Some(item) = extra.files.pop() {
        plan.total_bytes += item.size;
        plan.files.push(item);
    }
    Ok(())
}

/// 由相对路径生成稳定 file_id。

///
/// 用 BLAKE3 而不是随机 UUID：接收端重连、或用户重传同一批文件时，
/// ID 保持一致，续传状态才能被正确匹配上。
pub fn file_id_for(relative_path: &str) -> String {
    let h = blake3::hash(relative_path.as_bytes());
    hex::encode(&h.as_bytes()[..8])
}

/// 展开用户选中的路径为传输清单。
///
/// - 选中单个文件：根名 = 文件名，清单里只有它
/// - 选中目录：递归展开，根名 = 目录名，相对路径保留目录结构
/// - 多个路径：根名取公共的父目录名（简单起见用第一个的父目录）
pub fn plan_paths(paths: &[PathBuf]) -> Result<TransferPlan> {
    if paths.is_empty() {
        return Err(Error::protocol("没有选择任何要发送的文件"));
    }
    let mut plan = TransferPlan::default();
    let base: PathBuf = if paths.len() == 1 && paths[0].is_file() {
        paths[0]
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else if paths.len() == 1 {
        paths[0]
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        paths[0]
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    plan.root_name = paths
        .first()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "shared".to_string());

    for p in paths {
        if !p.exists() {
            return Err(Error::protocol(format!("路径不存在：{}", p.display())));
        }
        if p.is_file() {
            add_file(&mut plan, &base, p)?;
        } else {
            // 目录：递归，但跳过符号链接以避免环和逃逸出选中目录
            for entry in walkdir::WalkDir::new(p)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    add_file(&mut plan, &base, entry.path())?;
                }
            }
        }
    }

    plan.files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    plan.total_bytes = plan.files.iter().map(|f| f.size).sum();
    if plan.files.is_empty() {
        return Err(Error::protocol("选中的目录里没有可传输的文件"));
    }
    Ok(plan)
}

fn add_file(plan: &mut TransferPlan, base: &Path, path: &Path) -> Result<()> {
    let rel = path
        .strip_prefix(base)
        .map_err(|_| Error::protocol(format!("无法计算相对路径：{}", path.display())))?;
    let relative_path = rel.to_string_lossy().replace('\\', "/");
    // 发送前先自己校验一遍：不给接收端送去会被拒的路径
    fs_util::safe_relative_path(&relative_path)?;

    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let size = meta.len();
    let hash = fs_util::hash_file(path)?;

    plan.files.push(PlannedFile {
        file_id: file_id_for(&relative_path),
        relative_path,
        source_path: path.to_path_buf(),
        size,
        blake3: hex::encode(hash.as_bytes()),
        kind: ItemKind::File,
        text: None,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("chuan-plan-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn plan_text_carries_content_hash_and_kind() {
        let plan = plan_text("一个链接", "https://example.com/a?b=1").unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.total_bytes, "https://example.com/a?b=1".len() as u64);

        let item = &plan.files[0];
        assert_eq!(item.kind, ItemKind::Text, "文本条目必须带上种类");
        assert_eq!(item.text.as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(
            item.blake3,
            hex::encode(blake3::hash(b"https://example.com/a?b=1").as_bytes()),
            "文本也要有 BLAKE3——接收端靠它校验"
        );
        // 同样的内容得到同样的 ID：重发不会让接收端以为是另一个东西
        let again = plan_text("一个链接", "https://example.com/a?b=1").unwrap();
        assert_eq!(again.files[0].file_id, item.file_id);

        // 空标签有兜底，不然接收端会看到一片空白
        let unnamed = plan_text("   ", "x").unwrap();
        assert_eq!(unnamed.files[0].relative_path, "一段文本");
    }

    #[test]
    fn plan_text_refuses_oversized_text() {
        let big = "x".repeat(MAX_TEXT_BYTES + 1);
        let err = plan_text("太大", &big).expect_err("超过上限必须拒绝");
        assert!(
            err.to_string().contains("文本太长"),
            "错误信息要说清原因：{err}"
        );
    }

    #[test]
    fn append_text_keeps_files_and_updates_total() {
        let dir = tmpdir();
        let file = dir.join("a.bin");
        fs::write(&file, b"hello").unwrap();

        let mut plan = plan_paths(&[file]).unwrap();
        let before = plan.total_bytes;
        append_text(&mut plan, "一段文本", "贴一段字").unwrap();

        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].kind, ItemKind::File);
        assert_eq!(plan.files[1].kind, ItemKind::Text);
        assert_eq!(plan.total_bytes, before + "贴一段字".len() as u64, "总字节数要算上文本");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_single_file_records_hash_and_size() {
        let dir = tmpdir();
        let f = dir.join("a.bin");
        let data = vec![9u8; 5000];
        fs::write(&f, &data).unwrap();

        let plan = plan_paths(&[f.clone()]).unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].size, 5000);
        assert_eq!(plan.files[0].blake3, hex::encode(blake3::hash(&data).as_bytes()));
        assert_eq!(plan.files[0].relative_path, "a.bin");
        assert_eq!(plan.total_bytes, 5000);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_directory_preserves_structure_and_skips_nonfiles() {
        let dir = tmpdir();
        let root = dir.join("proj");
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("top.txt"), b"top").unwrap();
        fs::write(root.join("sub/mid.txt"), b"mid").unwrap();
        fs::write(root.join("sub/deep/leaf.txt"), b"leaf").unwrap();

        let plan = plan_paths(&[root]).unwrap();
        let mut names: Vec<_> = plan.files.iter().map(|f| f.relative_path.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "proj/sub/deep/leaf.txt",
                "proj/sub/mid.txt",
                "proj/top.txt"
            ]
        );
        assert_eq!(plan.total_bytes, 10);
        assert_eq!(plan.root_name, "proj");
        fs::remove_dir_all(&dir).ok();
    }


    #[test]
    fn file_id_is_stable_and_path_derived() {
        assert_eq!(file_id_for("a/b.txt"), file_id_for("a/b.txt"));
        assert_ne!(file_id_for("a/b.txt"), file_id_for("a/c.txt"));
        assert_eq!(file_id_for("x").len(), 16);
    }

    #[test]
    fn rejects_empty_input_and_missing_path() {
        assert!(plan_paths(&[]).is_err());
        assert!(plan_paths(&[PathBuf::from("/definitely/not/here/xyz")]).is_err());
    }
}
