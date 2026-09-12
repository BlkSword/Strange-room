//! 二维码载荷：**一次扫码同时传递地址、会话和信任锚点**。
//!
//! 这是整个产品最关键的设计点（见 PLAN 5.1）。载荷里三样东西缺一不可：
//!
//! | 字段 | 作用 |
//! |---|---|
//! | `addrs` | 连得上（怎么找到对方） |
//! | `fingerprint` | 敢连（怎么信任对方，等价于 Signal 安全码比对） |
//! | `session_id` | 连的是这个会话而不是别的 |
//!
//! 编码成 `srx1:<base64url(JSON)>`。加前缀是为了将来能识别版本、
//! 并把"这是一段 Strange Room 连接串"和普通文本区分开；用 base64url
//! 是为了避免 `+ / =` 在二维码和终端里的转义麻烦。

use serde::{Deserialize, Serialize};

use crate::bytes;
use crate::error::{Error, Result};

/// 载荷版本。将来字段不兼容时递增，并保留对旧版本的处理分支。
pub const QR_VERSION: u32 = 1;

/// 人类可读前缀，方便用户辨认、也方便 CLI/UI 直接判断输入类型。
pub const QR_PREFIX: &str = "srx1:";

/// 一个候选地址。v1 不依赖 mDNS，所以地址是"主机自己认为可用的局域网地址"，
/// 由主机枚举网卡得到；可能有多个（多网卡/虚拟网卡），全部带上让接收端挨个试。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddressHint {
    /// 形如 `192.168.1.7` 或 `[fe80::1]`（IPv6 带方括号）
    pub host: String,
    pub port: u16,
}

impl AddressHint {
    pub fn display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QrPayload {
    pub v: u32,
    /// 会话 ID（随机，一次会话一个）
    pub sid: String,
    /// 主机设备名，用于接收端确认"连的是谁的屏幕"
    pub name: String,
    /// 证书指纹（hex）。这是信任锚点，来自屏幕这个带外信道。
    pub fp: String,
    /// 候选地址列表
    pub addrs: Vec<AddressHint>,
}

impl QrPayload {
    pub fn new(
        session_id: impl Into<String>,
        device_name: impl Into<String>,
        fingerprint: impl Into<String>,
        addrs: Vec<AddressHint>,
    ) -> Self {
        Self {
            v: QR_VERSION,
            sid: session_id.into(),
            name: device_name.into(),
            fp: fingerprint.into(),
            addrs,
        }
    }

    /// 编码为二维码内容 / 可粘贴的连接串。
    pub fn encode(&self) -> Result<String> {
        let json = serde_json::to_vec(self)?;
        Ok(format!("{}{}", QR_PREFIX, bytes::b64url_encode(&json)))
    }

    /// 解析连接串，同时宽容地接受"用户只粘贴了 base64 部分"的情况
    /// （从二维码扫描结果里复制时经常发生）。
    pub fn decode(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let body = raw.strip_prefix(QR_PREFIX).unwrap_or(raw);
        let json = bytes::b64url_decode(body).map_err(|_| {
            Error::protocol("无法解析这个连接串，请确认二维码是否完整、是否过期")
        })?;
        let payload: QrPayload = serde_json::from_slice(&json).map_err(|e| {
            Error::protocol(format!("连接串格式不正确（{e}），可能是旧版本生成的"))
        })?;
        payload.validate()?;
        Ok(payload)
    }

    /// 结构校验。宁可在这里失败，也不要带着坏数据去连。
    pub fn validate(&self) -> Result<()> {
        if self.v != QR_VERSION {
            return Err(Error::protocol(format!(
                "连接串版本为 {}，当前版本只支持 {QR_VERSION}，请双方使用同一版本",
                self.v
            )));
        }
        if self.sid.is_empty() {
            return Err(Error::protocol("连接串缺少会话 ID"));
        }
        if self.fp.len() != crate::identity::FINGERPRINT_LEN * 2 {
            return Err(Error::protocol(format!(
                "连接串里的指纹长度不对（收到 {} 个字符）",
                self.fp.len()
            )));
        }
        if self.addrs.is_empty() {
            return Err(Error::protocol(
                "连接串里没有任何可用地址，请检查主机是否已加入局域网",
            ));
        }
        for a in &self.addrs {
            if a.host.is_empty() || a.port == 0 {
                return Err(Error::protocol("连接串包含非法地址"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> QrPayload {
        QrPayload::new(
            "sess-abc",
            "我的笔记本",
            "a".repeat(crate::identity::FINGERPRINT_LEN * 2),
            vec![
                AddressHint {
                    host: "192.168.1.7".into(),
                    port: 45001,
                },
                AddressHint {
                    host: "[fe80::1]".into(),
                    port: 45001,
                },
            ],
        )
    }

    #[test]
    fn encode_decode_roundtrip() {
        let p = sample();
        let s = p.encode().unwrap();
        assert!(s.starts_with(QR_PREFIX));
        assert_eq!(QrPayload::decode(&s).unwrap(), p);
    }

    #[test]
    fn keeps_qr_payload_short_enough_to_scan() {
        // 两个地址时，载荷应该远小于二维码在中等分辨率下的容量上限；
        // 这是"零准备"能否成立的实际约束，随字段增长必须警惕。
        let s = sample().encode().unwrap();
        assert!(s.len() < 400, "二维码载荷过长: {} 字符", s.len());
    }

    #[test]
    fn decode_accepts_bare_base64() {
        let p = sample();
        let s = p.encode().unwrap();
        let bare = s.strip_prefix(QR_PREFIX).unwrap();
        assert_eq!(QrPayload::decode(bare).unwrap(), p);
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(QrPayload::decode("hello world").is_err());
        assert!(QrPayload::decode("").is_err());
        assert!(QrPayload::decode("srx1:!!!!").is_err());
    }

    #[test]
    fn validate_rejects_bad_fingerprint_and_empty_addrs() {
        let mut p = sample();
        p.fp = "abc".into();
        assert!(p.validate().is_err());

        let mut p = sample();
        p.addrs.clear();
        assert!(p.validate().is_err());

        let mut p = sample();
        p.v = 99;
        assert!(p.validate().is_err());
    }

    #[test]
    fn decode_rejects_wrong_version() {
        let mut p = sample();
        p.v = 2;
        let json = serde_json::to_vec(&p).unwrap();
        let s = format!("{}{}", QR_PREFIX, bytes::b64url_encode(&json));
        let err = QrPayload::decode(&s).unwrap_err();
        assert!(err.to_string().contains("版本"), "{err}");
    }
}
