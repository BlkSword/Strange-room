//! 网络自检：连不上时，告诉用户**为什么**、**该做什么**。
//!
//! 这个模块存在的理由很直接：局域网产品最常见的失败不是代码 bug，而是环境——
//! 不在同一个 WiFi、访客网络开了 AP 隔离、企业防火墙拦了 UDP。这些**代码解决
//! 不了**，只能给出可操作的指引。否则用户看到的只是一句"连接超时"，然后放弃。
//!
//! 做法分两步：
//! 1. **探测**：对二维码里的每个候选地址真发一次 QUIC 握手（拿指纹校验），
//!    得到"通 / 没响应 / 连上了但不是我们要的主机"这样确定的结论；
//! 2. **归类**：结合本机地址与探测结果，给出人话结论和按可能性排序的建议。
//!
//! 刻意不把"网段不同"当成硬结论：判断网段需要掩码，而跨平台拿掩码很啰嗦，
//! 用 /16 做启发式可能误判。所以它只作为**提示**出现，用来调整建议的顺序。

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::net::{quic, tls};
use crate::qr::QrPayload;

/// 单次探测的超时。自检要快——用户是在等答案，不是在等握手。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// 单个候选地址的探测结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeOutcome {
    /// 握手成功：地址可达，且主机证书指纹与二维码一致
    Reachable,
    /// 发出去的包没有任何回应（ip 不通、被防火墙丢弃、或不在同一网段）
    Timeout,
    /// 连上了但不是我们要的主机（指纹不符）——说明地址被别的东西占用了
    CertMismatch,
    /// 其它错误
    Failed(String),
}

impl ProbeOutcome {
    pub fn describe(&self) -> String {
        match self {
            ProbeOutcome::Reachable => "可以连接".into(),
            ProbeOutcome::Timeout => "无响应".into(),
            ProbeOutcome::CertMismatch => "有响应，但不是这台主机".into(),
            ProbeOutcome::Failed(e) => format!("失败（{e}）"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub address: String,
    pub outcome: ProbeOutcome,
}

/// 自检的总体结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// 至少有一个地址能连上
    Reachable,
    /// 本机没有可用的局域网地址
    NoLocalNetwork,
    /// 看着像不在同一网段
    WrongNetwork,
    /// 同网段但完全没响应：最典型的是防火墙或 AP 隔离
    LikelyBlocked,
    /// 有响应但不是我们要的主机
    WrongHost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    /// 本机用于外出的地址（探测不到时为 None）
    pub local_address: Option<IpAddr>,
    pub host_addresses: Vec<String>,
    pub probes: Vec<ProbeReport>,
    pub verdict: Verdict,
    /// 一句话结论
    pub summary: String,
    /// 按可能性排序的建议
    pub advice: Vec<String>,
}

impl Diagnosis {
    /// 给终端/界面用的完整报告文本。
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("本机地址：{}\n", match self.local_address {
            Some(ip) => ip.to_string(),
            None => "（没有检测到可用的局域网地址）".to_string(),
        }));
        out.push_str("主机地址：\n");
        for p in &self.probes {
            out.push_str(&format!("  {}  →  {}\n", p.address, p.outcome.describe()));
        }
        out.push_str(&format!("\n结论：{}\n", self.summary));
        if !self.advice.is_empty() {
            out.push_str("\n可以这样排查：\n");
            for (i, a) in self.advice.iter().enumerate() {
                out.push_str(&format!("  {}. {}\n", i + 1, a));
            }
        }
        out
    }
}

/// 从 `地址:端口` 里取出地址部分。
///
/// 需要这个函数是因为二维码里的地址是带端口的（`192.168.1.9:51234`、
/// `[fe80::1]:51234`），而网段比较只关心地址。之前忘了剥离端口，
/// 导致解析静默失败、网段判断失效——于是"不在同一网段"被误报成"可能被防火墙挡住"，
/// 把用户引向了错误的排查方向。
fn host_part(addr: &str) -> &str {
    let a = addr.trim();
    if let Some(rest) = a.strip_prefix('[') {
        // [fe80::1]:51234
        return rest.split(']').next().unwrap_or(rest);
    }
    // 192.168.1.9:51234 —— 只在冒号后面确实全是数字时才当端口，
    // 免得把裸 IPv6 地址切坏
    match a.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => a,
    }
}

/// 判断两个 IPv4 地址是否"看起来在同一网段"。
///
/// 用 /16 而不是 /24：家庭网络多是 /24，但企业网里 10.x 常用 /16 甚至 /8，
/// 用 /24 会把"同网段"误判成"不同网段"，那是最糟的错误方向——把人引去查
/// 错误的 WiFi。宁可漏判，也不要误判。
fn looks_same_subnet(local: IpAddr, host: &str) -> Option<bool> {
    let host_ip: IpAddr = host_part(host).parse().ok()?;
    match (local, host_ip) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let (a, b) = (a.octets(), b.octets());
            Some(a[0] == b[0] && a[1] == b[1])
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            // 链路本地地址（fe80::）前 8 字节是固定的，比对其余部分意义不大，
            // 这里只做保守判断：同为链路本地即视为"可能同网段"
            let (a, b) = (a.octets(), b.octets());
            let link_local = |o: [u8; 16]| o[0] == 0xfe && (o[1] & 0xc0) == 0x80;
            if link_local(a) && link_local(b) {
                Some(true)
            } else {
                Some(a[..8] == b[..8])
            }
        }
        // 一边 v4 一边 v6：说明双方的网络栈对不上，这本身就是个信号
        _ => Some(false),
    }
}

/// 从探测结果与本机地址推导结论和建议。
///
/// 抽成纯函数是为了能直接测——这段"该说什么话"的逻辑比握手本身更值得测，
/// 因为它直接决定用户下一步怎么做。
fn classify(local: Option<IpAddr>, probes: &[ProbeReport]) -> Diagnosis {
    let host_addresses: Vec<String> = probes.iter().map(|p| p.address.clone()).collect();

    let any_reachable = probes.iter().any(|p| p.outcome == ProbeOutcome::Reachable);
    let any_cert_mismatch = probes
        .iter()
        .any(|p| p.outcome == ProbeOutcome::CertMismatch);
    let any_responded = probes
        .iter()
        .any(|p| !matches!(p.outcome, ProbeOutcome::Timeout));

    // 网段提示：只要有一个候选地址看着同网段，就认为"看起来在同一网段"
    let subnet_hint = local.and_then(|l| {
        let mut same = false;
        let mut known = false;
        for p in probes {
            if let Some(s) = looks_same_subnet(l, &p.address) {
                known = true;
                same |= s;
            }
        }
        if known { Some(same) } else { None }
    });

    let (verdict, summary, advice) = if any_reachable {
        (
            Verdict::Reachable,
            "网络是通的，可以连接。".to_string(),
            vec!["如果仍然连不上，请让主机确认还在等待接收（分享界面没有关掉）。".to_string()],
        )
    } else if local.is_none() {
        (
            Verdict::NoLocalNetwork,
            "本机没有检测到可用的局域网地址。".to_string(),
            vec![
                "请先连上 WiFi 或有线网络，再重新扫码。".to_string(),
                "如果确实已联网，请检查是否有 VPN 把网络流量全部接管了——\
                 某些 VPN 会让本机看不到局域网。"
                    .to_string(),
            ],
        )
    } else if any_cert_mismatch {
        (
            Verdict::WrongHost,
            "地址上有东西在响应，但它不是这台主机。".to_string(),
            vec![
                "二维码很可能已经过期（主机重新开过分享），请让对方重新出示二维码。".to_string(),
                "如果对方没重开过，要警惕这个地址被别的程序占用了，或者有人冒充。".to_string(),
            ],
        )
    } else if subnet_hint == Some(false) {
        (
            Verdict::WrongNetwork,
            "你和主机看起来不在同一个网段。".to_string(),
            vec![
                "确认双方连的是同一个 WiFi。很多会议室的访客网络会把设备彼此隔离，\
                 就算名字一样也不行。"
                    .to_string(),
                "手机热点、随身 WiFi 常把设备隔离，改用同一个路由器试试。".to_string(),
                "如果主机插着网线、你连的是 WiFi，两者可能根本不在一个网段。".to_string(),
            ],
        )
    } else if !any_responded {
        (
            Verdict::LikelyBlocked,
            "地址可达性未知：对面完全没有响应。".to_string(),
            vec![
                "最常见的原因是主机上的防火墙拦住了。Windows 首次运行时会弹窗询问，\
                 如果当时点了\"取消\"，需要在防火墙设置里手动允许 Strange Room。"
                    .to_string(),
                "其次是 WiFi 的 AP 隔离（访客网络常见）：同一个 WiFi 下设备之间也不能互访。"
                    .to_string(),
                "确认主机上的分享还开着，并且屏幕上的二维码没有换过。".to_string(),
            ],
        )
    } else {
        (
            Verdict::LikelyBlocked,
            "有响应但没能建立连接，多半被中间环节挡住了。".to_string(),
            vec![
                "检查主机的防火墙是否允许了本程序（这是最常见的原因）。".to_string(),
                "确认双方在同一个局域网，且没有开启 AP 隔离。".to_string(),
            ],
        )
    };

    Diagnosis {
        local_address: local,
        host_addresses,
        probes: probes.to_vec(),
        verdict,
        summary,
        advice,
    }
}

/// 对二维码里的主机做一次自检。会真的发包，但不会传输任何文件。
pub async fn diagnose(payload: &QrPayload) -> Result<Diagnosis> {
    tls::ensure_crypto_provider();

    let local = quic::local_address_hints(0).into_iter().next().and_then(|h| {
        // local_address_hints 在探测不到时会兜底给回环，这里要区分开
        h.host.parse::<IpAddr>().ok().filter(|ip| !ip.is_loopback())
    });

    let mut probes = Vec::new();
    for hint in &payload.addrs {
        let addr = match tls::resolve_addr(&hint.host, hint.port).await {
            Ok(a) => a,
            Err(e) => {
                probes.push(ProbeReport {
                    address: hint.display(),
                    outcome: ProbeOutcome::Failed(e.to_string()),
                });
                continue;
            }
        };
        probes.push(ProbeReport {
            address: hint.display(),
            outcome: probe_one(addr, &payload.fp).await,
        });
    }

    Ok(classify(local, &probes))
}

/// 对单个地址做一次握手探测。成功即证明"可达且是指定主机"。
/// 对单个地址做一次握手探测。成功即证明"可达且是指定主机"。
async fn probe_one(addr: std::net::SocketAddr, fingerprint: &str) -> ProbeOutcome {
    let (client_cfg, fp_rejected) = match tls::client_config(fingerprint) {
        Ok(v) => v,
        Err(e) => return ProbeOutcome::Failed(e.to_string()),
    };
    let quinn_cfg = match quinn::crypto::rustls::QuicClientConfig::try_from(client_cfg) {
        Ok(c) => quinn::ClientConfig::new(std::sync::Arc::new(c)),
        Err(e) => return ProbeOutcome::Failed(e.to_string()),
    };
    let mut endpoint = match quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()) {
        Ok(e) => e,
        Err(e) => return ProbeOutcome::Failed(e.to_string()),
    };
    endpoint.set_default_client_config(quinn_cfg);

    let connecting = match endpoint.connect(addr, crate::identity::SERVER_NAME) {
        Ok(c) => c,
        Err(e) => return ProbeOutcome::Failed(e.to_string()),
    };

    match tokio::time::timeout(PROBE_TIMEOUT, connecting).await {
        Ok(Ok(conn)) => {
            // 握手成功就已经证明了可达性与身份，不需要再做应用层协议
            conn.close(0u32.into(), b"diagnose");
            ProbeOutcome::Reachable
        }
        Ok(Err(e)) => {
            // 证书不符是我们自己拒的，要和其它错误区分开：
            // 前者说明"有东西在那，但不是这台主机"
            if fp_rejected.load(std::sync::atomic::Ordering::SeqCst)
                || tls::is_certificate_error(&e)
            {
                ProbeOutcome::CertMismatch
            } else {
                ProbeOutcome::Failed(e.to_string())
            }
        }
        // 没有任何响应：对端不在、被丢包、或不在同一网段
        Err(_) => ProbeOutcome::Timeout,
    }
}

/// 便利函数：直接给一段连接串做自检。
pub async fn diagnose_str(payload_str: &str) -> Result<Diagnosis> {
    let payload = QrPayload::decode(payload_str)?;
    diagnose(&payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn probe(address: &str, outcome: ProbeOutcome) -> ProbeReport {
        ProbeReport {
            address: address.to_string(),
            outcome,
        }
    }

    #[test]
    fn same_subnet_uses_slash_16_to_avoid_false_alarms() {
        let local = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24));
        assert_eq!(looks_same_subnet(local, "192.168.1.99"), Some(true));
        // 不同 /24 但同 /16：企业网里这算同一网段，必须判为"同"
        assert_eq!(looks_same_subnet(local, "192.168.2.10"), Some(true));
        // 完全不同
        assert_eq!(looks_same_subnet(local, "10.0.0.5"), Some(false));
        // 解析不了就别下结论
        assert_eq!(looks_same_subnet(local, "not-an-ip"), None);
    }

    #[test]
    fn subnet_check_strips_the_port() {
        // 回归测试：二维码里的地址是带端口的，忘了剥离会让网段判断静默失效，
        // 进而把"不在同一网段"误报成"可能被防火墙挡住"
        let local = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24));
        assert_eq!(looks_same_subnet(local, "192.168.1.9:51234"), Some(true));
        assert_eq!(looks_same_subnet(local, "10.0.0.5:51234"), Some(false));
        assert_eq!(looks_same_subnet(local, "[fe80::1]:51234"), Some(false));
    }

    #[test]
    fn v4_vs_v6_is_treated_as_different_network() {
        let local = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24));
        assert_eq!(looks_same_subnet(local, "[fe80::1]"), Some(false));
    }

    #[test]
    fn reachable_wins_over_everything() {
        let d = classify(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24))),
            &[
                probe("10.0.0.5:1", ProbeOutcome::Timeout),
                probe("192.168.1.9:1", ProbeOutcome::Reachable),
            ],
        );
        assert_eq!(d.verdict, Verdict::Reachable);
    }

    #[test]
    fn no_local_address_gives_network_setup_advice() {
        let d = classify(None, &[probe("192.168.1.9:1", ProbeOutcome::Timeout)]);
        assert_eq!(d.verdict, Verdict::NoLocalNetwork);
        assert!(d.advice.iter().any(|a| a.contains("VPN")), "应提到 VPN");
    }

    #[test]
    fn different_subnet_points_at_wifi_first() {
        let d = classify(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24))),
            &[probe("10.0.0.5:1", ProbeOutcome::Timeout)],
        );
        assert_eq!(d.verdict, Verdict::WrongNetwork);
        assert!(d.advice[0].contains("同一个 WiFi"), "第一条应是 WiFi 提示");
    }

    #[test]
    fn same_subnet_but_silent_points_at_firewall_first() {
        let d = classify(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24))),
            &[probe("192.168.1.9:1", ProbeOutcome::Timeout)],
        );
        assert_eq!(d.verdict, Verdict::LikelyBlocked);
        assert!(
            d.advice[0].contains("防火墙"),
            "同网段无响应时第一条建议应先说防火墙，实际：{}",
            d.advice[0]
        );
    }

    #[test]
    fn cert_mismatch_mentions_expired_qr_code() {
        let d = classify(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24))),
            &[probe("192.168.1.9:1", ProbeOutcome::CertMismatch)],
        );
        assert_eq!(d.verdict, Verdict::WrongHost);
        assert!(d.advice[0].contains("过期"), "应提示二维码过期");
    }

    #[test]
    fn render_contains_all_probed_addresses() {
        let d = classify(
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 24))),
            &[
                probe("192.168.1.9:1", ProbeOutcome::Timeout),
                probe("192.168.1.10:1", ProbeOutcome::Reachable),
            ],
        );
        let text = d.render();
        assert!(text.contains("192.168.1.9:1"));
        assert!(text.contains("192.168.1.10:1"));
        assert!(text.contains("结论"));
    }
}
