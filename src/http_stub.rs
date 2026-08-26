//! HTTP 桩 — no-net 编译模式
//! 完整实现在 http.rs,本桩仅在关闭 net feature 时编译。

use std::path::Path;

pub struct HttpOpts<'a> {
    pub method: &'a str,
    pub urls: &'a [String],
    pub headers: &'a [String],
    pub data: Option<&'a str>,
    pub json_body: bool,
    pub auth: Option<&'a str>,
    pub timeout: u64,
    pub show_headers: bool,
    pub body_only: bool,
    pub output: Option<&'a Path>,
    pub browser: Option<&'a str>,
    pub cookie_jar: Option<&'a Path>,
    pub cookies: &'a [String],
    pub user_agent: Option<&'a str>,
    pub text: bool,
    pub links: bool,
    pub budget: Option<usize>,
    pub form: &'a [String],
    pub no_probe: bool,
    pub cookie_json: Option<&'a str>,
    pub select: Option<&'a str>,
    pub session: Option<&'a str>,
    pub engine: Option<&'a str>,
    pub auth_hosts: &'a [String],
}

pub fn run(_opts: HttpOpts<'_>) -> anyhow::Result<()> {
    anyhow::bail!(
        "本 rxt 二进制未启用 http 功能。\n\
         编译: cargo build --release --features http\n\
         读浏览器 Cookie：`--browser firefox`（原生 sqlite）或 `--cookie-json`。"
    )
}
