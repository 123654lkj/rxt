//! HTTP 桩 — no-net 编译模式
//! 完整实现在 http.rs,本桩仅在关闭 net feature 时编译。

pub fn run(_method: &str, _url: &str, _headers: &[String], _data: Option<&str>, _json_body: bool, _auth: Option<&str>, _show_headers: bool, _body_only: bool) -> anyhow::Result<()> {
    anyhow::bail!(
        "本 rxt 二进制未启用 net 功能(http 命令不可用)。\n\
         原因: 本地编译时关闭了 `net` feature(避开 ureq→ring→C 编译器依赖)。\n\
         如需 HTTP 客户端，请用启用 net feature 编译的 rxt。"
    )
}
