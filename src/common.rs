//! 共享工具 — 多个命令共用的辅助函数
use std::path::{Path, PathBuf};

/// 查找项目根目录(找 Cargo.toml 的最近祖先)
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

/// Resolve a path, following symlinks when possible.
///
/// Note: on Windows, Rust std refuses to traverse reparse points/junctions
/// (os error 448). To work around this:
/// 1. Use the canonical path (e.g. C:\Users\foo\.mavis may be a junction to .minimax)
/// 2. Or call `rxt_safe_resolve` to resolve through Win32 API (uses `dunce` if available)
///
/// On non-Windows, returns canonicalize() result if successful, else original.
pub fn safe_resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
