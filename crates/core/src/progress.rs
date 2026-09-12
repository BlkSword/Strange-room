//! 进度模型：与 UI 无关。
//!
//! 内核只负责"把发生了什么"发出来，CLI 用 indicatif 渲染进度条，
//! Tauri 界面将来用同一套事件渲染图形界面。这样 UI 换几遍内核都不用动。

/// 一次传输过程中的事件。
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// 会话开始，对端已握手
    SessionStarted {
        peer: String,
        total_files: usize,
        total_bytes: u64,
    },
    /// 某个文件开始（resumed_from > 0 表示这是续传）
    FileStarted {
        file_id: String,
        relative_path: String,
        size: u64,
        resumed_from: u64,
    },
    /// 字节级进度
    ChunkProgress {
        file_id: String,
        bytes_done: u64,
        bytes_total: u64,
    },
    /// 某个文件完成并通过校验
    FileFinished {
        file_id: String,
        relative_path: String,
    },
    /// 会话结束
    SessionFinished { files: usize, bytes: u64 },
    /// 非致命问题（单个文件失败但会话继续）
    Warn(String),
}

/// 发送端。UI 通过 `subscribe()` 拿到接收端。
#[derive(Clone)]
pub struct ProgressSender {
    tx: tokio::sync::broadcast::Sender<ProgressEvent>,
}

impl std::fmt::Debug for ProgressSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressSender").finish_non_exhaustive()
    }
}

impl Default for ProgressSender {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressSender {
    pub fn new() -> Self {
        // 容量足够覆盖一次传输里常见的事件突发；满了会丢旧事件，
        // 对进度展示来说可以接受（丢的是过期的进度值）。
        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        Self { tx }
    }

    pub fn send(&self, event: ProgressEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ProgressEvent> {
        self.tx.subscribe()
    }
}

/// 纯计算：把进度换算成百分比，方便 UI 和测试直接复用。
pub fn percent(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 100.0;
    }
    ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
}

/// 纯计算：根据已用时间和已传字节估算剩余时间（秒）。
/// 返回 None 表示样本太少，估不出来——不要瞎猜一个数字给用户。
pub fn eta_seconds(done: u64, total: u64, elapsed_secs: f64) -> Option<f64> {
    if done == 0 || elapsed_secs <= 0.0 || total <= done {
        return None;
    }
    let rate = done as f64 / elapsed_secs;
    if rate <= 0.0 {
        return None;
    }
    Some((total - done) as f64 / rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_handles_zero_total() {
        assert_eq!(percent(0, 0), 100.0);
        assert_eq!(percent(50, 100), 50.0);
        assert_eq!(percent(200, 100), 100.0);
    }

    #[test]
    fn eta_is_none_when_underdetermined() {
        assert!(eta_seconds(0, 100, 1.0).is_none());
        assert!(eta_seconds(100, 100, 1.0).is_none());
        assert!(eta_seconds(50, 100, 0.0).is_none());
    }

    #[test]
    fn eta_estimates_remaining_time() {
        // 10 秒传了 50/100 → 还要 10 秒
        let eta = eta_seconds(50, 100, 10.0).unwrap();
        assert!((eta - 10.0).abs() < 0.001, "{eta}");
    }

    #[tokio::test]
    async fn events_are_delivered_to_subscriber() {
        let sender = ProgressSender::new();
        let mut rx = sender.subscribe();
        sender.send(ProgressEvent::SessionFinished {
            files: 1,
            bytes: 10,
        });
        let got = rx.recv().await.unwrap();
        assert_eq!(
            got,
            ProgressEvent::SessionFinished {
                files: 1,
                bytes: 10
            }
        );
    }

    #[tokio::test]
    async fn send_without_subscriber_does_not_panic() {
        let sender = ProgressSender::new();
        sender.send(ProgressEvent::Warn("x".into()));
    }
}
