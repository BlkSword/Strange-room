//! 引导页的端到端验证：真的起 HTTP 服务、真的发请求、真的把"客户端"下下来。
//!
//! 这条链路的意义是"对方还没有客户端"时不再卡住，所以它必须真的能下载到东西，
//! 而不只是"页面能渲染"。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 极简 HTTP GET：够用就行（不想为了测试引一个 HTTP 客户端依赖）。
async fn get(port: u16, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("连接引导页失败");
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.expect("发请求失败");

    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut buf))
        .await
        .expect("读响应超时")
        .expect("读响应失败");

    let text = String::from_utf8_lossy(&buf);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body_start = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(buf.len());
    (status, buf[body_start..].to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_the_page_payload_and_client_binary() {
    let payload = "srx1:测试用连接串";
    let client_bytes = vec![7u8; 4096];

    let server = sr_core::BootstrapServer::start(
        payload.to_string(),
        "我的笔记本".to_string(),
        client_bytes.clone(),
    )
    .await
    .expect("引导页起不来");
    let port = server.port();

    // 1) 页面：设备名、连接串、下载入口一个都不能少
    let (status, body) = get(port, "/").await;
    assert_eq!(status, 200);
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("我的笔记本"), "页面要显示设备名");
    assert!(html.contains(payload), "页面要包含连接串");
    assert!(html.contains("/download"), "页面要有下载入口");
    assert!(html.contains("不带参数"), "要告诉对方下载后怎么用");

    // 2) 连接串纯文本：脚本路径（curl 一下就能拿到，也是页面拿数据的方式）
    let (status, body) = get(port, "/payload").await;
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), payload);

    // 3) 客户端本体：逐字节一致——这是整件事的关键
    let (status, body) = get(port, "/download").await;
    assert_eq!(status, 200);
    assert_eq!(body, client_bytes, "下载下来的必须是同一个文件");

    // 4) 别的路径一律 404：这不是通用 web 服务器
    for path in ["/etc/passwd", "/../Cargo.toml", "/secrets"] {
        let (status, _) = get(port, path).await;
        assert_eq!(status, 404, "{path} 不该被服务");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn device_name_and_payload_are_escaped_in_the_page() {
    // 设备名是用户自己起的，连接串来自协议；两者都可能带尖括号，必须转义
    let server = sr_core::BootstrapServer::start(
        "srx1:x\"y<z>".to_string(),
        "<img src=x onerror=alert(1)>".to_string(),
        vec![1, 2, 3],
    )
    .await
    .expect("引导页起不来");

    let (_, body) = get(server.port(), "/").await;
    let html = String::from_utf8_lossy(&body);
    assert!(!html.contains("<img src=x"), "设备名必须被转义：{html}");
    assert!(html.contains("&lt;img"), "应当出现转义后的形式");
}
