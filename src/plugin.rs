//! 插件注册 — Git 风格：未知子命令走 ~/.rxt/plugins 或 PATH 上的 rxt-<name>
//!
//! rxt plugin list
//! rxt plugin new <name> [--lang sh|py|cmd|ps1] [--body '...'] [--stdin] [--open] [--force]
//! rxt plugin add <name|path>     # 路径存在则安装，否则创建
//! rxt plugin install <exe|dir> [--name foo] [--force]
//! rxt plugin show|edit|which|remove <name>

use std::cell::RefCell;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 留在 rxt 核心里的命令。其余全部是可装卸插件。
const BUILTINS: &[&str] = &[
    "plugin", "exec", "info", "version", "upgrade", "deploy", "publish", "sign",
];

/// 官方标准库（一份 rxt-tools 多路调用，按名字单独 seed/remove）。
pub const STDLIB: &[&str] = &[
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
    "sort",
    "uniq",
    "cut",
    "count",
    "build",
    "check",
    "size",
    "clean",
    "normalize",
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
    "sysinfo",
    "ps",
    "service",
    "reg",
    "net",
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
];

const USAGE: &str = "\
未知 plugin 动作: {other}
用法:
  rxt plugin list [--json]
  rxt plugin seed [name]             # 安装官方标准库（全部或一个）
  rxt plugin new <name> [--lang sh|py|cmd|ps1] [--body '...'] [--stdin] [--open] [--force]
  rxt plugin add <name|path> [--name] [--lang] [--body] [--stdin] [--force]
  rxt plugin install <exe|dir> [--name] [--force]
  rxt plugin show <name> [--json]
  rxt plugin edit <name>
  rxt plugin which <name>
  rxt plugin remove <name>";

thread_local! {
    static PLUGINS_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lang {
    Sh,
    Py,
    Cmd,
    Ps1,
}

impl Lang {
    fn as_str(self) -> &'static str {
        match self {
            Lang::Sh => "sh",
            Lang::Py => "py",
            Lang::Cmd => "cmd",
            Lang::Ps1 => "ps1",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Manifest {
    name: String,
    exe: String,
    #[serde(default)]
    force: bool,
}

pub struct PluginCli<'a> {
    pub action: &'a str,
    pub target: Option<&'a str>,
    pub name: Option<&'a str>,
    pub content: Option<&'a str>,
    pub body: Option<&'a str>,
    pub force: bool,
    pub json: bool,
    pub lang: Option<&'a str>,
    pub stdin: bool,
    pub open: bool,
}

pub fn plugins_dir() -> PathBuf {
    if let Some(p) = PLUGINS_DIR_OVERRIDE.with(|s| s.borrow().clone()) {
        return p;
    }
    if let Ok(p) = std::env::var("RXT_PLUGINS_DIR") {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("RXT_HOME") {
        return PathBuf::from(p).join("plugins");
    }
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

pub fn is_stdlib(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    STDLIB.contains(&n.as_str())
}

fn spawn(
    exe: &Path,
    args: &[String],
    host: Option<&str>,
    group: Option<&str>,
    plugin_name: Option<&str>,
) -> anyhow::Result<()> {
    let mut c = Command::new(exe);
    c.args(args);
    if let Some(h) = host {
        c.env("RXT_HOST", h);
    }
    if let Some(g) = group {
        c.env("RXT_GROUP", g);
    }
    if let Some(n) = plugin_name {
        c.env("RXT_PLUGIN_NAME", n);
    }
    let st = c.status()?;
    if st.success() {
        Ok(())
    } else {
        anyhow::bail!("插件 {} 退出 {}", exe.display(), st.code().unwrap_or(-1))
    }
}

/// 未知子命令：--host 则远端跑 `rxt <cmd>`；否则 plugins → PATH rxt-<name> → recipe
pub fn run_external(
    args: &[String],
    host: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<()> {
    let name = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.len() > 1 { &args[1..] } else { &[] };
    if name.is_empty() {
        anyhow::bail!("缺少子命令。rxt --help");
    }
    if let Some(g) = group {
        let hosts = crate::hosts::HostsFile::load()?;
        for member in hosts.get_group_members(g)? {
            eprintln!("\n=== [{}] ===", member);
            run_external(args, Some(&member), None)?;
        }
        return Ok(());
    }
    if let Some(h) = host {
        return forward_remote(h, args);
    }
    if let Some(exe) = resolve(name) {
        return spawn(&exe, rest, None, None, Some(name));
    }
    if crate::recipe::try_run_as_command(name, rest)? {
        return Ok(());
    }
    let hint = if is_stdlib(name) {
        format!("  标准库: rxt plugin seed {name}   （或 rxt plugin seed 装全套）")
    } else {
        format!("  新建插件: rxt plugin new {name}")
    };
    anyhow::bail!(
        "未知命令 '{name}'。\n  内置:     rxt --help\n{hint}\n  一行宏:   rxt recipe add {name} \"命令\"\n  装现成:   rxt plugin install <exe|dir>"
    )
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn forward_remote(host: &str, args: &[String]) -> anyhow::Result<()> {
    let rc = crate::remote::RemoteChannel::connect(host)?;
    let q: Vec<String> = std::iter::once("rxt".to_string())
        .chain(args.iter().cloned())
        .map(|a| sh_quote(&a))
        .collect();
    rc.exec_forward(&q.join(" "))
}

/// clap 之前：--force 安装的插件覆盖同名内置
pub fn run_forced_override(
    name: &str,
    rest: &[String],
    host: Option<&str>,
    group: Option<&str>,
) -> anyhow::Result<bool> {
    if let Some(exe) = resolve_forced(name) {
        spawn(&exe, rest, host, group, Some(name))?;
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

pub fn run(cli: PluginCli<'_>) -> anyhow::Result<()> {
    match cli.action {
        "list" | "ls" => list(cli.json),
        "install" => {
            let t = cli
                .target
                .ok_or_else(|| anyhow::anyhow!("需要 exe 或目录路径"))?;
            if t == "-" {
                create_from_cli(&cli)
            } else {
                install(Path::new(t), cli.name, cli.force, cli.lang)
            }
        }
        "add" => add_smart(&cli),
        "new" | "create" | "init" => create_from_cli(&cli),
        "seed" => {
            if let Some(n) = cli.target.or(cli.name) {
                seed_one(n, cli.force)?;
                eprintln!("# 已安装标准库插件 {n}");
            } else {
                let n = seed_all(cli.force)?;
                eprintln!("# 已 seed {n} 个标准库插件 → rxt plugin list");
            }
            Ok(())
        }
        "remove" | "rm" | "uninstall" => {
            let n = cli
                .target
                .or(cli.name)
                .ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            remove(n)
        }
        "which" | "path" => {
            let n = cli
                .target
                .or(cli.name)
                .ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            which_cmd(n, cli.json)
        }
        "show" | "cat" => {
            let n = cli
                .target
                .or(cli.name)
                .ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            show(n, cli.json)
        }
        "edit" => {
            let n = cli
                .target
                .or(cli.name)
                .ok_or_else(|| anyhow::anyhow!("需要插件名"))?;
            edit(n)
        }
        other => anyhow::bail!("{}", USAGE.replace("{other}", other)),
    }
}

fn add_smart(cli: &PluginCli<'_>) -> anyhow::Result<()> {
    match cli.target {
        Some("-") => create_from_cli(cli),
        Some(t) => {
            let p = Path::new(t);
            if p.exists() {
                install(p, cli.name, cli.force, cli.lang)
            } else if looks_like_path(t) {
                anyhow::bail!("不存在: {t}")
            } else if is_stdlib(t) {
                seed_one(t, cli.force)?;
                eprintln!("# 已安装标准库插件 {t}。卸载: rxt plugin remove {t}");
                Ok(())
            } else {
                create_from_cli(cli)
            }
        }
        None => {
            if cli.name.is_some() || cli.stdin {
                create_from_cli(cli)
            } else {
                anyhow::bail!(
                    "需要插件名或路径。例:\n  rxt plugin add hello --body 'echo hi'\n  rxt plugin add ./rxt-hello"
                )
            }
        }
    }
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/')
        || s.contains('\\')
        || s.ends_with(".exe")
        || s.ends_with(".cmd")
        || s.ends_with(".bat")
        || s.ends_with(".py")
        || s.ends_with(".sh")
        || s.ends_with(".ps1")
        || s.starts_with('.')
}

fn create_from_cli(cli: &PluginCli<'_>) -> anyhow::Result<()> {
    let raw_name = cli
        .name
        .or(cli.target.filter(|t| *t != "-"))
        .ok_or_else(|| anyhow::anyhow!("需要插件名。例: rxt plugin new hello"))?;
    if cli.stdin && cli.body.is_some() && cli.body != Some("-") {
        anyhow::bail!("不要同时用 --stdin 和 --body");
    }
    let body = read_body(cli)?;
    let lang = resolve_lang(cli.lang, body.as_deref())?;
    create_plugin(
        raw_name,
        lang,
        body.as_deref(),
        cli.force,
        cli.json,
        cli.open,
    )
}

fn read_body(cli: &PluginCli<'_>) -> anyhow::Result<Option<String>> {
    let from_flag = cli.body.or(cli.content);
    if cli.stdin || from_flag == Some("-") || cli.target == Some("-") {
        return Ok(Some(read_stdin()?));
    }
    Ok(from_flag.map(|s| s.to_string()))
}

fn read_stdin() -> anyhow::Result<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    if s.trim().is_empty() {
        anyhow::bail!("stdin 为空。用法: rxt plugin new hello --stdin  < script.sh");
    }
    Ok(s)
}

fn parse_lang(s: &str) -> anyhow::Result<Lang> {
    match s.trim().to_ascii_lowercase().as_str() {
        "sh" | "bash" | "shell" | "zsh" => Ok(Lang::Sh),
        "py" | "python" | "python3" => Ok(Lang::Py),
        "cmd" | "bat" | "batch" => Ok(Lang::Cmd),
        "ps1" | "ps" | "powershell" | "pwsh" => Ok(Lang::Ps1),
        other => anyhow::bail!("未知 --lang {other}（sh|py|cmd|ps1）"),
    }
}

fn default_lang() -> Lang {
    if cfg!(windows) {
        let git_bash = std::env::var("MSYSTEM").is_ok()
            || std::env::var("SHELL").unwrap_or_default().contains("bash");
        if git_bash {
            Lang::Sh
        } else {
            Lang::Cmd
        }
    } else {
        Lang::Sh
    }
}

fn resolve_lang(explicit: Option<&str>, body: Option<&str>) -> anyhow::Result<Lang> {
    if let Some(s) = explicit {
        return parse_lang(s);
    }
    if let Some(b) = body {
        let first = b.lines().next().unwrap_or("").trim();
        let l = first.to_ascii_lowercase();
        if l.contains("python") && (l.starts_with("#!") || l.starts_with("rem")) {
            return Ok(Lang::Py);
        }
        if l.starts_with("#!") && (l.contains("pwsh") || l.contains("powershell")) {
            return Ok(Lang::Ps1);
        }
        if l.starts_with("#!") {
            return Ok(Lang::Sh);
        }
        if l.starts_with("@echo") || l.starts_with("rem ") {
            return Ok(Lang::Cmd);
        }
    }
    Ok(default_lang())
}

fn with_shebang(body: &str, lang: Lang) -> String {
    let body = body.replace("\r\n", "\n");
    let trimmed = body.trim_end();
    if trimmed.starts_with("#!") || lang == Lang::Cmd {
        return format!("{trimmed}\n");
    }
    let sb = match lang {
        Lang::Sh => "#!/usr/bin/env bash",
        Lang::Py => "#!/usr/bin/env python3",
        Lang::Ps1 => {
            if cfg!(unix) {
                "#!/usr/bin/env pwsh"
            } else {
                ""
            }
        }
        Lang::Cmd => "",
    };
    if sb.is_empty() {
        format!("{trimmed}\n")
    } else {
        format!("{sb}\n{trimmed}\n")
    }
}

fn source_name(name: &str, lang: Lang) -> String {
    match lang {
        Lang::Sh => format!("rxt-{name}.sh"),
        Lang::Py => format!("rxt-{name}.py"),
        Lang::Cmd => format!("rxt-{name}.cmd"),
        Lang::Ps1 => format!("rxt-{name}.ps1"),
    }
}

fn stub(name: &str, lang: Lang) -> String {
    match lang {
        Lang::Sh => r#"#!/usr/bin/env bash
# rxt {name} — argv 不含子命令名；远程读 $RXT_HOST / $RXT_GROUP
set -euo pipefail
echo "{name} argv=$*"
if [[ -n "${RXT_HOST:-}" ]]; then
  echo "RXT_HOST=$RXT_HOST"
fi
"#
        .replace("{name}", name),
        Lang::Py => r#"#!/usr/bin/env python3
# rxt {name} — argv 不含子命令名；远程读 RXT_HOST / RXT_GROUP
import os, sys
print("{name} argv", sys.argv[1:])
print("RXT_HOST", os.environ.get("RXT_HOST", ""))
print("RXT_GROUP", os.environ.get("RXT_GROUP", ""))
"#
        .replace("{name}", name),
        Lang::Cmd => r#"@echo off
rem rxt {name} — argv 不含子命令名；远程读 %RXT_HOST% / %RXT_GROUP%
echo {name} argv=%*
if not "%RXT_HOST%"=="" echo RXT_HOST=%RXT_HOST%
"#
        .replace("{name}", name),
        Lang::Ps1 => r#"# rxt {name} — argv 不含子命令名；远程读 $env:RXT_HOST / $env:RXT_GROUP
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$PluginArgs)
Write-Output "{name} argv=$($PluginArgs -join ' ')"
if ($env:RXT_HOST) { Write-Output "RXT_HOST=$env:RXT_HOST" }
"#
        .replace("{name}", name),
    }
}

#[allow(dead_code)] // Windows 启动器；Linux 仅 --lang cmd 的 ensure 路径用到
fn win_cmd_wrapper(name: &str, lang: Lang) -> String {
    match lang {
        Lang::Sh => format!(
            r#"@echo off
setlocal EnableExtensions
set "SCRIPT=%~dp0rxt-{name}.sh"
set "BASH="
where bash >nul 2>&1 && set "BASH=bash"
if not defined BASH if exist "%ProgramFiles%\Git\bin\bash.exe" set "BASH=%ProgramFiles%\Git\bin\bash.exe"
if not defined BASH if exist "%ProgramFiles(x86)%\Git\bin\bash.exe" set "BASH=%ProgramFiles(x86)%\Git\bin\bash.exe"
if not defined BASH if exist "%LOCALAPPDATA%\Programs\Git\bin\bash.exe" set "BASH=%LOCALAPPDATA%\Programs\Git\bin\bash.exe"
if not defined BASH (
  echo rxt-{name}: 找不到 bash。请安装 Git Bash 或把 bash 加入 PATH. 1>&2
  exit /b 1
)
"%BASH%" "%SCRIPT%" %*
exit /b %ERRORLEVEL%
"#
        ),
        Lang::Py => format!(
            r#"@echo off
setlocal EnableExtensions
set "SCRIPT=%~dp0rxt-{name}.py"
where py >nul 2>&1
if not errorlevel 1 (
  py -3 "%SCRIPT%" %*
  exit /b %ERRORLEVEL%
)
where python >nul 2>&1
if not errorlevel 1 (
  python "%SCRIPT%" %*
  exit /b %ERRORLEVEL%
)
where python3 >nul 2>&1
if not errorlevel 1 (
  python3 "%SCRIPT%" %*
  exit /b %ERRORLEVEL%
)
echo rxt-{name}: 找不到 python 1>&2
exit /b 1
"#
        ),
        Lang::Ps1 => format!(
            r#"@echo off
setlocal EnableExtensions
set "SCRIPT=%~dp0rxt-{name}.ps1"
where pwsh >nul 2>&1
if not errorlevel 1 (
  pwsh -NoProfile -File "%SCRIPT%" %*
  exit /b %ERRORLEVEL%
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT%" %*
exit /b %ERRORLEVEL%
"#
        ),
        Lang::Cmd => String::new(),
    }
}

#[allow(dead_code)] // Linux 上跑 Windows .cmd 插件时的占位启动器
fn unix_cmd_helper(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
if command -v cmd.exe >/dev/null 2>&1; then
  exec cmd.exe /c "$DIR/rxt-{name}.cmd" "$@"
fi
echo "rxt-{name}: 这是 Windows .cmd 插件，当前系统不能直接执行。源文件: $DIR/rxt-{name}.cmd" >&2
exit 1
"#
    )
}

#[cfg(unix)]
fn chmod_755(p: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(p)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(p, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_755(_p: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn write_manifest(dir: &Path, name: &str, exe: &str, force: bool) -> anyhow::Result<()> {
    fs::write(
        dir.join("manifest.toml"),
        format!(
            "name = \"{}\"\nexe = \"{}\"\nforce = {}\n",
            name.replace('"', ""),
            exe.replace('"', ""),
            force
        ),
    )?;
    Ok(())
}

fn write_plugin_files(
    dir: &Path,
    name: &str,
    lang: Lang,
    body: &str,
) -> anyhow::Result<(String, String)> {
    let src_name = source_name(name, lang);
    let src_path = dir.join(&src_name);
    fs::write(&src_path, with_shebang(body, lang))?;
    chmod_755(&src_path)?;

    let exe_name = if cfg!(windows) {
        match lang {
            Lang::Cmd => src_name.clone(),
            _ => {
                let wrap = format!("rxt-{name}.cmd");
                fs::write(dir.join(&wrap), win_cmd_wrapper(name, lang))?;
                wrap
            }
        }
    } else {
        match lang {
            Lang::Cmd => {
                let wrap = format!("rxt-{name}");
                fs::write(dir.join(&wrap), unix_cmd_helper(name))?;
                chmod_755(&dir.join(&wrap))?;
                wrap
            }
            _ => src_name.clone(),
        }
    };
    Ok((exe_name, src_name))
}

fn make_staging(name: &str) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(plugins_dir())?;
    let staging = plugins_dir().join(format!(".{name}.install-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    Ok(staging)
}

fn atomic_swap(name: &str, staging: &Path) -> anyhow::Result<PathBuf> {
    let dir = plugin_dir(name);
    let backup = plugins_dir().join(format!(".{name}.backup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&backup);
    if dir.exists() {
        fs::rename(&dir, &backup)?;
    }
    if let Err(e) = fs::rename(staging, &dir) {
        let _ = fs::remove_dir_all(staging);
        if backup.exists() {
            fs::rename(&backup, &dir).map_err(|restore| {
                anyhow::anyhow!("安装插件失败（{e}），恢复旧版本也失败: {restore}")
            })?;
            anyhow::bail!("安装插件失败，已恢复旧版本: {e}");
        }
        anyhow::bail!("安装插件失败: {e}");
    }
    let _ = fs::remove_dir_all(&backup);
    Ok(dir)
}

fn prepare_name(raw: &str, force: bool) -> anyhow::Result<String> {
    let n = sanitize(raw)?;
    if is_builtin(&n) && !force {
        anyhow::bail!("'{n}' 是内置命令。覆盖请加 --force");
    }
    Ok(n)
}

fn create_plugin(
    raw_name: &str,
    lang: Lang,
    body: Option<&str>,
    force: bool,
    json: bool,
    open: bool,
) -> anyhow::Result<()> {
    let n = prepare_name(raw_name, force)?;
    let dir = plugin_dir(&n);
    if dir.exists() && !force {
        anyhow::bail!("已安装 '{n}'。覆盖加 --force，或 rxt plugin edit {n}");
    }
    let script = body
        .map(|s| s.to_string())
        .unwrap_or_else(|| stub(&n, lang));
    let staging = make_staging(&n)?;
    let (exe_name, src_name) = match write_plugin_files(&staging, &n, lang, &script) {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let man_force = is_builtin(&n) && force;
    if let Err(e) = write_manifest(&staging, &n, &exe_name, man_force) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let dir = atomic_swap(&n, &staging)?;
    let exe = dir.join(&exe_name);
    let source = dir.join(&src_name);
    let recipe_hit = crate::recipe::resolve_path(&n);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "name": n,
                "lang": lang.as_str(),
                "dir": dir.display().to_string(),
                "exe": exe.display().to_string(),
                "source": source.display().to_string(),
                "force": man_force,
                "run": format!("rxt {n}"),
                "recipe_shadowed": recipe_hit.as_ref().map(|p| p.display().to_string()),
            }))?
        );
    } else {
        eprintln!("# 已创建 {} ({}) -> {}", n, lang.as_str(), source.display());
        eprintln!("# 运行: rxt {n}");
        eprintln!("# 编辑: rxt plugin edit {n}");
        if let Some(p) = &recipe_hit {
            eprintln!(
                "# 注意: 已有 recipe '{}'（{}）。rxt {n} 优先走插件。",
                n,
                p.display()
            );
        }
    }
    if open {
        open_editor(&source)?;
    }
    Ok(())
}

fn source_file(dir: &Path, man: &Manifest) -> PathBuf {
    let n = &man.name;
    for cand in [
        dir.join(format!("rxt-{n}.py")),
        dir.join(format!("rxt-{n}.sh")),
        dir.join(format!("rxt-{n}.ps1")),
        dir.join(format!("{n}.py")),
        dir.join(format!("{n}.sh")),
        dir.join(format!("{n}.ps1")),
    ] {
        if cand.is_file() {
            return cand;
        }
    }
    dir.join(&man.exe)
}

fn editor_cmd() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".into()
            } else {
                "nano".into()
            }
        })
}

fn open_editor(path: &Path) -> anyhow::Result<()> {
    let editor = editor_cmd();
    let status = Command::new(&editor).arg(path).status()?;
    if !status.success() {
        anyhow::bail!("编辑器退出码 {}", status.code().unwrap_or(-1));
    }
    eprintln!("# 已编辑 {}", path.display());
    Ok(())
}

fn list_plugin_files(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            names.push(n);
        }
    }
    names.sort();
    names
}

fn show(name: &str, json: bool) -> anyhow::Result<()> {
    if let Ok(n) = sanitize(name) {
        let dir = plugin_dir(&n);
        if let Ok(man) = load_manifest(&dir) {
            let source = source_file(&dir, &man);
            let body = fs::read_to_string(&source).unwrap_or_default();
            let files = list_plugin_files(&dir);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "kind": "plugin",
                        "name": man.name,
                        "dir": dir.display().to_string(),
                        "exe": exe_in_dir(&dir, &man).display().to_string(),
                        "source": source.display().to_string(),
                        "force": man.force,
                        "files": files,
                        "body": body,
                    }))?
                );
            } else {
                println!("# plugin {}", man.name);
                println!("# dir: {}", dir.display());
                println!("# exe: {}", man.exe);
                println!("# force: {}", man.force);
                println!("# source: {}", source.display());
                println!("# files: {}", files.join(", "));
                println!(
                    "# --- {} ---",
                    source.file_name().unwrap_or_default().to_string_lossy()
                );
                print!("{body}");
                if !body.ends_with('\n') {
                    println!();
                }
            }
            return Ok(());
        }
    }
    if let Some(p) = crate::recipe::resolve_path(name) {
        let body = fs::read_to_string(&p).unwrap_or_default();
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "kind": "recipe",
                    "name": name,
                    "path": p.display().to_string(),
                    "body": body,
                }))?
            );
        } else {
            println!("# recipe: {}\n{}", name, body);
        }
        return Ok(());
    }
    anyhow::bail!("找不到: {name}。rxt plugin new {name}  或  rxt recipe add {name} \"命令\"")
}

fn edit(name: &str) -> anyhow::Result<()> {
    if let Ok(n) = sanitize(name) {
        let dir = plugin_dir(&n);
        if let Ok(man) = load_manifest(&dir) {
            let source = source_file(&dir, &man);
            return open_editor(&source);
        }
    }
    if let Some(p) = crate::recipe::resolve_path(name) {
        return open_editor(&p);
    }
    anyhow::bail!("找不到: {name}。先 rxt plugin new {name}")
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
    let recipes = crate::recipe::list_entries();
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
                "recipes": recipes.iter().map(|(n, p)| serde_json::json!({
                    "name": n, "path": p.display().to_string()
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }
    println!("# core {}", BUILTINS.len());
    println!("{}", BUILTINS.join(" "));
    println!(
        "# stdlib {}  (rxt plugin seed / remove 可单独装卸)",
        STDLIB.len()
    );
    println!("# installed {}", installed.len());
    if installed.is_empty() {
        println!("(无。创建: rxt plugin new <name>    安装: rxt plugin install <exe|dir>)");
    } else {
        for (n, p, f) in &installed {
            println!("  {} {}\t{}", n, if *f { "--force" } else { "" }, p);
        }
    }
    println!("# path {}", path_hits.len());
    for (n, p) in &path_hits {
        println!("  {}\t{}", n, p);
    }
    println!("# recipes {}  (也可 rxt <name>)", recipes.len());
    if recipes.is_empty() {
        println!("(无。一行宏: rxt recipe add <name> \"命令\")");
    } else {
        for (n, p) in &recipes {
            println!("  {}\t{}", n, p.display());
        }
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
            if !lower.ends_with(".exe") && !lower.ends_with(".cmd") && !lower.ends_with(".bat") {
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

fn skip_copy_name(n: &str) -> bool {
    matches!(
        n,
        ".git" | "node_modules" | "target" | "__pycache__" | ".svn" | ".hg"
    )
}

fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let namestr = name.to_string_lossy();
        if skip_copy_name(&namestr) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

enum SrcKind {
    NativeExe,
    Script(Lang),
}

fn classify_src(path: &Path, lang_override: Option<&str>) -> anyhow::Result<SrcKind> {
    if let Some(s) = lang_override {
        return Ok(SrcKind::Script(parse_lang(s)?));
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(match ext.as_str() {
        "exe" => SrcKind::NativeExe,
        "py" => SrcKind::Script(Lang::Py),
        "sh" | "bash" => SrcKind::Script(Lang::Sh),
        "cmd" | "bat" => SrcKind::Script(Lang::Cmd),
        "ps1" => SrcKind::Script(Lang::Ps1),
        _ => {
            if let Ok(head) = fs::read_to_string(path) {
                let first = head.lines().next().unwrap_or("");
                let l = first.to_ascii_lowercase();
                if l.starts_with("#!") && l.contains("python") {
                    return Ok(SrcKind::Script(Lang::Py));
                }
                if l.starts_with("#!") && (l.contains("pwsh") || l.contains("powershell")) {
                    return Ok(SrcKind::Script(Lang::Ps1));
                }
                if l.starts_with("#!") {
                    return Ok(SrcKind::Script(Lang::Sh));
                }
                if l.starts_with("@echo") || l.starts_with("rem ") {
                    return Ok(SrcKind::Script(Lang::Cmd));
                }
            }
            if cfg!(windows) {
                SrcKind::NativeExe
            } else {
                SrcKind::NativeExe
            }
        }
    })
}

fn is_windows_exe(p: &Path) -> bool {
    p.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
}

fn find_exe_in_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    let mut found = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let lower = name.to_ascii_lowercase();
            if lower == "manifest.toml" {
                continue;
            }
            if lower.starts_with("rxt-") {
                found.push(p);
            }
        }
    }
    found.sort_by(|a, b| prefer_launcher(a).cmp(&prefer_launcher(b)).then(a.cmp(b)));
    found
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("目录里没有 rxt-* 可执行文件"))
}

fn prefer_launcher(p: &Path) -> u8 {
    let n = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if n.ends_with(".exe") {
        0
    } else if n.ends_with(".cmd") || n.ends_with(".bat") {
        1
    } else if !n.contains('.') {
        2
    } else if n.ends_with(".sh") {
        3
    } else if n.ends_with(".py") {
        4
    } else if n.ends_with(".ps1") {
        5
    } else {
        9
    }
}

#[cfg(windows)]
fn sign_exes(dir: &Path) -> anyhow::Result<()> {
    fn walk(dir: &Path) -> anyhow::Result<()> {
        for e in fs::read_dir(dir)? {
            let p = e?.path();
            if p.is_dir() {
                walk(&p)?;
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
            {
                crate::sign::sign_path(&p, false)?;
            }
        }
        Ok(())
    }
    walk(dir)
}

fn ensure_windows_launcher(dir: &Path, man: &mut Manifest) -> anyhow::Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    let exe = dir.join(&man.exe);
    if !exe.is_file() {
        return Ok(());
    }
    if is_windows_exe(&exe) {
        return Ok(());
    }
    let lower = man.exe.to_ascii_lowercase();
    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        return Ok(());
    }
    let wrap = format!("rxt-{}.cmd", man.name);
    if dir.join(&wrap).is_file() {
        man.exe = wrap;
        return Ok(());
    }
    let lang = if lower.ends_with(".py") {
        Lang::Py
    } else if lower.ends_with(".ps1") {
        Lang::Ps1
    } else {
        Lang::Sh
    };
    fs::write(dir.join(&wrap), win_cmd_wrapper(&man.name, lang))?;
    man.exe = wrap;
    Ok(())
}

fn install(src: &Path, name: Option<&str>, force: bool, lang: Option<&str>) -> anyhow::Result<()> {
    if !src.exists() {
        anyhow::bail!("不存在: {}", src.display());
    }
    if src.is_dir() {
        install_dir(src, name, force)
    } else {
        install_file(src, name, force, lang)
    }
}

fn install_file(
    src: &Path,
    name: Option<&str>,
    force: bool,
    lang: Option<&str>,
) -> anyhow::Result<()> {
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plugin")
        .to_string();
    let n = prepare_name(name.unwrap_or(&stem), force)?;
    let kind = classify_src(src, lang)?;
    let staging = make_staging(&n)?;
    let result = (|| -> anyhow::Result<String> {
        match kind {
            SrcKind::NativeExe => {
                #[cfg(windows)]
                if !is_windows_exe(src) {
                    anyhow::bail!(
                        "Windows 原生插件必须是 .exe，脚本请用 .sh/.py/.cmd/.ps1: {}",
                        src.display()
                    );
                }
                let exe_name = if cfg!(windows) {
                    format!("rxt-{n}.exe")
                } else {
                    format!("rxt-{n}")
                };
                let dest = staging.join(&exe_name);
                fs::copy(src, &dest)?;
                chmod_755(&dest)?;
                #[cfg(windows)]
                crate::sign::sign_path(&dest, false)?;
                Ok(exe_name)
            }
            SrcKind::Script(script_lang) => {
                let text = fs::read_to_string(src).unwrap_or_default();
                if text.is_empty() && src.metadata()?.len() > 0 {
                    anyhow::bail!("不像文本脚本: {}", src.display());
                }
                let (exe_name, _) = write_plugin_files(&staging, &n, script_lang, &text)?;
                Ok(exe_name)
            }
        }
    })();
    let exe_name = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let man_force = is_builtin(&n) && force;
    write_manifest(&staging, &n, &exe_name, man_force)?;
    let dir = atomic_swap(&n, &staging)?;
    eprintln!("# 已安装 {} -> {}", n, dir.join(exe_name).display());
    Ok(())
}

fn install_dir(src: &Path, name: Option<&str>, force: bool) -> anyhow::Result<()> {
    let man_path = src.join("manifest.toml");
    let (stem, mut man_opt): (String, Option<Manifest>) = if man_path.is_file() {
        let man: Manifest = toml::from_str(&fs::read_to_string(&man_path)?)?;
        (man.name.clone(), Some(man))
    } else {
        let exe = find_exe_in_dir(src)?;
        let stem = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();
        (stem, None)
    };
    let n = prepare_name(name.unwrap_or(&stem), force)?;
    let staging = make_staging(&n)?;
    if let Err(e) = copy_tree(src, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let mut man = if let Some(mut m) = man_opt.take() {
        m.name = n.clone();
        if force {
            m.force = true;
        }
        if !staging.join(&m.exe).is_file() {
            match find_exe_in_dir(&staging) {
                Ok(exe) => {
                    m.exe = exe
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&m.exe)
                        .to_string();
                }
                Err(e) => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(e);
                }
            }
        }
        m
    } else {
        match find_exe_in_dir(&staging) {
            Ok(exe) => Manifest {
                name: n.clone(),
                exe: exe
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("rxt-plugin")
                    .to_string(),
                force: is_builtin(&n) && force,
            },
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        }
    };
    if is_builtin(&n) && !man.force && !force {
        let _ = fs::remove_dir_all(&staging);
        anyhow::bail!("'{n}' 是内置命令。覆盖请加 --force");
    }
    if force && is_builtin(&n) {
        man.force = true;
    }
    man.name = n.clone();
    if let Err(e) = ensure_windows_launcher(&staging, &mut man) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    if let Err(e) = write_manifest(&staging, &man.name, &man.exe, man.force) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let exe_path = staging.join(&man.exe);
    chmod_755(&exe_path)?;
    #[cfg(windows)]
    if let Err(e) = sign_exes(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let dir = atomic_swap(&n, &staging)?;
    eprintln!("# 已安装 {} -> {}", n, dir.join(&man.exe).display());
    Ok(())
}

fn remove(name: &str) -> anyhow::Result<()> {
    let n = sanitize(name)?;
    let dir = plugin_dir(&n);
    if !dir.exists() {
        if crate::recipe::resolve_path(&n).is_some() {
            anyhow::bail!("未安装插件 '{n}'，但有 recipe。删除: rxt recipe rm {n}");
        }
        anyhow::bail!("未安装: {n}");
    }
    fs::remove_dir_all(&dir)?;
    eprintln!("# 已移除 {n}");
    Ok(())
}

fn which_cmd(name: &str, json: bool) -> anyhow::Result<()> {
    let (kind, path) = if let Some(p) = resolve_forced(name) {
        ("force", p.display().to_string())
    } else if is_builtin(name) {
        ("builtin", "builtin".to_string())
    } else if let Some(p) = resolve(name) {
        let kind = if resolve_installed(name).is_some() {
            "installed"
        } else {
            "path"
        };
        (kind, p.display().to_string())
    } else if let Some(p) = crate::recipe::resolve_path(name) {
        ("recipe", p.display().to_string())
    } else {
        anyhow::bail!("找不到: {name}");
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": name,
                "kind": kind,
                "path": path,
            }))?
        );
    } else if kind == "force" {
        println!("{path} (force)");
    } else if kind == "recipe" {
        println!("{path} (recipe)");
    } else {
        println!("{path}");
    }
    Ok(())
}

pub fn rxt_home() -> PathBuf {
    if let Ok(p) = std::env::var("RXT_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
}

pub fn lib_dir() -> PathBuf {
    rxt_home().join("lib")
}

fn tools_bin_name() -> &'static str {
    if cfg!(windows) {
        "rxt-tools.exe"
    } else {
        "rxt-tools"
    }
}

pub fn tools_bin() -> PathBuf {
    lib_dir().join(tools_bin_name())
}

pub fn install_tools_bin_from_sibling() -> anyhow::Result<PathBuf> {
    let dest = tools_bin();
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join(tools_bin_name()));
        }
    }
    cands.push(PathBuf::from("/usr/local/bin").join(tools_bin_name()));
    if let Some(home) = dirs::home_dir() {
        cands.push(home.join(".local/bin").join(tools_bin_name()));
    }
    let src = cands.into_iter().find(|p| p.is_file());
    let Some(src) = src else {
        if dest.is_file() {
            return Ok(dest);
        }
        anyhow::bail!(
            "找不到 {}。先 cargo build --release --bin rxt-tools 或 rxt publish",
            tools_bin_name()
        );
    };
    fs::create_dir_all(lib_dir())?;
    if dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::copy(&src, &dest)?;
    chmod_755(&dest)?;
    Ok(dest)
}

pub fn installed_names() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(plugins_dir()) {
        for e in rd.flatten() {
            let dir = e.path();
            if dir.is_dir() {
                if let Ok(man) = load_manifest(&dir) {
                    out.push(man.name);
                }
            }
        }
    }
    out.sort();
    out
}

pub fn tools_describe() -> anyhow::Result<serde_json::Value> {
    let exe = if tools_bin().is_file() {
        tools_bin()
    } else {
        std::env::current_exe()?
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(tools_bin_name())
    };
    let out = Command::new(&exe).arg("--describe").output()?;
    if !out.status.success() {
        anyhow::bail!("rxt-tools --describe 失败");
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn seed_stamp() -> PathBuf {
    plugins_dir().join(".stdlib-seeded-0.10")
}

pub fn auto_seed() -> anyhow::Result<()> {
    if seed_stamp().is_file() {
        return Ok(());
    }
    if !tools_bin().is_file() {
        let _ = install_tools_bin_from_sibling();
    }
    if !tools_bin().is_file() {
        return Ok(());
    }
    seed_all(false)?;
    Ok(())
}

pub fn seed_all(force: bool) -> anyhow::Result<usize> {
    let _ = install_tools_bin_from_sibling();
    let mut n = 0usize;
    for name in STDLIB {
        seed_one(name, force)?;
        n += 1;
    }
    fs::create_dir_all(plugins_dir())?;
    fs::write(seed_stamp(), env!("CARGO_PKG_VERSION"))?;
    Ok(n)
}

pub fn seed_one(name: &str, force: bool) -> anyhow::Result<()> {
    let n = sanitize(name)?;
    if is_builtin(&n) {
        anyhow::bail!("'{n}' 是核心命令，不必 seed");
    }
    if !is_stdlib(&n) {
        anyhow::bail!("'{n}' 不是官方标准库。用户插件用 rxt plugin new / install");
    }
    let tools = if tools_bin().is_file() {
        tools_bin()
    } else {
        install_tools_bin_from_sibling()?
    };
    let dir = plugin_dir(&n);
    if dir.exists() && !force {
        if load_manifest(&dir).is_ok() {
            return Ok(());
        }
    }
    let staging = make_staging(&n)?;
    let exe_name = if cfg!(windows) {
        format!("rxt-{n}.cmd")
    } else {
        format!("rxt-{n}")
    };
    let dest = staging.join(&exe_name);
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&dest);
        std::os::unix::fs::symlink(&tools, &dest)?;
    }
    #[cfg(windows)]
    {
        fs::write(
            &dest,
            format!(
                "@echo off\r\nset \"RXT_PLUGIN_NAME={n}\"\r\n\"{}\" %*\r\n",
                tools.display()
            ),
        )?;
    }
    write_manifest(&staging, &n, &exe_name, false)?;
    atomic_swap(&n, &staging)?;
    Ok(())
}

#[cfg(test)]
fn with_plugins_dir<F, R>(dir: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            PLUGINS_DIR_OVERRIDE.with(|s| *s.borrow_mut() = None);
        }
    }
    let _g = Guard;
    PLUGINS_DIR_OVERRIDE.with(|s| *s.borrow_mut() = Some(dir.to_path_buf()));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_N: AtomicU64 = AtomicU64::new(0);

    fn test_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rxt-plugin-test-{}-{}",
            std::process::id(),
            TEST_N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

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
        let parsed = crate::core_cli::parse_cli(vec![
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
            crate::core_cli::Command::External(args) => {
                assert_eq!(args, vec!["hello", "--flag"]);
            }
            _ => panic!("未知命令没有进入 External"),
        }
    }

    #[test]
    fn clap_plugin_new_parses_body_and_lang() {
        let parsed = crate::core_cli::parse_cli(vec![
            "rxt".into(),
            "plugin".into(),
            "new".into(),
            "hello".into(),
            "echo hi".into(),
            "--lang".into(),
            "py".into(),
            "--force".into(),
        ])
        .unwrap()
        .unwrap();
        match parsed.command {
            crate::core_cli::Command::Plugin {
                action,
                target,
                content,
                lang,
                force,
                ..
            } => {
                assert_eq!(action, "new");
                assert_eq!(target.as_deref(), Some("hello"));
                assert_eq!(content.as_deref(), Some("echo hi"));
                assert_eq!(lang.as_deref(), Some("py"));
                assert!(force);
            }
            _ => panic!("expected Plugin"),
        }
    }

    #[test]
    fn builtins_include_plugin_and_sign() {
        assert!(!is_builtin("http"));
        assert!(is_stdlib("http"));
        assert!(is_stdlib("pack"));
        assert!(is_builtin("plugin"));
        assert!(is_builtin("sign"));
        assert!(is_builtin("exec"));
        assert!(!is_builtin("not-a-cmd"));
    }

    #[test]
    fn parse_lang_aliases() {
        assert_eq!(parse_lang("bash").unwrap(), Lang::Sh);
        assert_eq!(parse_lang("python3").unwrap(), Lang::Py);
        assert_eq!(parse_lang("bat").unwrap(), Lang::Cmd);
        assert_eq!(parse_lang("pwsh").unwrap(), Lang::Ps1);
        assert!(parse_lang("ruby").is_err());
    }

    #[test]
    fn shebang_and_lang_detect() {
        assert_eq!(
            resolve_lang(None, Some("#!/usr/bin/env python3\nprint(1)")).unwrap(),
            Lang::Py
        );
        assert_eq!(
            resolve_lang(None, Some("#!/usr/bin/env bash\necho 1")).unwrap(),
            Lang::Sh
        );
        assert_eq!(
            resolve_lang(None, Some("@echo off\necho 1")).unwrap(),
            Lang::Cmd
        );
        let wrapped = with_shebang("print('x')", Lang::Py);
        assert!(wrapped.starts_with("#!/usr/bin/env python3\n"));
        assert!(with_shebang("#!/usr/bin/env python3\nprint(1)", Lang::Py)
            .starts_with("#!/usr/bin/env python3"));
    }

    #[test]
    fn new_then_run_and_show() {
        let root = test_dir();
        with_plugins_dir(&root, || {
            create_plugin(
                "hello",
                Lang::Sh,
                Some("echo hello-$*"),
                false,
                false,
                false,
            )
            .unwrap();
            let dir = plugin_dir("hello");
            assert!(dir.join("manifest.toml").is_file());
            assert!(dir.join("rxt-hello.sh").is_file());
            let man = load_manifest(&dir).unwrap();
            assert_eq!(man.name, "hello");
            let src = fs::read_to_string(dir.join("rxt-hello.sh")).unwrap();
            assert!(src.contains("echo hello-$*"));
            assert!(src.starts_with("#!"));
            run_external(&["hello".into(), "world".into()], None, None).unwrap();
            which_cmd("hello", false).unwrap();
            show("hello", false).unwrap();
            remove("hello").unwrap();
            assert!(!dir.exists());
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn new_rejects_builtin_without_force() {
        let root = test_dir();
        with_plugins_dir(&root, || {
            let err = create_plugin("exec", Lang::Sh, Some("echo no"), false, false, false)
                .unwrap_err()
                .to_string();
            assert!(err.contains("内置"));
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_installs_existing_file() {
        let root = test_dir();
        let src = root.join("rxt-fromfile.sh");
        fs::write(&src, "#!/usr/bin/env bash\necho fromfile\n").unwrap();
        with_plugins_dir(&root.join("plugins"), || {
            let cli = PluginCli {
                action: "add",
                target: Some(src.to_str().unwrap()),
                name: None,
                content: None,
                body: None,
                force: false,
                json: false,
                lang: Some("sh"),
                stdin: false,
                open: false,
            };
            run(cli).unwrap();
            assert!(plugin_dir("fromfile").join("rxt-fromfile.sh").is_file());
            remove("fromfile").unwrap();
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_missing_name_creates() {
        let root = test_dir();
        with_plugins_dir(&root, || {
            let cli = PluginCli {
                action: "add",
                target: Some("made"),
                name: None,
                content: Some("echo made"),
                body: None,
                force: false,
                json: false,
                lang: Some("sh"),
                stdin: false,
                open: false,
            };
            run(cli).unwrap();
            assert!(plugin_dir("made").join("rxt-made.sh").is_file());
            remove("made").unwrap();
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_dir_copies_extra_files() {
        let root = test_dir();
        let src = root.join("pkg");
        fs::create_dir_all(src.join("lib")).unwrap();
        fs::write(src.join("rxt-packy.sh"), "#!/usr/bin/env bash\necho $1\n").unwrap();
        fs::write(src.join("lib").join("data.txt"), "asset\n").unwrap();
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git").join("HEAD"), "ref\n").unwrap();
        with_plugins_dir(&root.join("plugins"), || {
            install(&src, Some("packy"), false, Some("sh")).unwrap();
            let dir = plugin_dir("packy");
            assert!(dir.join("lib").join("data.txt").is_file());
            assert!(!dir.join(".git").exists());
            remove("packy").unwrap();
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn new_force_overwrites() {
        let root = test_dir();
        with_plugins_dir(&root, || {
            create_plugin("ow", Lang::Sh, Some("echo v1"), false, false, false).unwrap();
            create_plugin("ow", Lang::Sh, Some("echo v2"), true, false, false).unwrap();
            let src = fs::read_to_string(plugin_dir("ow").join("rxt-ow.sh")).unwrap();
            assert!(src.contains("echo v2"));
            remove("ow").unwrap();
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn recipe_fallback_runs() {
        let plug = test_dir();
        let rec = test_dir();
        with_plugins_dir(&plug, || {
            crate::recipe::with_recipes_dir(&rec, || {
                crate::recipe::run(
                    "add",
                    Some("rfb"),
                    Some("echo recipe-ok"),
                    &[],
                    false,
                    false,
                )
                .unwrap();
                run_external(&["rfb".into()], None, None).unwrap();
                which_cmd("rfb", false).unwrap();
            });
        });
        let _ = fs::remove_dir_all(&plug);
        let _ = fs::remove_dir_all(&rec);
    }

    #[test]
    fn looks_like_path_detects() {
        assert!(looks_like_path("./x"));
        assert!(looks_like_path("foo.py"));
        assert!(!looks_like_path("hello"));
    }

    #[test]
    fn skip_copy_junk() {
        assert!(skip_copy_name(".git"));
        assert!(skip_copy_name("node_modules"));
        assert!(!skip_copy_name("lib"));
    }

    #[test]
    fn unknown_command_hints_new() {
        let plug = test_dir();
        let rec = test_dir();
        with_plugins_dir(&plug, || {
            crate::recipe::with_recipes_dir(&rec, || {
                let err = run_external(&["no-such-cmd-xyz".into()], None, None)
                    .unwrap_err()
                    .to_string();
                assert!(err.contains("plugin new"), "{err}");
                assert!(err.contains("recipe add"), "{err}");
            });
        });
        let _ = fs::remove_dir_all(&plug);
        let _ = fs::remove_dir_all(&rec);
    }
}
