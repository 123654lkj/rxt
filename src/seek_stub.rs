use std::path::Path;

pub fn run(
    _query: Option<&str>,
    _path: Option<&Path>,
    _index: bool,
    _stats: bool,
    _clear: bool,
    _top_k: usize,
    _language: Option<&str>,
    _json: bool,
) -> anyhow::Result<()> {
    anyhow::bail!("seek 命令需要 net feature (编译时加 --features net)")
}
