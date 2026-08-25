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

/// stdout 是否接终端（Agent/管道捕获时为 false）。
pub fn stdout_is_tty() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        };
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if h.is_null() || h == (-1isize as _) {
                return false;
            }
            let mut mode = 0u32;
            GetConsoleMode(h, &mut mode) != 0
        }
    }
    #[cfg(not(windows))]
    {
        unsafe { libc::isatty(1) == 1 }
    }
}

/// 是否处于 Agent/管道捕获模式（自动 BOM，便于 PowerShell 识别 UTF-8）。
///
/// 触发条件（任一）：
/// - `RXT_AGENT=1` / `true` / `yes`
/// - `RXT_WRITE_BOM=1`
/// - Windows 且 stdout 非 TTY（被管道/工具捕获）
pub fn agent_capture_mode() -> bool {
    fn truthy(v: &str) -> bool {
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }
    if std::env::var("RXT_WRITE_BOM")
        .ok()
        .as_deref()
        .map(truthy)
        .unwrap_or(false)
    {
        return true;
    }
    if std::env::var("RXT_AGENT")
        .ok()
        .as_deref()
        .map(truthy)
        .unwrap_or(false)
    {
        return true;
    }
    #[cfg(windows)]
    {
        if !stdout_is_tty() {
            return true;
        }
    }
    false
}

/// Configure stdout for cross-platform UTF-8 output.
///
/// On Windows, Rust std writes to console with code page 437/GBK by default,
/// causing Chinese characters to display as `?` even though the underlying
/// bytes are valid UTF-8. This function:
/// 1. Sets console **input+output** code page to UTF-8 (65001)
/// 2. Enables virtual terminal processing when possible
///
/// Call this at the start of any command that may output non-ASCII content.
pub fn setup_utf8_console() {
    #[cfg(windows)]
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            use windows_sys::Win32::System::Console::{
                GetConsoleMode, GetStdHandle, SetConsoleCP, SetConsoleMode, SetConsoleOutputCP,
                ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
            };

            // 65001 = UTF-8 code page（输入+输出都设，修 PS 读入/显示中文乱码）
            let _ = SetConsoleOutputCP(65001);
            let _ = SetConsoleCP(65001);

            for handle_id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let h = GetStdHandle(handle_id);
                if h.is_null() || h == (-1isize as _) {
                    continue;
                }
                let mut mode = 0u32;
                if GetConsoleMode(h, &mut mode) != 0 {
                    let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        });
    }
    // On non-Windows, no action needed
}

/// Write a UTF-8 BOM (3 bytes: EF BB BF) to stdout when in agent/capture mode.
///
/// 解决：Agent/PowerShell 管道捕获时把 UTF-8 中文当系统 ANSI 解码导致乱码。
/// 关闭：`RXT_NO_BOM=1`（即使管道也不写 BOM）。
pub fn maybe_write_bom(out: &mut dyn std::io::Write) -> std::io::Result<()> {
    if std::env::var("RXT_NO_BOM")
        .ok()
        .as_deref()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
    {
        return Ok(());
    }
    if agent_capture_mode() {
        out.write_all(&[0xEF, 0xBB, 0xBF])?;
    }
    Ok(())
}

/// 带 UTF-8 控制台 + 可选 BOM 的 println 封装（pack/info 等入口复用）。
pub fn println_utf8(s: &str) {
    setup_utf8_console();
    let mut stdout = std::io::stdout().lock();
    let _ = maybe_write_bom(&mut stdout);
    use std::io::Write;
    let _ = writeln!(stdout, "{}", s);
}

/// Find files matching a pattern in a directory (helper for rxt_ls).
///
/// `pattern` supports glob: `*` `?` `[abc]`
pub fn find_files(dir: &Path, pattern: &str, max_depth: Option<usize>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    fn walk(
        dir: &Path,
        pattern: &str,
        depth: usize,
        max_depth: Option<usize>,
        results: &mut Vec<PathBuf>,
    ) {
        if let Some(md) = max_depth {
            if depth > md {
                return;
            }
        }
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
        if *i >= p.len() || p[*i] != b'[' {
            return false;
        }
        *i += 1;
        let negate = *i < p.len() && p[*i] == b'!';
        if negate {
            *i += 1;
        }
        let mut found = false;
        while *i < p.len() && p[*i] != b']' {
            if *i + 2 < p.len() && p[*i + 1] == b'-' && p[*i + 2] != b']' {
                // range
                if p[*i] <= p[*i + 2] {
                    found = true;
                }
                *i += 3;
            } else {
                if p[*i] != b'?' {
                    found = true;
                }
                *i += 1;
            }
        }
        if *i < p.len() {
            *i += 1;
        } // skip ]
        negate ^ found
    }
    fn match_here(p: &[u8], i: &mut usize, n: &[u8], j: &mut usize) -> bool {
        while *i < p.len() {
            match p[*i] {
                b'*' => {
                    *i += 1;
                    while *i < p.len() && p[*i] == b'*' {
                        *i += 1;
                    }
                    if *i >= p.len() {
                        return true;
                    }
                    while *j <= n.len() {
                        let save_i = *i;
                        let save_j = *j;
                        if match_here(p, i, n, j) {
                            return true;
                        }
                        *i = save_i;
                        *j = save_j + 1;
                    }
                    return false;
                }
                b'?' => {
                    *i += 1;
                    if *j >= n.len() {
                        return false;
                    }
                    *j += 1;
                }
                b'[' => {
                    let save_i = *i;
                    let save_j = *j;
                    if char_classes(p, i) {
                        if *j < n.len() {
                            *j += 1;
                        }
                        if match_here(p, i, n, j) {
                            return true;
                        }
                    }
                    *i = save_i;
                    *j = save_j;
                    if !match_here(p, i, n, j) {
                        return false;
                    }
                }
                c => {
                    if *j >= n.len() || n[*j] != c {
                        return false;
                    }
                    *i += 1;
                    *j += 1;
                }
            }
        }
        *j == n.len()
    }
    let pb = pattern.as_bytes();
    let nb = name.as_bytes();
    let mut i = 0;
    let mut j = 0;
    match_here(pb, &mut i, &nb, &mut j)
}

// ===== v0.4.0 神经层进化: 项目理解基建 =====

/// 默认忽略的目录名(gitignore + 常见垃圾目录集中管理)
const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cvsignore",
    "target",
    "node_modules",
    "vendor",
    "dist",
    "build",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".next",
    ".nuxt",
    ".turbo",
    ".cache",
    ".idea",
    ".vscode",
    "coverage",
    ".nyc_output",
    "Pods",
    ".gradle",
    ".terraform",
    ".rxt-cache", // rxt 自己的缓存目录
];

/// gitignore-aware 干净遍历 — 集中忽略逻辑, 所有代码理解命令复用
///
/// - 永远跳过 IGNORED_DIRS 列出的目录
/// - 永远跳过点开头文件(.git/.env 等)
/// - 解析当前目录的 .gitignore(轻量: 简单 glob, 不递归嵌套 .gitignore)
/// - exts: None=所有文件, Some=["rs","py"] 只返回这些扩展名
/// - max_depth: None=无限, Some(n)=最多递归 n 层
pub fn walk_clean(root: &Path, exts: Option<&[&str]>, max_depth: Option<usize>) -> Vec<PathBuf> {
    let mut results = Vec::new();
    // 收集 .gitignore 规则(根目录)
    let ignore_patterns = load_gitignore(root);
    walk_inner(root, exts, max_depth, 0, &ignore_patterns, &mut results);
    results
}

fn load_gitignore(dir: &Path) -> Vec<String> {
    let mut patterns = Vec::new();
    if let Ok(content) = std::fs::read_to_string(dir.join(".gitignore")) {
        for line in content.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            patterns.push(l.to_string());
        }
    }
    patterns
}

fn walk_inner(
    dir: &Path,
    exts: Option<&[&str]>,
    max_depth: Option<usize>,
    depth: usize,
    ignore_patterns: &[String],
    results: &mut Vec<PathBuf>,
) {
    if let Some(md) = max_depth {
        if depth > md {
            return;
        }
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // 跳过点开头文件/目录
        if name.starts_with('.') {
            continue;
        }
        // 跳过 IGNORED_DIRS
        if IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        // 跳过 .gitignore 命中的(简单匹配: 精确名或后缀)
        if ignore_patterns.iter().any(|p| matches_gitignore(p, &name)) {
            continue;
        }
        let p = entry.path();
        if p.is_dir() {
            walk_inner(&p, exts, max_depth, depth + 1, ignore_patterns, results);
        } else if p.is_file() {
            // 扩展名过滤
            if let Some(allowed) = exts {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !allowed.contains(&ext) {
                    continue;
                }
            }
            results.push(p);
        }
    }
}

fn matches_gitignore(pattern: &str, name: &str) -> bool {
    // 去 / 后缀(只取最后一段)
    let pat = pattern.rsplit('/').next().unwrap_or(pattern);
    let pat = pat.trim_end_matches('/');
    if pat.is_empty() {
        return false;
    }
    if pat == name {
        return true;
    }
    // *.ext 后缀匹配
    if let Some(suffix) = pat.strip_prefix("*.") {
        return name.ends_with(&format!(".{}", suffix));
    }
    // foo* 前缀匹配
    if let Some(prefix) = pat.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    // 用 glob_match 兜底
    glob_match(pat, name)
}

/// 项目类型嗅探 — 检测 Cargo.toml/package.json/go.mod/pyproject.toml
///
/// 返回项目种类、名称、版本、清单文件路径
pub struct ProjectKind {
    pub kind: String,
    pub name: String,
    pub version: String,
    pub manifest: PathBuf,
}

pub fn detect_kind(dir: &Path) -> Option<ProjectKind> {
    // Rust
    let cargo = dir.join("Cargo.toml");
    if cargo.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo) {
            let name = extract_toml_field(&content, "name").unwrap_or_else(|| "unknown".into());
            let version = extract_toml_field(&content, "version").unwrap_or_else(|| "0.0.0".into());
            return Some(ProjectKind {
                kind: "rust".into(),
                name,
                version,
                manifest: cargo,
            });
        }
    }
    // Node.js
    let pkg = dir.join("package.json");
    if pkg.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg) {
            let name = extract_json_field(&content, "name").unwrap_or_else(|| "unknown".into());
            let version = extract_json_field(&content, "version").unwrap_or_else(|| "0.0.0".into());
            return Some(ProjectKind {
                kind: "node".into(),
                name,
                version,
                manifest: pkg,
            });
        }
    }
    // Go
    let gomod = dir.join("go.mod");
    if gomod.exists() {
        if let Ok(content) = std::fs::read_to_string(&gomod) {
            let module = content
                .lines()
                .find_map(|l| {
                    l.trim()
                        .strip_prefix("module ")
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_else(|| "unknown".into());
            return Some(ProjectKind {
                kind: "go".into(),
                name: module,
                version: "0.0.0".into(), // go.mod 无版本
                manifest: gomod,
            });
        }
    }
    // Python
    let pyproject = dir.join("pyproject.toml");
    if pyproject.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
            let name = extract_toml_field(&content, "name").unwrap_or_else(|| "unknown".into());
            let version = extract_toml_field(&content, "version").unwrap_or_else(|| "0.0.0".into());
            return Some(ProjectKind {
                kind: "python".into(),
                name,
                version,
                manifest: pyproject,
            });
        }
    }
    let setup_py = dir.join("setup.py");
    if setup_py.exists() {
        return Some(ProjectKind {
            kind: "python".into(),
            name: "unknown".into(),
            version: "0.0.0".into(),
            manifest: setup_py,
        });
    }
    None
}

/// 从 TOML 文本提取 `name = "..."` 字段(简单行匹配, 不用 toml crate)
fn extract_toml_field(content: &str, field: &str) -> Option<String> {
    let needle = format!("{} =", field);
    for line in content.lines() {
        let l = line.trim();
        if l.starts_with(&needle) {
            // name = "rxt"
            let after = l[needle.len()..].trim();
            let val = after
                .trim_start_matches('"')
                .split('"')
                .next()
                .unwrap_or("");
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// 从 JSON 文本提取 "key": "value" (简单字符串扫描, 不依赖 serde 解析整个文件)
fn extract_json_field(content: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let idx = content.find(&needle)?;
    let after = &content[idx + needle.len()..];
    let colon = after.find(':')?;
    let rest = &after[colon + 1..];
    let quote = rest.find('"')?;
    let val_start = quote + 1;
    let val_rest = &rest[val_start..];
    let end = val_rest.find('"')?;
    Some(val_rest[..end].to_string())
}

/// token 估算 — chars/4 启发式(OpenAI 官方经验值, 零依赖)
pub fn approx_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    (chars + 3) / 4 // 向上取整, 避免短文本算成 0
}

/// token 预算截断 — 逐行累加 token, 超预算即停
///
/// 返回 (保留的行, 是否发生了截断)
pub fn truncate_to_budget(
    lines: &[(usize, String)],
    budget: usize,
) -> (Vec<(usize, String)>, bool) {
    if budget == 0 {
        return (lines.to_vec(), false);
    }
    let mut kept = Vec::new();
    let mut tokens = 0usize;
    for (line_no, text) in lines {
        let line_tokens = approx_tokens(text);
        if tokens + line_tokens > budget {
            // 这行加上就超了, 截断
            return (kept, true);
        }
        tokens += line_tokens;
        kept.push((*line_no, text.clone()));
    }
    (kept, false)
}
// ===== pub 桥接: 供 map/build_structure 复用忽略逻辑 =====

/// pub 版: 加载 .gitignore 规则
pub fn load_gitignore_pub(dir: &Path) -> Vec<String> {
    load_gitignore(dir)
}

/// pub 版: 判断目录名是否在默认忽略列表
pub fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

/// pub 版: 判断文件名是否匹配某条 gitignore 规则
pub fn matches_gitignore_pub(pattern: &str, name: &str) -> bool {
    matches_gitignore(pattern, name)
}

// ===== v0.9.1 内容读取帽道 =====
// 2026-08-22：rxt 0.9.0 grep 对 73GiB mkv 做 fs::read 全文。
// kernel: __vm_enough_memory bytes=78871310336（与 Chamber.of.Secrets UHD remux 只差 2844 字节），
// 随后 OOM killer 杀掉 RSS~19.5GiB 的 rxt。判二进制必须先看扩展名/大小/8KB 采样，禁止先整读。

/// 默认单文件内容读取上限。`RXT_MAX_TEXT_BYTES`（字节）或 `RXT_MAX_READ_MB`（MiB）可覆盖。
pub const DEFAULT_MAX_READ_BYTES: u64 = 32 * 1024 * 1024;

const BINARY_EXTS: &[&str] = &[
    "mkv", "mp4", "webm", "avi", "mov", "m4v", "ts", "m2ts", "flv", "wmv", "mp3", "flac", "wav",
    "aac", "ogg", "m4a", "wma", "opus", "jpg", "jpeg", "png", "gif", "webp", "bmp", "ico", "psd",
    "tiff", "heic", "zip", "7z", "rar", "gz", "bz2", "xz", "zst", "iso", "tar", "gguf", "bin",
    "exe", "dll", "so", "dylib", "wasm", "o", "a", "db", "sqlite", "sqlite3", "parquet", "woff",
    "woff2", "ttf", "otf", "eot", "pdf", "docx", "xlsx", "pptx",
];

pub fn max_text_read_bytes() -> u64 {
    if let Ok(s) = std::env::var("RXT_MAX_TEXT_BYTES") {
        if let Ok(n) = s.parse::<u64>() {
            if n > 0 {
                return n;
            }
        }
    }
    match std::env::var("RXT_MAX_READ_MB") {
        Ok(s) => s
            .parse::<u64>()
            .ok()
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .unwrap_or(DEFAULT_MAX_READ_BYTES),
        Err(_) => DEFAULT_MAX_READ_BYTES,
    }
}

pub fn max_text_bytes() -> u64 {
    max_text_read_bytes()
}

pub fn looks_binary_prefix(sample: &[u8]) -> bool {
    if sample.is_empty() {
        return false;
    }
    let n = sample.len();
    let nulls = sample.iter().filter(|&&b| b == 0).count();
    nulls * 20 > n
}

pub fn is_binary_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| BINARY_EXTS.iter().any(|b| e.eq_ignore_ascii_case(b)))
        .unwrap_or(false)
}

/// 扩展名或体积超限：连整文件 open+read 都不必。
pub fn should_skip_content(path: &Path) -> bool {
    if is_binary_ext(path) {
        return true;
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > max_text_read_bytes() => true,
        Err(_) => true,
        _ => false,
    }
}

pub fn skip_heavy_file(path: &Path) -> bool {
    should_skip_content(path)
}

/// 先看 metadata + 头 8KB，绝不先把整文件读进内存。
pub fn read_bytes_capped(path: &Path) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    if should_skip_content(path) {
        return None;
    }
    let max = max_text_read_bytes();
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len > max {
        return None;
    }
    let mut sample = [0u8; 8192];
    let n = f.read(&mut sample).ok()?;
    if looks_binary_prefix(&sample[..n]) {
        return None;
    }
    if (n as u64) >= len {
        return Some(sample[..n].to_vec());
    }
    f.seek(SeekFrom::Start(0)).ok()?;
    let mut buf = Vec::new();
    if buf.try_reserve((len as usize).min(max as usize)).is_err() {
        return None;
    }
    f.take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

pub fn read_text_bytes(path: &Path) -> Option<Vec<u8>> {
    read_bytes_capped(path)
}

pub fn read_utf8_lossy_capped(path: &Path) -> Option<String> {
    let raw = read_bytes_capped(path)?;
    Some(String::from_utf8_lossy(&raw).into_owned())
}

pub fn format_bytes(n: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", n as f64 / GIB)
    } else if n >= 1024 * 1024 {
        format!("{:.1} MiB", n as f64 / MIB)
    } else {
        format!("{} B", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn skip_mkv_ext() {
        assert!(is_binary_ext(Path::new("a.mkv")));
        assert!(is_binary_ext(Path::new("A.MP4")));
        assert!(is_binary_ext(Path::new("model.gguf")));
        assert!(!is_binary_ext(Path::new("a.rs")));
        assert!(!is_binary_ext(Path::new("notes.md")));
    }

    #[test]
    fn skip_oversize_sparse() {
        let dir = std::env::temp_dir().join(format!("rxt-cap-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("big.txt");
        {
            let f = std::fs::File::create(&p).unwrap();
            f.set_len(80 * 1024 * 1024 * 1024).unwrap();
        }
        assert!(should_skip_content(&p));
        assert!(read_bytes_capped(&p).is_none());
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn read_small_ok() {
        let dir = std::env::temp_dir().join(format!("rxt-cap-small-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("ok.rs");
        {
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(b"fn main() {}\n").unwrap();
        }
        let s = read_utf8_lossy_capped(&p).unwrap();
        assert!(s.contains("fn main"));
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_dir(&dir);
    }
}
