//! 插件注册 — Git 风格：未知子命令走 ~/.rxt/plugins 或 PATH 上的 rxt-<name>
//!
//! rxt plugin list
//! rxt plugin install <exe|dir> [--name foo] [--force]
//! rxt plugin remove <name>
//! rxt plugin which <name>

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BUILTINS: &[&str] = &[
    "replace",
    "read",
    "write",
    "cat",
    "jsonl",
    "patch",
    "stat",
    "find",
    "struct",
    "diff",
    "dep",
    "sed",
    "grep",
    "search",
    "py",
    "mem",
    "tree",
    "jq",
    "unzip",
    "ls",
    "http",
    "edit",
    "hash",
    "uuid",
    "enc",
    "dec",
    "watch",
    "tail",
    "time",
    "exec",
    "sort",
    "uniq",
    "cut",
    "count",
    "build",
    "check",
    "size",
    "clean",
    "normalize",
    "info",
    "git",
    "ctx",
    "map",
    "digest",
    "pack",
    "refs",
    "churn",
    "dead",
    "trace",
    "impact",
    "publish",
    "sysinfo",
    "ps",
    "service",
    "reg",
    "net",
    "upgrade",
    "deploy",
    "version",
    "sync",
    "serve",
    "snapshot",
    "qr",
    "clip",
    "repeat",
    "notify",
    "dup",
    "trash",
    "recipe",
    "bench",
    "watch-run",
    "evolve",
    "mcp",
    "plugin",
    "sign",
];

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Manifest {
    name: String,
    exe: String,
    #[serde(default)]
    force: bool,
}

pub fn plugins_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
        .join("plugins")
}

fn sanitize(name: &str) -> anyhow::Result<String> {
    let mut n = name.trim();
    if n.to_ascii_lowercase().starts_with("rxt-") {
        n = &n[4..];
    }
    if n.to_ascii_lowercase().ends_with(".exe") {
        n = &n[..n.len() - 4];
    }
    if n.is_empty()
        || !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("非法插件名: {name}");
    }
    Ok(n.to_ascii_lowercase())
}

fn plugin_dir(name: &str) -> PathBuf {
    plugins_dir().join(name)
}

fn load_manifest(dir: &Path) -> anyhow::Result<Manifest> {
    let p = dir.join("manifest.toml");
    let raw =
        fs::read_to_string(&p).map_err(|_| anyhow::anyhow!("无 manifest: {}", p.display()))?;
    Ok(toml::from_str(&raw)?)
}

fn exe_in_dir(dir: &Path, man: &Manifest) -> PathBuf {
    dir.join(&man.exe)
}

/// 已安装且 manifest.force=true 的插件（可覆盖内置名）
pub fn resolve_forced(name: &str) -> Option<PathBuf> {
    let Ok(n) = sanitize(name) else { return None };
    let dir = plugin_dir(&n);
    let man = load_manifest(&dir).ok()?;
    if !man.force {
        return None;
    }
    let exe = exe_in_dir(&dir, &man);
    exe.is_file().then_some(exe)
}

fn resolve_installed(name: &str) -> Option<PathBuf> {
    let Ok(n) = sanitize(name) else { return None };
    let dir = plugin_dir(&n);
    if let Ok(man) = load_manifest(&dir) {
        let exe = exe_in_dir(&dir, &man);
        if exe.is_file() {
            return Some(exe);
        }
    }
    None
}

fn resolve_path(name: &str) -> Option<PathBuf> {
    let Ok(n) = sanitize(name) else { return None };
    let cand = if cfg!(windows) {
        format!("rxt-{n}.exe")
    } else {
        format!("rxt-{n}")
    };
    which(&cand)
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(cmd);
        if p.is_file() {
            return Some(p);
        }
        #[cfg(windows)]
        {
            let p2 = dir.join(format!("{cmd}.exe"));
            if p2.is_file() {
                return Some(p2);
            }
        }
    }
    None
}

pub fn resolve(name: &str) -> Option<PathBuf> {
    resolve_installed(name).or_else(|| resolve_path(name))
}

pub fn is_builtin(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    BUILTINS.contains(&n.as_str())
}

fn spawn(
    exe: &Path,
    args: &[String],
    host: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<()> {
    let mut c = Command::new(exe);
    c.args(args);
    if let Some(h) = host {
        c.env("RXT_HOST", h);
    }
    if let Some(g) = group {
        c.env("RXT_GROUP", g);
    }
    let st = c.status()?;
    if st.success() {
        Ok(())
    } else {
        anyhow::bail!("插件 {} 退出 {}", exe.display(), st.code().unwrap_or(-1))
    }
}

/// 未知子命令：plugins 目录 → PATH rxt-<name>
pub fn run_external(
    args: &[String],
    host: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<()> {
    let name = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.len() > 1 { &args[1..] } else { &[] };
    if let Some(exe) = resolve(name) {
        return spawn(&exe, rest, host, group);
    }
    anyhow::bail!("未知命令 '{name}'。内置见 rxt --help；外挂: rxt plugin install <exe>")
}

/// clap 之前：--force 安装的插件覆盖同名内置
pub fn run_forced_override(
    name: &str,
    rest: &[String],
    host: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<bool> {
    if let Some(exe) = resolve_forced(name) {
        spawn(&exe, rest, host, group)?;
        return Ok(true);
    }
    Ok(false)
}

pub fn peek_flag(args: &[String], flag: &str) -> Option<String> {
    let eq = format!("{flag}=");
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag {
            return args.get(i + 1).cloned();
        }
        if let Some(v) = args[i].strip_prefix(&eq) {
            return Some(v.to_string());
        }
        i += 1;
    }
    None
}

pub fn strip_global_flags(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--host" || a == "--group" {
            i += 2;
            continue;
        }
        if a.starts_with("--host=") || a.starts_with("--group=") {
            i += 1;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

pub fn peek_subcommand(args: &[String]) -> Option<(String, Vec<String>)> {
    let mut i = 1usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--host" || a == "--group" {
            i += 2;
            continue;
        }
        if a.starts_with("--host=") || a.starts_with("--group=") {
            i += 1;
            continue;
        }
        if a == "--help" || a == "-h" || a == "--version" || a == "-V" || a == "--describe" {
            return None;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        // argv 被包一层时 args[1] 可能是 exe 路径，跳过
        if a.contains('\\') || a.contains('/') || a.ends_with(".exe") {
            i += 1;
            continue;
        }
        let name = a.clone();
        let rest = args[i + 1..].to_vec();
        return Some((name, rest));
    }
    None
}

pub fn run(
    action: &str,
    name: Option<&str>,
    path: Option<&Path>,
    force: bool,
    json: bool,
) -> anyhow::Result<()> {
    match action {
        "list" | "ls" => list(json),
        "install" | "add" => {
            let p = path.ok_or_else(|| anyhow::anyhow!("需要 exe 或目录路径"))?;
            install(p, name, force)
        }
        "remove" | "rm" | "uninstall" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            remove(n)
        }
        "which" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            which_cmd(n)
        }
        other => anyhow::bail!("未知 plugin 动作: {other}（list|install|remove|which）"),
    }
}

fn list(json: bool) -> anyhow::Result<()> {
    let mut installed: Vec<(String, String, bool)> = Vec::new();
    if let Ok(rd) = fs::read_dir(plugins_dir()) {
        for e in rd.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            if let Ok(man) = load_manifest(&dir) {
                let exe = exe_in_dir(&dir, &man);
                installed.push((man.name, exe.display().to_string(), man.force));
            }
        }
    }
    installed.sort_by(|a, b| a.0.cmp(&b.0));
    let path_hits = scan_path_plugins();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "builtins": BUILTINS,
                "installed": installed.iter().map(|(n, p, f)| serde_json::json!({
                    "name": n, "path": p, "force": f
                })).collect::<Vec<_>>(),
                "path": path_hits.iter().map(|(n, p)| serde_json::json!({
                    "name": n, "path": p
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }
    println!("# builtin {}", BUILTINS.len());
    println!("{}", BUILTINS.join(" "));
    println!("# installed {}", installed.len());
    if installed.is_empty() {
        println!("(无。安装: rxt plugin install <exe>)");
    } else {
        for (n, p, f) in &installed {
            println!("  {} {}\t{}", n, if *f { "--force" } else { "" }, p);
        }
    }
    println!("# path {}", path_hits.len());
    for (n, p) in &path_hits {
        println!("  {}\t{}", n, p);
    }
    Ok(())
}

fn scan_path_plugins() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some(path) = std::env::var_os("PATH") else {
        return out;
    };
    for dir in std::env::split_paths(&path) {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let lower = fname.to_ascii_lowercase();
            if !lower.starts_with("rxt-") {
                continue;
            }
            #[cfg(windows)]
            if !lower.ends_with(".exe") {
                continue;
            }
            if !p.is_file() {
                continue;
            }
            if let Ok(n) = sanitize(fname) {
                out.push((n, p.display().to_string()));
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn install(src: &Path, name: Option<&str>, force: bool) -> anyhow::Result<()> {
    if !src.exists() {
        anyhow::bail!("不存在: {}", src.display());
    }
    let (exe_src, stem) = if src.is_dir() {
        let man_path = src.join("manifest.toml");
        if man_path.is_file() {
            let man: Manifest = toml::from_str(&fs::read_to_string(&man_path)?)?;
            let exe = src.join(&man.exe);
            (exe, man.name)
        } else {
            let exe = find_exe_in_dir(src)?;
            let stem = exe
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("plugin")
                .to_string();
            (exe, stem)
        }
    } else {
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();
        (src.to_path_buf(), stem)
    };
    #[cfg(windows)]
    if !exe_src
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
    {
        anyhow::bail!("Windows 插件必须是 .exe: {}", exe_src.display());
    }
    let mut n = sanitize(name.unwrap_or(&stem))?;
    if n.starts_with("rxt-") {
        n = n.trim_start_matches("rxt-").to_string();
    }
    if is_builtin(&n) && !force {
        anyhow::bail!("'{n}' 是内置命令。覆盖请加 --force");
    }
    let dir = plugin_dir(&n);
    fs::create_dir_all(plugins_dir())?;
    let staging = plugins_dir().join(format!(".{n}.install-{}", std::process::id()));
    let backup = plugins_dir().join(format!(".{n}.backup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&backup);
    fs::create_dir(&staging)?;
    let exe_name = if cfg!(windows) {
        format!("rxt-{n}.exe")
    } else {
        format!("rxt-{n}")
    };
    let dest = staging.join(&exe_name);
    fs::copy(&exe_src, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(&dest)?.permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&dest, perm)?;
    }
    let man = Manifest {
        name: n.clone(),
        exe: exe_name.clone(),
        force,
    };
    fs::write(
        staging.join("manifest.toml"),
        format!(
            "name = \"{}\"\nexe = \"{}\"\nforce = {}\n",
            man.name.replace('"', ""),
            man.exe.replace('"', ""),
            man.force
        ),
    )?;
    #[cfg(windows)]
    {
        if let Err(e) = crate::sign::sign_path(&dest, false) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    }

    if dir.exists() {
        fs::rename(&dir, &backup)?;
    }
    if let Err(e) = fs::rename(&staging, &dir) {
        if backup.exists() {
            fs::rename(&backup, &dir).map_err(|restore| {
                anyhow::anyhow!("安装插件失败（{e}），恢复旧版本也失败: {restore}")
            })?;
            anyhow::bail!("安装插件失败，已恢复旧版本: {e}");
        }
        anyhow::bail!("安装插件失败: {e}");
    }
    let _ = fs::remove_dir_all(&backup);
    eprintln!("# 已安装 {} -> {}", n, dir.join(exe_name).display());
    Ok(())
}

fn find_exe_in_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    let mut found = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("rxt-")
                && (p
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
                    || cfg!(not(windows)))
            {
                found.push(p);
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("目录里没有 rxt-* 可执行文件"))
}

fn remove(name: &str) -> anyhow::Result<()> {
    let n = sanitize(name)?;
    let dir = plugin_dir(&n);
    if !dir.exists() {
        anyhow::bail!("未安装: {n}");
    }
    fs::remove_dir_all(&dir)?;
    eprintln!("# 已移除 {n}");
    Ok(())
}

fn which_cmd(name: &str) -> anyhow::Result<()> {
    if let Some(p) = resolve_forced(name) {
        println!("{} (force)", p.display());
        return Ok(());
    }
    if is_builtin(name) {
        println!("builtin");
        return Ok(());
    }
    if let Some(p) = resolve(name) {
        println!("{}", p.display());
        return Ok(());
    }
    anyhow::bail!("找不到: {name}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_prefix() {
        assert_eq!(sanitize("rxt-foo.exe").unwrap(), "foo");
        assert_eq!(sanitize("Bar_1").unwrap(), "bar_1");
        assert!(sanitize("../x").is_err());
        assert!(sanitize("").is_err());
    }

    #[test]
    fn peek_skips_global_flags() {
        let args = vec![
            "rxt".into(),
            "--host".into(),
            "huhu".into(),
            "foo".into(),
            "--bar".into(),
        ];
        let (name, rest) = peek_subcommand(&args).unwrap();
        assert_eq!(name, "foo");
        assert_eq!(rest, vec!["--bar"]);
        assert_eq!(peek_flag(&args, "--host").as_deref(), Some("huhu"));
    }

    #[test]
    fn strips_globals_after_external_command() {
        let args = vec![
            "hello".into(),
            "one".into(),
            "--host".into(),
            "huhu".into(),
            "--group=core".into(),
            "--two".into(),
        ];
        assert_eq!(strip_global_flags(&args), vec!["hello", "one", "--two"]);
    }

    #[test]
    fn peek_skips_exe_path() {
        let args = vec![
            r"C:\rxt\rxt.exe".into(),
            r"C:\rxt\rxt.exe".into(),
            "foo".into(),
        ];
        let (name, _) = peek_subcommand(&args).unwrap();
        assert_eq!(name, "foo");
    }

    #[test]
    fn clap_captures_external_subcommand() {
        let parsed = crate::parse_cli(vec![
            "rxt".into(),
            "--host".into(),
            "huhu".into(),
            "hello".into(),
            "--flag".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.host.as_deref(), Some("huhu"));
        match parsed.command {
            crate::Command::External(args) => {
                assert_eq!(args, vec!["hello", "--flag"]);
            }
            _ => panic!("未知命令没有进入 External"),
        }
    }

    #[test]
    fn builtins_include_plugin_and_sign() {
        assert!(is_builtin("http"));
        assert!(is_builtin("plugin"));
        assert!(is_builtin("sign"));
        assert!(!is_builtin("not-a-cmd"));
    }
}
