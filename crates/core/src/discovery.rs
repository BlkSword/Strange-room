//! 附近设备发现（mDNS / DNS-SD）。
//!
//! **定位：发现是便利，不是必要条件。** 二维码/连接串永远是兜底路径——这句话是
//! 这一块设计的起点。局域网里 mDNS 失效的方式太多了（访客网络开了 AP 隔离、
//! 企业 WiFi 禁组播、Windows 防火墙拦 UDP 5353、VPN 抢走默认路由），所以：
//!
//! - 发现失败时给出的必须是**能指导下一步**的话，而不是一句"没找到设备"；
//! - 传输路径不为发现做任何假设：`NearbyHost` 最后会被转成和扫码**完全一样**的
//!   `QrPayload`，后面走的是同一套接收流程（两套传输路径是 bug 的温床）。
//!
//! ## 信任模型（很重要）
//!
//! 二维码里的指纹是**带外**传过来的（屏幕 → 摄像头），这是我们的信任锚点。
//! mDNS 里的指纹是**和地址同一个信道**过来的：局域网里能伪造 mDNS 的人，也能
//! 同时伪造指纹。所以走发现路径时，指纹只能证明"我连上的就是广播里说的那台"，
//! 不能证明"广播里说的那台就是我想找的那台"。
//!
//! 为此每台设备都会显示一个**由指纹派生的 6 位验证码**，和主机屏幕上的完全一致
//! （`verification_code`）。想确认身份的人对一眼就够了——这是 Signal 安全码的
//! 思路，缩短到 6 位。不想对的用户得到的是"和扫码一样快"的体验，代价是信任
//! 降到 TOFU。这条取舍写在 README 的"安全"一节里，不藏着。

use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::cancel::CancelToken;
use crate::error::{Error, Result};
use crate::qr::{AddressHint, QrPayload};

/// 服务类型。用 `_udp`：实际承载是 QUIC（跑在 UDP 上）。
pub const SERVICE_TYPE: &str = "_coalesce._udp.local.";

/// TXT 里的协议版本。加它是为了将来能**安静地忽略**不认识的广播，
/// 而不是把新版本的设备当成损坏数据。
const TXT_VERSION: &str = "1";

/// 默认监听时长。mDNS 的查询是退避重发的（1s、2s、4s…），
/// 扫太短会漏设备，扫太长又让人干等——3 秒是这两者的折中。
pub const DEFAULT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

/// 由证书指纹派生的 6 位验证码，形如 `427 913`。
///
/// 用它而不是指纹原文，是因为**人眼比对**才是这里的用途：32 位十六进制没人会真去对，
/// 6 位数字一眼就能看出不一样。分成两组三位是手机号式的习惯写法，读起来更快。
pub fn verification_code(fingerprint: &str) -> String {
    let h = blake3::hash(format!("coalesce-verify:{fingerprint}").as_bytes());
    let b = h.as_bytes();
    let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

/// 附近正在分享的一台设备。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyHost {
    pub device_name: String,
    pub sid: String,
    pub fingerprint: String,
    /// 候选地址（已按"最可能连上"排序，同网段的排前面）
    pub addrs: Vec<AddressHint>,
    /// 与主机屏幕一致的 6 位验证码
    pub code: String,
}

impl NearbyHost {
    /// 转成和扫码一模一样的载荷——这样接收流程一行都不用改。
    pub fn payload(&self) -> QrPayload {
        QrPayload::new(
            self.sid.clone(),
            self.device_name.clone(),
            self.fingerprint.clone(),
            self.addrs.clone(),
        )
    }

    /// 列表里的一行：`名字 · 验证码 427 913 · 192.168.1.9:51234`
    pub fn display_line(&self) -> String {
        let addr = self
            .addrs
            .first()
            .map(|a| format!("{}:{}", a.host, a.port))
            .unwrap_or_else(|| "（没有可用地址）".to_string());
        format!("{} · 验证码 {} · {}", self.device_name, self.code, addr)
    }
}

/// 主机侧：把"我在分享"广播到局域网。
///
/// 生命周期就是分享的生命周期——`Drop` 时撤销广告（mDNS 会补一组 goodbye 包，
/// 别人的列表里会立刻消失）。所以调用方只要把它拿着别丢即可。
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertisement {
    /// 开始广播一次分享。
    ///
    /// `addrs` 里的回环地址会被过滤掉：广播 `127.0.0.1` 给别人毫无意义，
    /// 反而会让对方把候选地址浪费在一个永远连不上的地址上。
    pub fn start(
        device_name: &str,
        sid: &str,
        fingerprint: &str,
        port: u16,
        addrs: &[AddressHint],
    ) -> Result<Self> {
        let ips: Vec<IpAddr> = addrs
            .iter()
            .filter_map(|h| IpAddr::from_str(h.host.trim()).ok())
            .filter(|ip| !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast())
            .collect();
        if ips.is_empty() {
            return Err(Error::protocol(
                "没有可广播的局域网地址（只找到回环地址）——对方无法通过发现找到这台设备",
            ));
        }

        let daemon = ServiceDaemon::new()
            .map_err(|e| Error::protocol(format!("启动 mDNS 服务失败：{e}")))?;

        // 实例名必须**唯一**：会话 ID 天生唯一，用它就不会和同机的另一个实例顶掉。
        // 展示用的名字走 TXT 里的 name 字段，所以这里不需要"好读"。
        let instance = format!("coa-{sid}");
        let host_name = format!("coa-{}.local.", &sid[..sid.len().min(8)]);

        let mut props: HashMap<String, String> = HashMap::new();
        props.insert("v".to_string(), TXT_VERSION.to_string());
        props.insert("sid".to_string(), sid.to_string());
        props.insert("name".to_string(), truncate_utf8(device_name, 60));
        props.insert("fp".to_string(), fingerprint.to_string());

        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host_name, &ips[..], port, props)
            .map_err(|e| Error::protocol(format!("构造 mDNS 广播失败：{e}")))?;
        let fullname = info.get_fullname().to_string();

        // 注册本身失败（端口被占、名字非法）在这里立刻暴露。
        //
        // 注意：**注册成功不等于对方一定搜得到**——组播可能被防火墙或 AP 隔离拦住，
        // 那是环境问题，检测不到也不该让分享失败（二维码路径照样能用）。
        // 所以这里不做"确认能被发现"的二次探测，真正的验证留给用户在真实网络上
        // 用 `coa discover` 跑一次（见 README）。
        daemon
            .register(info)
            .map_err(|e| Error::protocol(format!("注册 mDNS 广播失败：{e}")))?;

        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// 扫一圈附近的设备，返回去重、排序后的列表。
///
/// 空列表不是错误：没有任何人在分享是**正常情况**，怎么表达"没找到"由调用方决定
/// （CLI 会补上排查建议，界面会显示引导）。
pub async fn discover(
    timeout: Duration,
    exclude_sid: Option<&str>,
    cancel: &CancelToken,
) -> Result<Vec<NearbyHost>> {
    let daemon = ServiceDaemon::new()
        .map_err(|e| Error::protocol(format!("启动 mDNS 服务失败：{e}")))?;
    let rx = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| Error::protocol(format!("无法开始搜索附近设备：{e}")))?;

    let deadline = Instant::now() + timeout;
    // 用 sid 去重：同一台设备可能从多张网卡广播，或者被解析多次
    let mut found: HashMap<String, NearbyHost> = HashMap::new();

    while Instant::now() < deadline {
        cancel.check()?;
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        // 切成小片等，取消才能立刻生效（而不是等满整个扫描时长）
        let slice = Duration::from_millis(120).min(deadline - now);
        match rx.recv_timeout(slice) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if let Some(host) = host_from_resolved(&info) {
                    if Some(host.sid.as_str()) == exclude_sid {
                        continue; // 自己发的广告，别列给自己
                    }
                    found.entry(host.sid.clone()).or_insert(host);
                }
            }
            // 其余事件（开始搜索、发现线报、服务消失）在"扫一圈"的场景里不需要处理
            Ok(_) => {}
            // 超时是正常的（还没人应答），继续等下一片
            Err(mdns_sd::RecvTimeoutError::Timeout) => {}
            Err(mdns_sd::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = daemon.shutdown();

    let mut list: Vec<NearbyHost> = found.into_values().collect();
    for host in &mut list {
        sort_addrs_by_likelihood(&mut host.addrs);
    }
    list.sort_by(|a, b| {
        a.device_name
            .cmp(&b.device_name)
            .then_with(|| a.sid.cmp(&b.sid))
    });
    Ok(list)
}

/// 从 mDNS 解析结果里取出我们要的字段。
///
/// 只做"翻译"，判断逻辑全在 `host_from_parts` 里——mdns-sd 的
/// `ResolvedService` 是 `non_exhaustive`，外部构造不出来，所以把逻辑放在
/// 能被单元测试直接喂数据的地方。
fn host_from_resolved(info: &mdns_sd::ResolvedService) -> Option<NearbyHost> {
    host_from_parts(
        info.get_property_val_str("v"),
        info.get_property_val_str("sid"),
        info.get_property_val_str("name"),
        info.get_property_val_str("fp"),
        info.port,
        info.addresses.iter().map(|ip| ip.to_string()),
    )
}

/// 把 mDNS 的字段拼成一台设备（地址是原始字符串，端口来自 SRV 记录）。
fn host_from_parts(
    version: Option<&str>,
    sid: Option<&str>,
    name: Option<&str>,
    fingerprint: Option<&str>,
    port: u16,
    raw_addrs: impl Iterator<Item = String>,
) -> Option<NearbyHost> {
    let addrs: Vec<AddressHint> = raw_addrs
        .filter_map(|raw| usable_ip(&raw))
        .map(|ip| AddressHint {
            host: ip.to_string(),
            port,
        })
        .collect();
    host_from_fields(version, sid, name, fingerprint, addrs)
}

/// 这个地址值不值得试？
///
/// 只要 IPv4，以及**不需要 scope 就能用**的 IPv6（全局/唯一本地）。链路本地
/// 地址（fe80::）必须带 `%接口` 才能连，而我们的连接串里没法表达 scope——
/// 把它放进候选列表只会浪费一轮连接超时。
fn usable_ip(raw: &str) -> Option<IpAddr> {
    let stripped = raw.split('%').next().unwrap_or(raw);
    let ip = IpAddr::from_str(stripped.trim()).ok()?;
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return None;
    }
    match ip {
        IpAddr::V4(_) => Some(ip),
        IpAddr::V6(v6) => {
            let o = v6.octets();
            let link_local = o[0] == 0xfe && (o[1] & 0xc0) == 0x80;
            if link_local {
                None
            } else {
                Some(ip)
            }
        }
    }
}

/// 把 TXT 字段拼成一台设备。字段缺失就返回 `None`。
///
/// 宁可少列一台设备，也不要把残缺信息拿去连接——那会变成一个用户完全看不懂的
/// 连接失败。版本号不匹配也在这里挡掉。
fn host_from_fields(
    version: Option<&str>,
    sid: Option<&str>,
    name: Option<&str>,
    fingerprint: Option<&str>,
    addrs: Vec<AddressHint>,
) -> Option<NearbyHost> {
    if version != Some(TXT_VERSION) {
        return None;
    }
    let sid = sid?.trim();
    let name = name?.trim();
    let fp = fingerprint?.trim();
    if sid.is_empty() || name.is_empty() || fp.is_empty() || addrs.is_empty() {
        return None;
    }
    Some(NearbyHost {
        device_name: name.to_string(),
        sid: sid.to_string(),
        fingerprint: fp.to_string(),
        addrs,
        code: verification_code(fp),
    })
}

/// 按"最可能连上"排序：和本机同网段的地址排前面。
///
/// 为什么要排：主机可能同时广播好几张网卡（有线、无线、VPN、Docker），
/// 而接收端是**挨个试**的，每个失败要等 8 秒连接超时。把同网段的放前面，
/// 第一发就中的概率大得多。
fn sort_addrs_by_likelihood(addrs: &mut [AddressHint]) {
    let locals: Vec<IpAddr> = crate::net::quic::local_address_hints(0)
        .into_iter()
        .filter_map(|h| IpAddr::from_str(h.host.trim()).ok())
        .collect();
    // sort_by_key 是稳定排序：同档内保持原顺序
    addrs.sort_by_key(|h| {
        let same = locals
            .iter()
            .any(|l| crate::diag::looks_same_subnet(*l, &h.host).unwrap_or(false));
        if same {
            0
        } else {
            1
        }
    });
}

/// 按字符（不是字节）截断，避免把多字节字符切一半。
fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = 0;
    for (idx, ch) in s.char_indices() {
        if idx + ch.len_utf8() > max_bytes {
            break;
        }
        end = idx + ch.len_utf8();
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(host: &str) -> AddressHint {
        AddressHint {
            host: host.to_string(),
            port: 51234,
        }
    }

    #[test]
    fn verification_code_is_six_digits_and_stable() {
        let fp = "d9df290b3485672194535f40b457ca6d";
        let code = verification_code(fp);
        let (a, b) = code.split_once(' ').expect("应当是 123 456 这种形式");
        assert_eq!(a.len(), 3, "{code}");
        assert_eq!(b.len(), 3, "{code}");
        assert!(a.chars().all(|c| c.is_ascii_digit()), "{code}");
        assert!(b.chars().all(|c| c.is_ascii_digit()), "{code}");
        // 同一个指纹必须永远得到同一个码（主机和接收端各算一次，必须对得上）
        assert_eq!(code, verification_code(fp), "同一指纹必须得到同一个码");
        // 不同指纹应当不同（这里只做抽样检查，不要求密码学意义上的保证）
        assert_ne!(code, verification_code("00000000000000000000000000000000"));
    }

    #[test]
    fn host_from_fields_requires_every_piece() {
        let good = || {
            host_from_fields(
                Some("1"),
                Some("sid-1"),
                Some("我的笔记本"),
                Some("fp-1"),
                vec![hint("10.0.0.5")],
            )
        };
        let host = good().expect("字段齐全时应当解析成功");
        assert_eq!(host.device_name, "我的笔记本");
        assert_eq!(host.sid, "sid-1");
        assert_eq!(host.code, verification_code("fp-1"));

        // 任何一项缺失都不列出来——残缺信息只会变成看不懂的连接失败
        assert!(host_from_fields(None, Some("s"), Some("n"), Some("f"), vec![hint("10.0.0.5")]).is_none());
        assert!(host_from_fields(Some("1"), None, Some("n"), Some("f"), vec![hint("10.0.0.5")]).is_none());
        assert!(host_from_fields(Some("1"), Some("s"), None, Some("f"), vec![hint("10.0.0.5")]).is_none());
        assert!(host_from_fields(Some("1"), Some("s"), Some("n"), None, vec![hint("10.0.0.5")]).is_none());
        assert!(host_from_fields(Some("1"), Some("s"), Some("n"), Some("f"), vec![]).is_none());
        // 版本不认识：安静跳过，而不是当坏数据报错
        assert!(host_from_fields(Some("2"), Some("s"), Some("n"), Some("f"), vec![hint("10.0.0.5")]).is_none());
        // 空白串等同缺失
        assert!(host_from_fields(Some("1"), Some("  "), Some("n"), Some("f"), vec![hint("10.0.0.5")]).is_none());
        // 两头空白要去掉（mDNS 的 TXT 值可能有）
        let host = host_from_fields(Some("1"), Some(" s "), Some(" n "), Some(" f "), vec![hint("10.0.0.5")])
            .expect("应当容错空白");
        assert_eq!(host.sid, "s");
        assert_eq!(host.device_name, "n");
    }

    #[test]
    fn usable_ip_filters_out_addresses_we_cannot_connect_to() {
        assert_eq!(usable_ip("192.168.1.9"), Some("192.168.1.9".parse().unwrap()));
        assert_eq!(usable_ip("127.0.0.1"), None, "回环地址对别人没意义");
        assert_eq!(usable_ip("0.0.0.0"), None);
        assert_eq!(usable_ip("224.0.0.251"), None, "组播地址不是主机地址");
        // 链路本地 IPv6 必须带 scope 才能连，而连接串里表达不了 scope
        assert_eq!(usable_ip("fe80::1%以太网"), None);
        assert_eq!(usable_ip("fe80::1"), None);
        assert_eq!(usable_ip("2001:db8::1"), Some("2001:db8::1".parse().unwrap()));
        assert_eq!(usable_ip("不是地址"), None);
    }

    #[test]
    fn truncate_utf8_never_splits_a_character() {
        assert_eq!(truncate_utf8("abc", 10), "abc");
        // 每个汉字 3 字节：截到 7 字节只能放下两个汉字
        assert_eq!(truncate_utf8("中文名字测试", 7), "中文");
        assert_eq!(truncate_utf8("中文", 3), "中");
        assert_eq!(truncate_utf8("中文", 0), "");
    }

    #[test]
    fn display_line_shows_name_code_and_address() {
        let host = NearbyHost {
            device_name: "笔记本".into(),
            sid: "s".into(),
            fingerprint: "fp".into(),
            addrs: vec![hint("10.0.0.5")],
            code: verification_code("fp"),
        };
        let line = host.display_line();
        assert!(line.contains("笔记本"), "{line}");
        assert!(line.contains(&host.code), "{line}");
        assert!(line.contains("10.0.0.5:51234"), "{line}");

        // 没有地址时也不能 panic（列表渲染路径上出现过就麻烦了）
        let no_addr = NearbyHost {
            addrs: vec![],
            ..host
        };
        assert!(no_addr.display_line().contains("没有可用地址"));
    }

#[test]
    fn host_from_parts_uses_the_srv_port_and_drops_junk_addresses() {
        let raw = vec![
            "fe80::1%以太网".to_string(), // 链路本地：连接串里表达不了 scope
            "127.0.0.1".to_string(),      // 回环：对别人没意义
            "不是地址".to_string(),
            "10.0.0.5".to_string(),
        ];
        let host = host_from_parts(
            Some("1"),
            Some("sid-9"),
            Some("台式机"),
            Some("fp-9"),
            51234,
            raw.into_iter(),
        )
        .expect("有一个可用地址就应当解析成功");

        assert_eq!(host.addrs.len(), 1, "只有可用的地址该留下：{:?}", host.addrs);
        assert_eq!(host.addrs[0].host, "10.0.0.5");
        assert_eq!(host.addrs[0].port, 51234, "端口必须来自 SRV 记录");
        assert_eq!(host.code, verification_code("fp-9"));

        // 一个可用地址都没有 → 不列出来（否则用户点进去只会得到一轮连接超时）
        let none = host_from_parts(
            Some("1"),
            Some("sid-9"),
            Some("台式机"),
            Some("fp-9"),
            51234,
            vec!["127.0.0.1".to_string()].into_iter(),
        );
        assert!(none.is_none());
    }

    #[test]
    fn advertisement_refuses_when_there_is_nothing_worth_broadcasting() {
        // 只有回环地址时应当直接报错：广播 127.0.0.1 给别人毫无意义
        let err = Advertisement::start(
            "本机",
            "sid-1",
            "fp-1",
            51234,
            &[hint("127.0.0.1")],
        )
        .err()
        .expect("只给回环地址时应当拒绝");
        assert!(
            err.to_string().contains("回环"),
            "错误信息要说清原因：{err}"
        );
    }


    #[test]
    fn payload_carries_everything_the_receive_flow_needs() {
        let host = NearbyHost {
            device_name: "笔记本".into(),
            sid: "8f3c".into(),
            fingerprint: "d9df".into(),
            addrs: vec![hint("10.0.0.5")],
            code: verification_code("d9df"),
        };
        let payload = host.payload();
        assert_eq!(payload.sid, "8f3c");
        assert_eq!(payload.name, "笔记本");
        assert_eq!(payload.fp, "d9df");
        assert_eq!(payload.addrs.len(), 1);
        assert_eq!(payload.addrs[0].host, "10.0.0.5");
        assert_eq!(payload.addrs[0].port, 51234);
    }
}
