//! 共享工具 — 多个命令共用的辅助函数

use std::path::{Path, PathBuf};

/// 查找项目根目录 (含 Cargo.toml 的最近祖先)
/// 用于 build/check/clean/size 等 Rust 项目命令
pub fn find_project_root(dir: Option<&str>) -> anyhow::Result<PathBuf> {
    let start = match dir {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()?,
    };
    let mut current = Some(start.as_path());
    while let Some(p) = current {
        if p.join("Cargo.toml").exists() {
            return Ok(p.to_path_buf());
        }
        current = p.parent();
    }
    anyhow::bail!("no Cargo.toml found in current or parent directories")
}
