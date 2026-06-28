//! 星枢记忆桩 — no-net 编译模式
//! 完整实现在 mem.rs,本桩仅在关闭 net feature 时编译。

pub fn run_save(_content: &str, _category: &str, _importance: f64) -> anyhow::Result<()> {
    anyhow::bail!(
        "本 rxt 二进制未启用 net 功能(mem 记忆命令不可用)。\n\
         原因: 本地编译时关闭了 `net` feature(避开 ureq→ring→C 编译器依赖)。\n\
         如需星枢记忆,请用启用 net 的版本(虎虎上编译的 rxt)。"
    )
}

pub fn run_search(_query: &str, _top_k: usize) -> anyhow::Result<()> {
    anyhow::bail!("本 rxt 二进制未启用 net 功能(mem 记忆命令不可用)。")
}

pub fn run_stats() -> anyhow::Result<()> {
    anyhow::bail!("本 rxt 二进制未启用 net 功能(mem 记忆命令不可用)。")
}
