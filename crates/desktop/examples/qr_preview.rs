//! 为 UI 预览页生成一张真实的二维码 SVG。
//!
//! 刻意复用应用里同一段渲染逻辑（同一个依赖、同样的配色），这样预览页
//! 里的二维码和真机上的长得一样，而不是"画一个差不多的"。

fn main() {
    use coalesce_core::{AddressHint, QrPayload};

    let payload = QrPayload::new(
        "8f3c1d2e-7a41-4b90-9d2c-1e6f0a7b3c55",
        "张三的笔记本",
        "9a4f3c1d8e2b7056af13c9d4e8b20a67",
        vec![AddressHint {
            host: "192.168.1.24".into(),
            port: 51234,
        }],
    )
    .encode()
    .expect("编码失败");

    eprintln!("PAYLOAD={payload}");

    let code = qrcode::QrCode::new(payload.as_bytes()).expect("生成二维码失败");
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(260, 260)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#1a1a1a"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    println!("{svg}");
}
