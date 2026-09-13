//! 线协议：帧格式 + 消息定义。
//!
//! 帧格式（与语言、传输都无关，将来换掉 QUIC 也不用改）：
//!
//! ```text
//! +--------+--------------+------------------+
//! | kind   | len (u32 BE) | payload          |
//! | 1 byte | 4 bytes      | len bytes        |
//! +--------+--------------+------------------+
//! ```
//!
//! 控制消息用 JSON（可读、好调试、字段可演进）；文件数据块走 DATA 帧发原始
//! 字节，不经过 JSON，避免大块数据的编码开销和体积膨胀。

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};

/// 线路协议版本。
///
/// v2：第三条流做"双向房间"（两个方向各一条流，并发投放）。
/// 老版本只开一条流且用"顺序交换"绕行，与新版本**不能混用**，所以版本号必须涨。
pub const PROTOCOL_VERSION: u32 = 2;

/// 默认分块大小。1 MiB 是吞吐与内存占用的折中点。
pub const DEFAULT_CHUNK_SIZE: u32 = 1024 * 1024;

/// 单帧上限。控制消息远小于此，数据帧也不应超过它。
pub const MAX_FRAME_SIZE: u32 = 8 * 1024 * 1024;

// ---- 帧类型 ----
pub const KIND_HELLO: u8 = 1;
pub const KIND_OFFER: u8 = 2;
pub const KIND_ACK: u8 = 3;
pub const KIND_DATA: u8 = 4;
pub const KIND_FILE_END: u8 = 5;
pub const KIND_RESULT: u8 = 6;
pub const KIND_OK: u8 = 7;
pub const KIND_ERROR: u8 = 8;
pub const KIND_BYE: u8 = 9;
pub const KIND_MANIFEST: u8 = 10;

#[derive(Debug, Clone)]
pub struct Frame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(kind: u8, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    pub fn json<T: Serialize>(kind: u8, value: &T) -> Result<Self> {
        let payload = serde_json::to_vec(value)?;
        if payload.len() as u32 > MAX_FRAME_SIZE {
            return Err(Error::protocol("控制消息超过单帧上限"));
        }
        Ok(Self { kind, payload })
    }

    pub fn decode_json<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        serde_json::from_slice(&self.payload)
            .map_err(|e| Error::protocol(format!("消息解析失败: {e}")))
    }

    pub fn expect_kind(&self, expected: u8, what: &str) -> Result<()> {
        if self.kind != expected {
            return Err(Error::protocol(format!(
                "期望{what}（帧类型 {expected}），实际收到帧类型 {}",
                self.kind
            )));
        }
        Ok(())
    }
}

/// 写入一帧。长度前缀为 u32 大端。
pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, frame: &Frame) -> Result<()> {
    if frame.payload.len() as u32 > MAX_FRAME_SIZE {
        return Err(Error::protocol("待发送的帧超过上限"));
    }
    let mut header = [0u8; 5];
    header[0] = frame.kind;
    header[1..5].copy_from_slice(&(frame.payload.len() as u32).to_be_bytes());
    w.write_all(&header)
        .await
        .map_err(|e| Error::protocol(format!("写入帧头失败: {e}")))?;
    w.write_all(&frame.payload)
        .await
        .map_err(|e| Error::protocol(format!("写入帧体失败: {e}")))?;
    Ok(())
}

/// 读取一帧。对端干净关闭时返回 `Ok(None)`；读到半截则报错。
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Frame>> {
    let mut header = [0u8; 5];
    match r.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(Error::protocol(format!("读取帧头失败: {e}"))),
    }
    let kind = header[0];
    let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);
    if len > MAX_FRAME_SIZE {
        return Err(Error::protocol(format!(
            "对端声明了过大的帧（{len} 字节），可能是协议不匹配或恶意数据"
        )));
    }
    if kind == 0 {
        return Err(Error::protocol("收到非法帧类型 0"));
    }
    let mut payload = vec![0u8; len as usize];
    if len > 0 {
        r.read_exact(&mut payload)
            .await
            .map_err(|e| Error::protocol(format!("读取帧体失败: {e}")))?;
    }
    Ok(Some(Frame { kind, payload }))
}

// ==================== 消息定义 ====================

/// 主动连接的一方是**要收文件**的人，所以叫 receiver。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientHello {
    pub protocol_version: u32,
    /// 会话 ID，来自二维码。主机用它确认对端扫的是当前这个会话。
    pub session_id: String,
    pub device_name: String,
    /// TCP 回退下的车道号：0 = 我送对方取，1 = 对方送我取。
    ///
    /// QUIC 路径用不到它（两条流天然分开），所以固定发 0；给默认值是为了
    /// 让"没这个字段"的旧握手也照常解析——加它不需要动协议版本。
    #[serde(default)]
    pub lane: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerHello {
    pub protocol_version: u32,
    pub session_id: String,
    pub device_name: String,
    /// 主机愿意接受的最大分块大小，接收端据此决定 offer 里的 chunk_size。
    pub max_chunk_size: u32,
    /// 主机这次是否也收东西（即有没有指定接收目录）。
    ///
    /// 有了它，接收端带着东西来、而对方没开接收目录时，可以**立刻**说清楚，
    /// 而不是先把东西推过去、被拒绝、再重试四轮——那是个用法错误，重试没意义。
    #[serde(default)]
    pub accepts_incoming: bool,
}

/// 主机端共享的清单条目（文件或文本）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub file_id: String,
    /// 文件是相对路径；文本是给人看的来源说明（例如"来自 小黑的剪贴板"）
    pub relative_path: String,
    pub size: u64,
    pub blake3: String,
    /// 条目种类。旧版本发的清单没有这个字段，按"文件"处理。
    #[serde(default)]
    pub kind: crate::transfer::plan::ItemKind,
}


/// 清单：跟在 ServerHello 之后发送，让接收端先拿到全貌。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileManifest {
    pub files: Vec<FileEntry>,
    pub total_bytes: u64,
}

/// 接收端对某个文件的请求。`have_bytes` 就是续传的起点：
/// 接收端扫自己本地已收了多少，直接告诉发送端从哪开始。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileOffer {
    pub file_id: String,
    /// 相对路径，可能包含目录，如 `photos/a.jpg`
    pub relative_path: String,
    pub size: u64,
    /// 发送端清单里的 BLAKE3（hex），接收端最终用它校验
    pub blake3: String,
    pub have_bytes: u64,
    pub chunk_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfferAck {
    pub file_id: String,
    /// 发送端确认的实际起始偏移。通常等于 `have_bytes`；若发送端认为
    /// 接收端状态不可信（如偏移越界）则是 0，即重传。
    pub start_offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEnd {
    pub file_id: String,
    /// 发送端回传的 BLAKE3，接收端用它和本地计算结果比对
    pub blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferResult {
    pub file_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorMsg {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bye {
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn file_entry_without_kind_is_treated_as_a_file() {
        // 旧版本发来的清单里没有 kind 字段（那时只有文件）。
        // 这必须被当成文件、而不是报错——向前兼容是"房间里能混着放东西"的前提。
        let json = r#"{"file_id":"a1","relative_path":"a.bin","size":3,"blake3":"00ff"}"#;
        let entry: FileEntry = serde_json::from_str(json).expect("应当能解析旧清单");
        assert_eq!(entry.kind, crate::transfer::plan::ItemKind::File);
    }

    #[tokio::test]
    async fn frame_roundtrip_over_stream() {
        let (mut a, mut b) = duplex(64 * 1024);
        let frames = vec![
            Frame::new(KIND_DATA, vec![1, 2, 3, 4, 5]),
            Frame::new(KIND_HELLO, vec![]),
            Frame::new(KIND_DATA, vec![0u8; 40_000]),
        ];
        let to_write = frames.clone();
        let writer = tokio::spawn(async move {
            for f in &to_write {
                write_frame(&mut a, f).await.unwrap();
            }
        });
        for expected in &frames {
            let got = read_frame(&mut b).await.unwrap().unwrap();
            assert_eq!(got.kind, expected.kind);
            assert_eq!(got.payload, expected.payload);
        }
        assert!(read_frame(&mut b).await.unwrap().is_none());
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn json_messages_roundtrip() {
        let hello = ClientHello {
            lane: 0,

            protocol_version: PROTOCOL_VERSION,
            session_id: "abc123".into(),
            device_name: "我的笔记本".into(),
        };
        let frame = Frame::json(KIND_HELLO, &hello).unwrap();
        assert_eq!(frame.decode_json::<ClientHello>().unwrap(), hello);

        let offer = FileOffer {
            file_id: "f1".into(),
            relative_path: "photos/假期 2026/a.jpg".into(),
            size: 123_456_789,
            blake3: "deadbeef".into(),
            have_bytes: 65_536,
            chunk_size: DEFAULT_CHUNK_SIZE,
        };
        let frame = Frame::json(KIND_OFFER, &offer).unwrap();
        assert_eq!(frame.decode_json::<FileOffer>().unwrap(), offer);
    }

    #[tokio::test]
    async fn large_json_payload_survives() {
        let manifest = FileManifest {
            files: (0..500)
                .map(|i| FileEntry {
                    file_id: format!("f{i}"),
                    relative_path: format!("dir{}/file-{i}.bin", i % 20),
                    size: 1024 * 1024,
                    blake3: "ab".repeat(32),
                    kind: crate::transfer::plan::ItemKind::File,
                })
                .collect(),
            total_bytes: 500 * 1024 * 1024,
        };
        let frame = Frame::json(KIND_MANIFEST, &manifest).unwrap();
        assert!(frame.payload.len() > 40_000);
        assert_eq!(frame.decode_json::<FileManifest>().unwrap(), manifest);
    }

    #[tokio::test]
    async fn rejects_oversized_frame_length() {
        let (mut a, mut b) = duplex(1024);
        let mut bad = [0u8; 5];
        bad[0] = KIND_DATA;
        bad[1..5].copy_from_slice(&(MAX_FRAME_SIZE + 1).to_be_bytes());
        a.write_all(&bad).await.unwrap();
        drop(a);
        let err = read_frame(&mut b).await.unwrap_err();
        assert!(err.to_string().contains("过大"), "{err}");
    }

    #[tokio::test]
    async fn rejects_zero_kind() {
        let (mut a, mut b) = duplex(1024);
        a.write_all(&[0u8, 0, 0, 0, 0]).await.unwrap();
        drop(a);
        assert!(read_frame(&mut b).await.is_err());
    }

    #[tokio::test]
    async fn expect_kind_reports_useful_error() {
        let f = Frame::new(KIND_RESULT, vec![]);
        let err = f.expect_kind(KIND_OK, "确认帧").unwrap_err();
        assert!(err.to_string().contains("确认帧"), "{err}");
    }
}
