//! qr — 终端二维码
//!
//! 两种模式:
//! 1. 在线模式(net feature): 调 api.qrserver.com 取 ASCII/Unicode 渲染,可靠
//! 2. 离线桩: 提示 URL,手机浏览器打开看
//!
//! 用法:
//!   rxt qr "https://example.com"
//!   rxt qr "任何文本"

#[cfg(feature = "http")]
pub fn run(text: &str, _invert: bool, _compact: bool) -> anyhow::Result<()> {
    use std::io::Read;
    if text.trim().is_empty() {
        anyhow::bail!("内容为空");
    }
    // 用 qrcode.show 的 txt API(返回 ASCII art)
    let url = format!("https://qrcode.show/txt/{}", urlencoding::encode(text));
    let agent = ureq::agent();
    match agent.get(&url).call() {
        Ok(resp) => {
            let mut body = String::new();
            resp.into_body().into_reader().read_to_string(&mut body)?;
            if body.trim().is_empty() {
                println!("（API 返回空,内容可能过长）");
            } else {
                // 去掉每行末尾多余空格
                let cleaned: Vec<&str> = body.lines().map(|l| l.trim_end()).collect();
                println!("{}", cleaned.join("\n"));
            }
            Ok(())
        }
        Err(e) => {
            // 降级: 给出可访问的链接
            anyhow::bail!(
                "在线生成失败({})。\n手机访问查看:\nhttps://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}",
                e, urlencoding::encode(text)
            )
        }
    }
}

#[cfg(not(feature = "http"))]
pub fn run(text: &str, _invert: bool, _compact: bool) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        anyhow::bail!("内容为空");
    }
    println!("⚠ 本地版(无 http feature)不能在线生成二维码。");
    println!("手机浏览器打开以下链接查看:");
    println!(
        "https://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}",
        urlencoding::encode(text)
    );
    Ok(())
}
