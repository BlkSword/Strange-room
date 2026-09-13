//! 引导页：让"对方还没有客户端"这件事不再卡住流程。
//!
//! 这是"零准备"最后一块拼图。在这之前，访客侧必须先有 `coa` 才能接收——而现实里
//! 对方多半什么都没装。所以主机在分享时顺手开一个**只读**的小 HTTP 服务：
//!
//! - `GET /`          一页说明：怎么下载、怎么用（含可复制的连接串）
//! - `GET /payload`   连接串纯文本（给脚本用）
//! - `GET /download`  客户端本体（当前正在运行的这个可执行文件）
//!
//! 访客的动作因此变成：**浏览器打开一个地址 → 下载 → 双击**（无参数运行就是
//! "发现附近设备并接收"，见 CLI）。不需要安装、不需要账号、不需要联网下载。
//!
//! ## 安全
//!
//! 这里服务的东西**只有**三样：一页静态 HTML、连接串、以及客户端可执行文件本身
//! （它本来就是公开分发的）。没有目录遍历、没有用户文件、没有上传口。
//! 页面和连接串本来就是屏幕上的公开信息；可执行文件是我们主动要送出去的东西。
//! 唯一暴露面是"同一局域网的人可以反复下载这个 exe"——可接受，而且服务随分享
//! 一起停（`BootstrapServer` 被 drop 时任务中止）。
//!
//! 请求解析刻意做得很小：只认 `GET`，只认三个路径，请求头最多读 8KB，10 秒超时。
//! 这不是通用 web 服务器，也不该被当成通用 web 服务器。

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::error::{Error, Result};

/// 请求头上限。超过就断开：正常浏览器一个 GET 请求远小于这个数。
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// 单个连接的读写超时。
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// 正在运行的引导页。
///
/// `Drop` 会中止服务任务——分享结束，页面也就该没了。
pub struct BootstrapServer {
    port: u16,
    /// 局域网地址（用于拼给人看的 URL）。拿不到时为 None。
    lan_ip: Option<String>,
    task: tokio::task::JoinHandle<()>,
}

impl BootstrapServer {
    /// 起一个引导页。`client_bytes` 是要分发的客户端本体。
    pub async fn start(
        payload: String,
        device_name: String,
        client_bytes: Vec<u8>,
    ) -> Result<Self> {
        // 绑定 0.0.0.0（和 QUIC 监听同样的暴露面），端口交给系统挑
        let listener = TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|e| Error::protocol(format!("引导页无法监听端口：{e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::protocol(format!("引导页无法获取端口：{e}")))?
            .port();

        let lan_ip = crate::net::quic::local_address_hints(port)
            .into_iter()
            .map(|h| h.host)
            .find(|h| h != "127.0.0.1");

        let page = Arc::new(build_page(&payload, &device_name, port));
        let client = Arc::new(client_bytes);
        let payload_for_route = payload.clone();

        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _peer)) = listener.accept().await else {
                    // 监听器出错就停：宁可没有引导页，也不要空转刷日志
                    break;
                };
                let page = Arc::clone(&page);
                let client = Arc::clone(&client);
                let payload = payload_for_route.clone();
                tokio::spawn(async move {
                    let _ = serve(stream, page, client, payload).await;
                });
            }
        });

        Ok(Self { port, lan_ip, task })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// 给用户念/发出去的地址。优先用局域网地址——访客就是从别的设备打开的。
    pub fn url(&self) -> String {
        match &self.lan_ip {
            Some(ip) => format!("http://{ip}:{}", self.port),
            None => format!("http://127.0.0.1:{}", self.port),
        }
    }
}

impl Drop for BootstrapServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: tokio::net::TcpStream,
    page: Arc<String>,
    client: Arc<Vec<u8>>,
    payload: String,
) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let response = match path {
        "/" | "/index.html" => Response::html(page.as_str()),
        "/payload" => Response::text(&payload),
        "/download" | "/coa.exe" | "/coa" => Response::attachment(&client, "coa.exe"),
        _ => Response::not_found(),
    };
    response.write_to(&mut stream).await
}

/// 读到一个完整的请求头（以空行结束）。只读头，不读 body——我们只支持 GET。
async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        if buf.len() > MAX_REQUEST_BYTES {
            return Err(Error::protocol("请求头过大，已断开"));
        }
        let deadline = tokio::time::sleep(CONNECTION_TIMEOUT);
        let n = tokio::select! {
            r = stream.read(&mut chunk) => {
                r.map_err(|e| Error::protocol(format!("读取请求失败：{e}")))?
            }
            _ = deadline => return Err(Error::protocol("读取请求超时")),
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    disposition: Option<String>,
    body: Vec<u8>,
}

impl Response {
    fn html(body: &str) -> Self {
        Self {
            status: "200 OK",
            content_type: "text/html; charset=utf-8",
            disposition: None,
            body: body.as_bytes().to_vec(),
        }
    }

    fn text(body: &str) -> Self {
        Self {
            status: "200 OK",
            content_type: "text/plain; charset=utf-8",
            disposition: None,
            body: body.as_bytes().to_vec(),
        }
    }

    fn attachment(body: &[u8], filename: &str) -> Self {
        Self {
            status: "200 OK",
            content_type: "application/octet-stream",
            // 文件名固定为 ASCII，免得再折腾 header 编码
            disposition: Some(format!("attachment; filename=\"{filename}\"")),
            body: body.to_vec(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: "404 Not Found",
            content_type: "text/plain; charset=utf-8",
            disposition: None,
            body: "没有这个页面。引导页只有三个地址：/ 、/payload 、/download\n"
                .as_bytes()
                .to_vec(),
        }
    }

    async fn write_to(self, stream: &mut tokio::net::TcpStream) -> Result<()> {
        let mut head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
            self.status,
            self.content_type,
            self.body.len()
        );
        if let Some(d) = self.disposition {
            head.push_str(&format!("Content-Disposition: {d}\r\n"));
        }
        head.push_str("\r\n");

        let write = async {
            stream
                .write_all(head.as_bytes())
                .await
                .map_err(|e| Error::protocol(format!("写响应失败：{e}")))?;
            stream
                .write_all(&self.body)
                .await
                .map_err(|e| Error::protocol(format!("写响应体失败：{e}")))?;
            let _ = stream.shutdown().await;
            Ok::<(), Error>(())
        };
        tokio::time::timeout(CONNECTION_TIMEOUT, write)
            .await
            .map_err(|_| Error::protocol("写响应超时"))?
    }
}

/// 引导页。刻意写成单页纯静态：不引外部资源（对方的网络可能只连得上我们这一台），
/// 不写 cookie，除一个"复制"按钮外不跑脚本。
fn build_page(payload: &str, device_name: &str, port: u16) -> String {
    let name = escape_html(device_name);
    let payload_attr = escape_html(payload);
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{name} 想给你传文件</title>
<style>
  :root {{ color-scheme: light dark; --fg:#111; --bg:#fff; --line:#e3e3e6; --muted:#666; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --fg:#eee; --bg:#16181a; --line:#2c3033; --muted:#9aa0a6; }} }}
  body {{ margin:0; padding:32px 20px; background:var(--bg); color:var(--fg);
         font:16px/1.6 -apple-system,"Segoe UI","Microsoft YaHei",sans-serif; }}
  main {{ max-width:640px; margin:0 auto; }}
  h1 {{ font-size:22px; margin:0 0 4px; }}
  p.sub {{ color:var(--muted); margin:0 0 24px; }}
  ol {{ padding-left:1.2em; }}
  li {{ margin:10px 0; }}
  .card {{ border:1px solid var(--line); border-radius:12px; padding:16px; margin:16px 0; }}
  code {{ font-family:ui-monospace,Consolas,monospace; font-size:14px; word-break:break-all; }}
  .payload {{ display:block; padding:10px; border:1px solid var(--line); border-radius:8px;
              background:rgba(127,127,127,.08); margin:8px 0; }}
  a.dl {{ display:inline-block; padding:10px 18px; border-radius:8px; background:#2b6cb0; color:#fff;
          text-decoration:none; font-weight:600; }}
  .note {{ color:var(--muted); font-size:14px; }}
</style>
</head>
<body>
<main>
  <h1>{name} 想给你传文件</h1>
  <p class="sub">局域网直传，不经过任何服务器。这个页面只在对方分享期间存在。</p>

  <div class="card">
    <strong>还没有客户端？</strong>
    <ol>
      <li><a class="dl" href="/download">下载客户端（单文件，免安装）</a></li>
      <li>双击运行它。<b>不带参数运行就是"接收"</b>，它会自动发现这台设备。</li>
    </ol>
    <p class="note">下载下来的是一个普通 exe：不需要安装、不写注册表、用完删掉即可。</p>
  </div>

  <div class="card">
    <strong>已经有客户端？</strong>
    <p class="note">在接收端执行下面这条命令（连接串就是二维码里的那一串）：</p>
    <code class="payload" id="p" data-payload="{payload_attr}">{payload_attr}</code>
    <button onclick="navigator.clipboard.writeText(document.getElementById('p').dataset.payload);this.textContent='已复制'"
            style="padding:8px 14px;border-radius:8px;border:1px solid var(--line);background:transparent;color:inherit;cursor:pointer">
      复制连接串
    </button>
  </div>

  <p class="note">引导页端口 {port}。分享结束后这个地址会立刻失效。</p>
</main>
</body>
</html>
"#
    )
}

/// 页面里会插入设备名，而设备名是用户自己起的（可能带尖括号）。
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
