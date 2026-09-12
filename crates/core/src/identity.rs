//! 本机身份：自签证书 + 指纹。
//!
//! 产品最关键的设计点是"二维码同时承载地址和信任锚点"（见 PLAN 5.1）。
//! 这里的指纹就是那个信任锚点：接收端在 TLS 握手时只接受**指纹匹配**的证书，
//! 等价于 Signal 的安全码比对，不需要 CA、不需要账号、不需要云。
//!
//! 首次启动生成一次，落盘到用户数据目录，之后长期复用。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use crate::error::{Error, Result};

/// 自签证书里使用的占位名字。因为校验只认指纹，域名不参与判断。
pub const SERVER_NAME: &str = "strange-room.local";

/// 证书指纹算法：BLAKE3 输出前 16 字节（128 位），hex 编码后 32 个字符。
///
/// 128 位对于"人工比对/二维码携带"的场景足够（碰撞概率可忽略），
/// 同时比完整 256 位更适合放进二维码。
pub const FINGERPRINT_LEN: usize = 16;

impl Clone for Identity {
    fn clone(&self) -> Self {
        Self {
            cert_chain: self.cert_chain.clone(),
            private_key: self.private_key.clone_key(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

pub struct Identity {
    /// DER 编码的证书链（只有一张自签证书）
    pub cert_chain: Vec<CertificateDer<'static>>,
    /// PKCS#8 私钥
    pub private_key: PrivateKeyDer<'static>,
    /// 证书指纹（hex）
    pub fingerprint: String,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

/// 从 DER 证书算指纹。证书和指纹必须用同一个函数，否则会自相矛盾。
pub fn fingerprint_of(cert_der: &[u8]) -> String {
    let hash = blake3::hash(cert_der);
    let bytes = &hash.as_bytes()[..FINGERPRINT_LEN];
    crate::bytes::to_hex(bytes)
}

const IDENTITY_FILE: &str = "identity.bin";
const IDENTITY_MAGIC: &[u8; 8] = b"SRIDENT1";

/// 把身份编码成单一字节串。证书与私钥必须一起发布，否则会出现错配。
fn encode_identity(id: &Identity) -> Vec<u8> {
    let cert = id.cert_chain[0].as_ref();
    let key: &[u8] = match &id.private_key {
        PrivateKeyDer::Pkcs8(k) => k.secret_pkcs8_der(),
        _ => &[],
    };
    let mut out = Vec::with_capacity(8 + 8 + cert.len() + key.len());
    out.extend_from_slice(IDENTITY_MAGIC);
    out.extend_from_slice(&(cert.len() as u32).to_le_bytes());
    out.extend_from_slice(cert);
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out
}

/// 解析身份文件，并做结构校验。任何异常都返回错误，绝不"猜测"。
fn decode_identity(mut bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    if bytes.len() < 8 || &bytes[..8] != IDENTITY_MAGIC {
        return Err(Error::protocol("身份文件格式不正确"));
    }
    bytes = &bytes[8..];
    let cert = take_block(&mut bytes)?;
    let key = take_block(&mut bytes)?;
    if cert.is_empty() || key.is_empty() {
        return Err(Error::protocol("身份文件内容不完整"));
    }
    Ok((cert, key))
}

fn take_block(bytes: &mut &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 4 {
        return Err(Error::protocol("身份文件被截断"));
    }
    let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    *bytes = &bytes[4..];
    if bytes.len() < len {
        return Err(Error::protocol("身份文件被截断"));
    }
    let out = bytes[..len].to_vec();
    *bytes = &bytes[len..];
    Ok(out)
}

impl Identity {
    /// 生成一份全新的自签身份。
    pub fn generate() -> Result<Self> {
        let key_pair = rcgen::KeyPair::generate().map_err(|e| Error::Other(e.into()))?;

        let params = rcgen::CertificateParams::new(vec![SERVER_NAME.to_string()])
            .map_err(|e| Error::Other(e.into()))?;

        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| Error::Other(e.into()))?;

        let cert_der = cert.der().clone();
        let fingerprint = fingerprint_of(&cert_der);

        let key_der = key_pair.serialize_der();
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));

        Ok(Self {
            cert_chain: vec![cert_der],
            private_key,
            fingerprint,
        })
    }

    /// 从磁盘加载，不存在则生成并落盘。
    /// 加载已有身份；不存在则创建一份，并把写入做成原子操作。
    ///
    /// 并发安全是这里的关键：多台机器/多个进程可能同时启动并指向同一个
    /// 用户数据目录。如果实现允许"读失败就重新生成"，就会出现两台机器各自
    /// 生成不同身份、连接时指纹不匹配的情况——用户完全无从理解。
    /// 因此策略是：**只有在确认"确实还没有身份"时才创建，其余一律重试读取**。
    /// 加载已有身份；不存在则创建一份。
    ///
    /// 并发安全的关键在于**把证书和私钥放在同一个文件里**。
    /// 早先的实现在两个文件里分别保存，并用两次 `rename` 发布——这在多进程
    /// 同时启动时会交叉发布，出现"证书来自 A、私钥来自 B"的错配，表现为
    /// `KeyMismatch` 或"指纹不匹配"。一对文件无法原子发布，一个文件可以。
    ///
    /// 文件格式：magic(8) + u32 cert_len + cert_der + u32 key_len + key_der
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let path = dir.join(IDENTITY_FILE);

        // 只要正式文件存在，就一定能完整读到（发布是原子的）
        if path.exists() {
            for attempt in 0..50u32 {
                match Self::load(&path) {
                    Ok(id) => return Ok(id),
                    Err(_) if attempt < 49 => {
                        // 极少数情况下会遇到别人正在覆盖的瞬间，稍等再试
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // 确认还没有身份：自己生成一份，写入临时文件后一次性 rename 发布
        let id = Self::generate()?;
        let tmp = dir.join(format!("{IDENTITY_FILE}.{}.tmp", std::process::id()));
        fs::write(&tmp, encode_identity(&id)).map_err(|e| Error::io(&tmp, e))?;

        match fs::rename(&tmp, &path) {
            Ok(()) => Ok(id),
            Err(_) => {
                // 别人抢先发布了：以先到者为准，保证所有人的指纹一致
                let _ = fs::remove_file(&tmp);
                Self::load(&path)
            }
        }
    }

    fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
        let (cert_bytes, key_bytes) = decode_identity(&bytes)?;
        let cert_der = CertificateDer::from(cert_bytes);
        let fingerprint = fingerprint_of(&cert_der);
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));
        Ok(Self {
            cert_chain: vec![cert_der],
            private_key,
            fingerprint,
        })
    }

    /// 用户数据目录，按平台约定。
    pub fn default_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("SR_DATA_DIR") {
            return PathBuf::from(dir);
        }
        let base = dirs_fallback();
        base.join("strange-room")
    }
}

fn dirs_fallback() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata);
        }
    }
    #[cfg(unix)]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share");
        }
    }
    std::env::temp_dir()
}

pub fn log_warn(msg: &str) {
    eprintln!("[warn] {msg}");
}

pub fn log_info(msg: &str) {
    println!("[info] {msg}");
}

/// 握手时用于校验对端证书的验签器。
///
/// 只做一件事：把对端证书的指纹和二维码里带来的指纹比对。
/// 不做链验证、不看域名——因为这里根本没有 CA 参与，信任完全来自带外信道。
#[derive(Debug)]
pub struct FingerprintVerifier {
    expected: String,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl FingerprintVerifier {
    pub fn new(expected: impl Into<String>) -> Self {
        Self {
            expected: expected.into(),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual = fingerprint_of(end_entity.as_ref());
        if actual.eq_ignore_ascii_case(&self.expected) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "证书指纹不匹配：期望 {}，实际 {}",
                self.expected, actual
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_gives_consistent_fingerprint() {
        let id = Identity::generate().unwrap();
        assert_eq!(id.fingerprint.len(), FINGERPRINT_LEN * 2);
        assert_eq!(id.fingerprint, fingerprint_of(id.cert_chain[0].as_ref()));
    }

    #[test]
    fn two_identities_differ() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn load_or_create_persists_fingerprint() {
        let dir = std::env::temp_dir().join(format!("sr-id-{}", uuid::Uuid::new_v4()));
        let first = Identity::load_or_create(&dir).unwrap();
        let second = Identity::load_or_create(&dir).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        std::fs::remove_dir_all(&dir).ok();
    }
}
