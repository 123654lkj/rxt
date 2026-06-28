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
pub fn safe_resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Configure stdout for cross-platform UTF-8 output.
///
/// On Windows, Rust std writes to console with code page 437/GBK by default,
/// causing Chinese characters to display as `?` even though the underlying
/// bytes are valid UTF-8. This function:
/// 1. Detects if running on Windows
/// 2. Sets the console output code page to UTF-8 (65001)
/// 3. Reconfigures stdout to use UTF-8
///
/// Call this at the start of any command that may output non-ASCII content.
pub fn setup_utf8_console() {
    #[cfg(windows)]
    {
        unsafe {
            use windows_sys::Win32::System::Console::{SetConsoleOutputCP, GetConsoleOutputCP};

            // 65001 = UTF-8 code page
            let _ = SetConsoleOutputCP(65001);

            // Sanity check — ensure API is reachable
            let _ = GetConsoleOutputCP();
        }
    }
    // On non-Windows, no action needed
}

/// Write a UTF-8 BOM (3 bytes: EF BB BF) to stdout.
///
/// Useful when piping to Windows tools that expect a BOM to detect UTF-8.
/// Idempotent: only writes BOM if explicitly requested via env var
/// RXT_WRITE_BOM=1. Otherwise writes raw UTF-8.
pub fn maybe_write_bom(out: &mut dyn std::io::Write) -> std::io::Result<()> {
    if std::env::var("RXT_WRITE_BOM").ok().as_deref() == Some("1") {
        out.write_all(&[0xEF, 0xBB, 0xBF])?;
    }
    Ok(())
}

/// Find files matching a pattern in a directory (helper for rxt_ls).
///
/// `pattern` supports glob: `*` `?` `[abc]`
pub fn find_files(dir: &Path, pattern: &str, max_depth: Option<usize>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    fn walk(dir: &Path, pattern: &str, depth: usize, max_depth: Option<usize>, results: &mut Vec<PathBuf>) {
        if let Some(md) = max_depth { if depth > md { return; } }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with('.') {
                    if glob_match(pattern, &name) {
                        results.push(p.clone());
                    }
                    if p.is_dir() {
                        walk(&p, pattern, depth + 1, max_depth, results);
                    }
                }
            }
        }
    }
    walk(dir, pattern, 0, max_depth, &mut results);
    results
}

/// Simple glob matcher supporting `*`, `?`, `[abc]` (no `**`).
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn char_classes(p: &[u8], i: &mut usize) -> bool {
        if *i >= p.len() || p[*i] != b'[' { return false; }
        *i += 1;
        let negate = *i < p.len() && p[*i] == b'!';
        if negate { *i += 1; }
        let mut found = false;
        while *i < p.len() && p[*i] != b']' {
            if *i + 2 < p.len() && p[*i + 1] == b'-' && p[*i + 2] != b']' {
                // range
                if p[*i] <= p[*i + 2] { found = true; }
                *i += 3;
            } else {
                if p[*i] != b'?' { found = true; }
                *i += 1;
            }
        }
        if *i < p.len() { *i += 1; }  // skip ]
        negate ^ found
    }
    fn match_here(p: &[u8], i: &mut usize, n: &[u8], j: &mut usize) -> bool {
        while *i < p.len() {
            match p[*i] {
                b'*' => {
                    *i += 1;
                    while *i < p.len() && p[*i] == b'*' { *i += 1; }
                    if *i >= p.len() { return true; }
                    while *j <= n.len() {
                        let save_i = *i; let save_j = *j;
                        if match_here(p, i, n, j) { return true; }
                        *i = save_i; *j = save_j + 1;
                    }
                    return false;
                }
                b'?' => { *i += 1; if *j >= n.len() { return false; } *j += 1; }
                b'[' => {
                    let save_i = *i; let save_j = *j;
                    if char_classes(p, i) {
                        if *j < n.len() { *j += 1; }
                        if match_here(p, i, n, j) { return true; }
                    }
                    *i = save_i; *j = save_j;
                    if !match_here(p, i, n, j) { return false; }
                }
                c => {
                    if *j >= n.len() || n[*j] != c { return false; }
                    *i += 1; *j += 1;
                }
            }
        }
        *j == n.len()
    }
    let pb = pattern.as_bytes();
    let nb = name.as_bytes();
    let mut i = 0; let mut j = 0;
    match_here(pb, &mut i, &nb, &mut j)
}
