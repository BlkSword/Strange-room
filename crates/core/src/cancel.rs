//! 取消信号。
//!
//! 传输可能持续很久（一个几百 GB 的文件夹），用户随时可能想停下。这里用一个
//! 可以在任务之间共享的原子标志，让传输循环在**每个能安全停下的位置**检查一次。
//!
//! 刻意做成协作式（cooperative）而不是强杀进程，原因是"停下"的语义：
//!
//! - 协作式：循环在自己节奏里退出，此时已经写入的数据是完整的、检查点是最新的，
//!   用户下次打开还能接着传。
//! - 强杀：留下半截 `.part` 和过期的检查点，下次要么重传整段，要么（更糟）
//!   把不完整的数据当成完整的。
//!
//! 所以"能取消"这件事，价值不只在体验，也在数据正确性。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{Error, Result};

/// 可克隆的取消信号。克隆出来的所有副本共享同一个标志。
#[derive(Clone)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    /// 光有标志位不够：TCP 回退通道的读帧会**阻塞在网络上**，没人叫醒它
    /// 就一直挂着。这个 watch 通道让 `cancelled()` 能立刻返回。
    /// 用 watch 而不是 Notify：watch 会记住"已经取消"这件事，
    /// 不存在"取消发生在注册等待之前"的竞态。
    watch: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for CancelToken {
    fn default() -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(false);
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            watch: Arc::new(tx),
        }
    }
}

impl std::fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求取消。可以从任意线程调用（UI 的按钮线程、信号处理器都行）。
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        let _ = self.watch.send(true);
    }

    /// 等到被取消；已经取消则立刻返回。
    ///
    /// 给"读网络"这类会长期阻塞的地方用：只有标志位的话，读会一直挂着。
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let mut rx = self.watch.subscribe();
        // 先看当前值再等变化：`send` 发生在 `subscribe` 之前的窗口不会漏
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// 已取消则返回错误，用于在循环里用 `?` 传播。
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_not_cancelled() {
        let t = CancelToken::new();
        assert!(!t.is_cancelled());
        assert!(t.check().is_ok());
    }

    #[test]
    fn clones_share_one_flag() {
        // 这是关键性质：UI 线程拿一个克隆去取消，传输任务手里的那个必须能看见
        let a = CancelToken::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        a.cancel();
        assert!(b.is_cancelled(), "克隆体必须共享同一个标志");
        assert!(matches!(b.check(), Err(Error::Cancelled)));
    }

    #[test]
    fn cancel_is_idempotent() {
        let t = CancelToken::new();
        t.cancel();
        t.cancel();
        assert!(t.is_cancelled());
    }
}
