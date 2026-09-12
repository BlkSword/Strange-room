//! 统一错误类型。
//!
//! 设计原则：错误消息是给**用户**看的，必须可操作（"磁盘空间不足"），
//! 而不是给开发者看的（"io error: No space left on device"）。

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("磁盘空间不足：目标磁盘还需要 {need} 字节，但只剩 {available} 字节")]
    InsufficientSpace { need: u64, available: u64 },

    #[error("路径不安全，已拒绝：{0}（不允许绝对路径、上级目录或系统保留名）")]
    UnsafePath(String),

    #[error("证书指纹不匹配：期望 {expected}，实际 {actual}。可能是二维码过期或存在中间人攻击")]
    FingerprintMismatch { expected: String, actual: String },

    #[error("协议错误：{0}")]
    Protocol(String),

    #[error("对端提前断开连接（已接收 {received}/{total} 字节，可用同一会话续传）")]
    Disconnected { received: u64, total: u64 },

    #[error("校验失败：{path} 的 BLAKE3 与源文件不一致")]
    ChecksumMismatch { path: PathBuf },

    #[error("对端拒绝了文件 {name}：{reason}")]
    OfferRejected { name: String, reason: String },

    #[error("IO 错误（{path}）：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("序列化失败：{0}")]
    Serde(#[from] serde_json::Error),
    #[error("已取消")]
    Cancelled,


    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
