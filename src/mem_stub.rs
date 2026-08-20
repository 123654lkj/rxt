//! 星枢记忆桩 — 无 http feature 时编译。
//! 完整实现见 mem.rs。

pub fn run_save(_content: &str, _category: &str, _importance: f64) -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。请用 default 特性编译。")
}
pub fn run_search(_query: &str, _top_k: usize) -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
pub fn run_stats() -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
pub fn run_extract(_t: &str, _f: &str, _d: bool) -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
pub fn run_bootstrap(_f: &str, _b: u32) -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
pub fn run_layers(_f: &str) -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
pub fn run_help() -> anyhow::Result<()> {
    anyhow::bail!("此 rxt 未启用 http feature（mem 不可用）。")
}
