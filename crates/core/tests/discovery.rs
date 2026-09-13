//! 附近设备发现的端到端验证：真的注册一个广播，再真的把它搜出来。
//!
//! 两个 daemon（一个注册、一个搜索）在同一次运行里互发多播，等价于两台设备在同一
//! 局域网里互相发现。实测在本仓库的开发机（云主机 + 虚拟网卡）上稳定通过。
//!
//! ⚠️ 这条测试**依赖环境的多播是否可用**。装在虚拟交换不转发组播的环境里、或者
//! 本机有别的程序占着 UDP 5353（抓包工具、某些 VPN 客户端）时会失败——那种失败
//! 不是代码问题。判断方法：一台机器 `coa send 文件`，另一台 `coa discover`，
//! 后者应当列出前者，并显示和前者屏幕一致的验证码。

use std::time::Duration;

use coalesce_core::qr::AddressHint;
use coalesce_core::{verification_code, Advertisement, CancelToken};

#[tokio::test(flavor = "multi_thread")]
async fn advertises_and_discovers_over_mdns() {
    let sid = format!("test-{}", std::process::id());
    let fp = "d9df290b3485672194535f40b457ca6d";

    // 必须有一块真实的局域网地址：只广播回环等于什么都没广播
    let lan_ip = coalesce_core::net::quic::local_address_hints(51234)
        .into_iter()
        .map(|h| h.host)
        .find(|h| h != "127.0.0.1")
        .expect("这台机器没有局域网地址，无法测试 mDNS 广播");

    let _ad = Advertisement::start(
        "测试主机",
        &sid,
        fp,
        51234,
        &[AddressHint {
            host: lan_ip.clone(),
            port: 51234,
        }],
    )
    .expect("注册广播失败");

    let hosts = coalesce_core::discover(Duration::from_secs(5), None, &CancelToken::new())
        .await
        .expect("搜索附近设备失败");

    let found = hosts
        .iter()
        .find(|h| h.sid == sid)
        .unwrap_or_else(|| panic!("没能发现刚注册的设备；实际发现：{hosts:?}"));

    assert_eq!(found.device_name, "测试主机");
    assert_eq!(found.fingerprint, fp);
    assert_eq!(found.code, verification_code(fp), "验证码必须和主机屏幕一致");
    assert_eq!(found.addrs.len(), 1);
    assert_eq!(found.addrs[0].host, lan_ip);
    assert_eq!(found.addrs[0].port, 51234, "端口必须来自 SRV 记录");

    // 转成载荷之后，接上下面就是完全一样的接收流程（这是刻意的设计）
    let payload = found.payload();
    assert_eq!(payload.sid, sid);
    assert_eq!(payload.fp, fp);
    assert!(payload.encode().is_ok(), "发现得到的设备必须能直接生成连接串");
}
