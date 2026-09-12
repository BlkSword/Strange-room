//! TLS 配置：自签证书 + 指纹校验。
//!
//! rustls 需要显式安装一个 crypto provider。这里统一用 `ring`：
//! 它不需要 NASM/CMake 之类的构建工具，Windows 上也不会因为
//! `aws-lc-rs` 缺失工具链而编译失败。

use std::sync::{Arc, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::net::lookup_host;

use crate::error::{Error, Result};
use crate::identity::{fingerprint_of, Identity, SERVER_NAME};

/// QUIC 必须在 ClientHello 里带 ALPN；双方用同一个值，
/// 也顺便防止未来和其它 QUIC 服务串台。
pub const ALPN: &[u8] = b"sr1";

/// 全局安装一次 crypto provider。重复调用是安全的。
pub fn ensure_crypto_provider() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        // 已经装过就忽略错误
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// 主机（发送端）使用的 TLS 配置。
pub fn server_config(identity: &Identity) -> Result<rustls::ServerConfig> {
    ensure_crypto_provider();
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            identity.cert_chain.clone(),
            identity.private_key.clone_key(),
        )
        .map_err(|e| Error::protocol(format!("证书配置失败: {e}")))?;
    Ok(cfg)
}

/// 接收端使用的 TLS 配置。
///
/// 返回 `(config, fingerprint_mismatch_flag)`：第二个值是给调用方用的信号——
/// 校验器一旦因指纹不符拒绝了对端证书，就会把它置为 true。
///
/// 为什么需要这个标记：QUIC 在证书被拒时不会立刻向应用层报错，而是静默重试，
/// 直到握手超时。如果只依赖超时，用户要等十几秒才看到一个笼统的"连接超时"，
/// 而真实原因是"二维码过期/被换过"。有了这个标记，我们能立刻中止并给出准确提示。
pub fn client_config(
    expected_fingerprint: &str,
) -> Result<(rustls::ClientConfig, std::sync::Arc<std::sync::atomic::AtomicBool>)> {
    ensure_crypto_provider();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let verifier = Arc::new(IdentityVerifier::with_flag(
        expected_fingerprint,
        flag.clone(),
    )?);
    let cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok((cfg, flag))
}

/// 把 `host:port` 解析成 socket 地址。
///
/// 二维码里可能给的是 `[fe80::1]:45001` 这种形式，所以要支持 IPv6 字面量。
pub async fn resolve_addr(host: &str, port: u16) -> Result<std::net::SocketAddr> {
    let host = host.trim();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    // 先试字面量解析，失败再走 DNS（局域网里通常都是字面量）
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(std::net::SocketAddr::new(ip, port));
    }
    let mut addrs = lookup_host((host, port))
        .await
        .map_err(|e| Error::protocol(format!("无法解析地址 {host}:{port}（{e}）")))?;
    addrs
        .next()
        .ok_or_else(|| Error::protocol(format!("地址 {host}:{port} 没有解析出任何结果")))
}

/// 把 QUIC 错误翻译成用户能看懂的话。

/// 把 QUIC 连接错误翻译成用户能看懂的话。
///
/// 关键区分：加密握手失败（指纹不对/二维码过期）必须和网络不通分开报，
/// 否则用户会去查网线，而真实原因是二维码过期了。
pub fn friendly_connect_error(addr: std::net::SocketAddr, err: &quinn::ConnectionError) -> Error {
    let hint = classify_connect_error(err);
    Error::protocol(format!("连接 {addr} 失败：{hint}"))
}

/// 单独抽出来便于测试：只做分类，不做格式化。
pub fn classify_connect_error(err: &quinn::ConnectionError) -> String {
    match err {
        quinn::ConnectionError::TimedOut | quinn::ConnectionError::LocallyClosed =>
            "连接超时。请确认双方在同一个局域网、主机程序仍在运行，并检查主机的防火墙是否放行了本程序".to_string(),
        quinn::ConnectionError::ConnectionClosed(_) => "对端关闭了连接".to_string(),
        quinn::ConnectionError::TransportError(e) => {
            let code = e.code;
            let raw: u64 = code.into();
            if (0x100..0x200).contains(&raw) {
                "加密握手失败。通常是二维码已过期（主机重启过或换了会话），请重新扫描屏幕上的二维码".to_string()
            } else {
                format!("网络传输异常（{code}）。可能是 WiFi 不稳定，或中间网络设备拦截了 UDP")
            }
        }
        _ => "连接失败。请重新扫描二维码，或改用二维码里的地址手动连接".to_string(),
    }
}

/// 只认指纹的证书校验器。
///
/// 不做链验证、不看域名——因为这里根本没有 CA 参与，信任完全来自
/// 二维码这个带外信道（等价于 Signal 的安全码比对）。域名和链验证
/// 在这种模型下没有意义，反而会误伤自签证书。
#[derive(Debug)]
pub struct IdentityVerifier {
    expected: Arc<str>,
    provider: Arc<rustls::crypto::CryptoProvider>,
    /// 一旦因指纹不符拒绝过证书，就置为 true，供上层立刻中止连接。
    mismatch_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl IdentityVerifier {
    pub fn new(expected: &str) -> Result<Self> {
        ensure_crypto_provider();
        Ok(Self {
            expected: Arc::from(expected),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
            mismatch_flag: None,
        })
    }

    /// 带"指纹不符"信号的构造器：校验失败时会把 `flag` 置为 true，
    /// 让连接方立刻中止，而不是等握手超时。
    pub fn with_flag(
        expected: &str,
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self> {
        let mut v = Self::new(expected)?;
        v.mismatch_flag = Some(flag);
        Ok(v)
    }
}
impl ServerCertVerifier for IdentityVerifier {
    fn verify_server_cert(

        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let actual = fingerprint_of(end_entity.as_ref());
        if actual.eq_ignore_ascii_case(&self.expected) {
            Ok(ServerCertVerified::assertion())
        } else {
            // 记下这次拒绝，让连接方不必等到握手超时才失败
            if let Some(flag) = &self.mismatch_flag {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
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
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
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
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// `SERVER_NAME` 只用于握手占位，不参与信任判断。
pub fn placeholder_server_name() -> Result<ServerName<'static>> {
    ServerName::try_from(SERVER_NAME)
        .map_err(|e| Error::protocol(format!("内部错误：占位域名非法（{e}）")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_accepts_any_fingerprint_shape() {
        // 只要给一个合法长度的指纹就能构造配置；真正的校验发生在握手阶段。
        let fp = "a".repeat(crate::identity::FINGERPRINT_LEN * 2);
        let (cfg, flag) = client_config(&fp).expect("配置构造不应失败");
        assert!(
            !flag.load(std::sync::atomic::Ordering::SeqCst),
            "刚构造出来的校验器不该已经标记指纹不符"
        );
        let _ = cfg;
    }

    #[test]
    fn server_config_from_identity() {
        let id = Identity::generate().unwrap();
        assert!(server_config(&id).is_ok());
    }

    #[test]
    fn resolve_addr_handles_ipv4_literal() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let addr = rt.block_on(resolve_addr("192.168.1.5", 45001)).unwrap();
        assert_eq!(addr.port(), 45001);
        assert_eq!(addr.ip().to_string(), "192.168.1.5");
    }

    #[test]
    fn resolve_addr_handles_ipv6_brackets() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let addr = rt.block_on(resolve_addr("[::1]", 45002)).unwrap();
        assert_eq!(addr.port(), 45002);
        assert!(addr.is_ipv6());
    }

    #[test]
    fn placeholder_name_parses() {
        assert!(placeholder_server_name().is_ok());
    }
}

#[cfg(test)]
mod verifier_tests {
    use super::*;
    use std::sync::Arc;

    /// 直接驱动校验器，确认它：
    /// 1. 指纹相符时放行；
    /// 2. 指纹不符时拒绝，并且把 `mismatch_flag` 置起来，
    ///    让连接方可以立刻中止而不是等握手超时。
    #[test]
    fn verifier_accepts_matching_and_flags_mismatching_fingerprint() {
        ensure_crypto_provider();
        let id = crate::identity::Identity::generate().unwrap();
        let cert = id.cert_chain[0].clone();

        let server_name = placeholder_server_name().unwrap();
        let now = rustls::pki_types::UnixTime::since_unix_epoch(std::time::Duration::from_secs(
            1_700_000_000,
        ));

        // 指纹相符 → 放行
        let ok = IdentityVerifier::new(&id.fingerprint).unwrap();
        assert!(
            ok.verify_server_cert(&cert, &[], &server_name, &[], now)
                .is_ok(),
            "指纹相符时必须放行"
        );

        // 指纹不符 → 拒绝 + 置标记
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let bad = IdentityVerifier::with_flag(&"f".repeat(crate::identity::FINGERPRINT_LEN * 2), flag.clone())
            .unwrap();
        let result = bad.verify_server_cert(&cert, &[], &server_name, &[], now);
        assert!(result.is_err(), "指纹不符时必须拒绝");
        assert!(
            flag.load(std::sync::atomic::Ordering::SeqCst),
            "拒绝后必须置起 mismatch_flag，否则调用方只能等超时"
        );
    }

    /// 校验器支持的签名算法必须非空，否则 TLS 握手会因"没有可用算法"失败。
    #[test]
    fn verifier_reports_supported_schemes() {
        let v = IdentityVerifier::new(&"a".repeat(crate::identity::FINGERPRINT_LEN * 2)).unwrap();
        assert!(!v.supported_verify_schemes().is_empty());
    }
}
