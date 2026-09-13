//! `strange-room-core`：Strange Room 的内核。
//!
//! 这一层**不依赖任何 UI**，因此可以被三种前端复用：
//! - `sr-cli`（v1 的主验证工具，也是长期的测试资产）
//! - Tauri 桌面壳（M6 起）
//! - 将来的移动端
//!
//! 分层：
//! ```text
//! qr / identity  ← 信任与地址（二维码把两者绑在一起）
//! protocol       ← 线协议（与传输无关）
//! transfer       ← 计划、分块、续传状态
//! net            ← QUIC + TLS + 会话驱动
//! progress       ← 把"发生了什么"发出去，UI 自己渲染
//! ```

pub mod bootstrap;
pub mod bytes;
pub mod diag;
pub mod discovery;
pub mod cancel;
pub mod error;
pub mod fs_util;
pub mod identity;
pub mod net;
pub mod progress;
pub mod protocol;
pub mod qr;
pub mod transfer;

pub use bootstrap::BootstrapServer;
pub use cancel::CancelToken;
pub use diag::{Diagnosis, ProbeOutcome, ProbeReport, Verdict};
pub use discovery::{discover, verification_code, Advertisement, NearbyHost};
pub use error::{Error, Result};
pub use progress::{ProgressEvent, ProgressSender};
pub use qr::{AddressHint, QrPayload};
pub use transfer::plan::{plan_paths, PlannedFile, TransferPlan};
pub use transfer::resume::{PartialFile, ResumeState};

/// 语义化版本，便于日志和协议兼容排查。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
