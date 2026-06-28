//! rxt git - AI 友好的 git 包装
//! status/diff/log/branch 输出 JSON,AI 直接消化

use std::process::Command;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub struct GitStatus {
    pub branch: String,
    pub clean: bool,
    pub staged: Vec<FileChange>,
    pub modified: Vec<FileChange>,
    pub untracked: Vec<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct FileChange {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}

fn git(args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").args(args).output()?;
    if !out.status.success() {
        anyhow::bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn is_git_repo() -> bool {
    Command::new("git").args(["rev-parse", "--git-dir"]).output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn run(subcmd: GitSubCmd, json_output: bool) -> anyhow::Result<()> {
    if !is_git_repo() {
        eprintln!("Not a git repository");
        std::process::exit(1);
    }
    match subcmd {
        GitSubCmd::Status { path } => status(path.as_deref(), json_output),
        GitSubCmd::Diff { path, staged } => diff(path.as_deref(), staged, json_output),
        GitSubCmd::Log { num } => log(num, json_output),
        GitSubCmd::Branch => branch(json_output),
        GitSubCmd::Add { paths } => add_cmd(&paths),
        GitSubCmd::Commit { message, all, dry_run } => commit_cmd(all, dry_run, message),
        GitSubCmd::Undo { soft } => undo(soft),
        GitSubCmd::Push { remote, force, upstream, branch } => push(&remote, force, upstream, branch.as_deref()),
        GitSubCmd::Pull { remote, rebase, branch } => pull(&remote, rebase, branch.as_deref()),
        GitSubCmd::Fetch { remote, refspec } => fetch(&remote, refspec.as_deref()),
        GitSubCmd::Remote { add, url, del, rename, to, set_url } => remote_cmd(add.as_deref(), url.as_deref(), del.as_deref(), rename.as_deref(), to.as_deref(), set_url.as_deref()),
    }
}

#[derive(clap::Subcommand)]
pub enum GitSubCmd {
    #[command(about = "查看改动状态")]
    Status { path: Option<PathBuf> },
    #[command(about = "查看 diff")]
    Diff {
        path: Option<PathBuf>,
        #[arg(long, help = "显示已 stage 的改动")]
        staged: bool,
    },
    #[command(about = "查看最近 N 条提交")]
    Log { num: Option<usize> },
    #[command(about = "列出分支")]
    Branch,
    #[command(about = "Stage 文件")]
    Add { paths: Vec<PathBuf> },
    #[command(about = "提交")]
    Commit {
        #[arg(short, long = "message")]
        message: Option<String>,
        #[arg(long)] all: bool,
        #[arg(long)] dry_run: bool,
    },
    #[command(about = "撤销上次 commit (默认保留改动)")]
    Undo {
        #[arg(long, help = "soft 模式: 保留改动在 stage")]
        soft: bool,
    },
    #[command(about = "推送到远程 (默认 origin,可 --remote 指定)")]
    Push {
        #[arg(long, default_value = "origin")] remote: String,
        #[arg(long, help = "强制推送")] force: bool,
        #[arg(long, help = "推送并设置上游跟踪")] upstream: bool,
        #[arg(help = "分支名(默认当前分支)")] branch: Option<String>,
    },
    #[command(about = "拉取远程 (默认 origin)")]
    Pull {
        #[arg(long, default_value = "origin")] remote: String,
        #[arg(long, help = "用 rebase 而非 merge")] rebase: bool,
        #[arg(help = "分支名(默认当前分支)")] branch: Option<String>,
    },
    #[command(about = "fetch 远程(不合并)")]
    Fetch {
        #[arg(long, default_value = "origin")] remote: String,
        #[arg(help = "可选 refspec")] refspec: Option<String>,
    },
    #[command(about = "远程仓库管理 (list/add/remove/set-url)")]
    Remote {
        #[arg(long, help = "添加: --add 名称 URL")] add: Option<String>,
        #[arg(long, help = "add 时的 URL")] url: Option<String>,
        #[arg(long = "del", help = "删除远程")] del: Option<String>,
        #[arg(long, help = "改名: --rename 旧 新")] rename: Option<String>,
        #[arg(long, help = "rename 的新名字")] to: Option<String>,
        #[arg(long = "set-url", help = "改 URL: --set-url 名称 --url URL")] set_url: Option<String>,
    },
}

fn status(path: Option<&Path>, json_output: bool) -> anyhow::Result<()> {
    if let Some(p) = path {
        std::env::set_current_dir(p)?;
    }
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "HEAD".to_string()).trim().to_string();
    let porcelain = git(&["status", "--porcelain"])?;
    let mut staged = Vec::new();
    let mut modified = Vec::new();
    let mut untracked = Vec::new();

    for line in porcelain.lines() {
        if line.len() < 3 { continue; }
        let xy = &line[..2];
        let path = line[3..].trim().to_string();
        match xy {
            "M " | "A " | "C " | "R " => staged.push(change_with_stats(&path)),
            " M" | " D" | "MM" => modified.push(change_with_stats(&path)),
            "??" => untracked.push(path),
            _ => {}
        }
    }

    if json_output {
        let result = GitStatus {
            branch,
            clean: staged.is_empty() && modified.is_empty() && untracked.is_empty(),
            staged,
            modified,
            untracked,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("On branch {}", branch);
        if staged.is_empty() && modified.is_empty() && untracked.is_empty() {
            println!("Working tree clean");
        } else {
            for s in &staged { println!("  staged:    {}", s.path); }
            for m in &modified { println!("  modified:  {}", m.path); }
            for u in &untracked { println!("  untracked: {}", u); }
        }
    }
    Ok(())
}

fn change_with_stats(path: &str) -> FileChange {
    let diff = git(&["diff", "--numstat", "--", path]).unwrap_or_default();
    let mut ins = 0;
    let mut dels = 0;
    for line in diff.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            ins += parts[0].parse::<usize>().unwrap_or(0);
            dels += parts[1].parse::<usize>().unwrap_or(0);
        }
    }
    FileChange { path: path.to_string(), insertions: ins, deletions: dels }
}

fn diff(path: Option<&Path>, staged: bool, json_output: bool) -> anyhow::Result<()> {
    if let Some(p) = path {
        std::env::set_current_dir(p)?;
    }
    let mut arg_strs: Vec<String> = vec!["diff".to_string()];
    if staged {
        arg_strs.push("--cached".to_string());
    }
    let refs: Vec<&str> = arg_strs.iter().map(|s| s.as_str()).collect();
    let output = git(&refs)?;
    if json_output {
        let mut files: Vec<serde_json::Value> = Vec::new();
        let mut current_file = String::new();
        let mut insertions = 0;
        let mut deletions = 0;
        for line in output.lines() {
            if line.starts_with("diff --git") {
                if !current_file.is_empty() {
                    files.push(serde_json::json!({
                        "file": current_file,
                        "insertions": insertions,
                        "deletions": deletions,
                    }));
                }
                if let Some(name) = line.split(" b/").nth(1) {
                    current_file = name.to_string();
                } else {
                    current_file = String::new();
                }
                insertions = 0;
                deletions = 0;
            } else if line.starts_with("+") && !line.starts_with("+++") {
                insertions += 1;
            } else if line.starts_with("-") && !line.starts_with("---") {
                deletions += 1;
            }
        }
        if !current_file.is_empty() {
            files.push(serde_json::json!({
                "file": current_file,
                "insertions": insertions,
                "deletions": deletions,
            }));
        }
        let json = serde_json::json!({
            "files": files,
            "total_files": files.len(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        print!("{}", output);
    }
    Ok(())
}

fn log(num: Option<usize>, json_output: bool) -> anyhow::Result<()> {
    let n = num.unwrap_or(10);
    let fmt = if json_output { "%H|%h|%an|%ae|%ad|%s" } else { "%h %an %ad %s" };
    let output = git(&["log", &format!("-n{}", n), &format!("--format={}", fmt), "--date=iso-strict"])?;
    if json_output {
        let mut entries = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 6 {
                entries.push(serde_json::json!({
                    "hash": parts[0],
                    "short": parts[1],
                    "author": parts[2],
                    "email": parts[3],
                    "date": parts[4],
                    "subject": parts[5],
                }));
            }
        }
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        print!("{}", output);
    }
    Ok(())
}

fn branch(_json_output: bool) -> anyhow::Result<()> {
    let output = git(&["branch", "--format=%(HEAD) %(refname:short)"])?;
    for line in output.lines() {
        if line.starts_with("* ") {
            println!("  * {}", &line[2..]);
        } else if !line.is_empty() {
            println!("    {}", line);
        }
    }
    Ok(())
}

fn add_cmd(paths: &[PathBuf]) -> anyhow::Result<()> {
    if paths.is_empty() {
        git(&["add", "-A"])?;
        println!("Added all changes");
    } else {
        let mut args = vec!["add".to_string()];
        for p in paths {
            args.push(p.to_string_lossy().to_string());
        }
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git(&refs)?;
        println!("Added {} file(s)", paths.len());
    }
    Ok(())
}

fn commit_cmd(all: bool, dry_run: bool, message: Option<String>) -> anyhow::Result<()> {
    if all && !dry_run {
        git(&["add", "-A"])?;
    }
    let status = git(&["status", "--porcelain"])?;
    if status.trim().is_empty() {
        eprintln!("Nothing to commit");
        return Ok(());
    }
    if dry_run {
        let stat = git(&["diff", "--cached", "--stat"])?;
        println!("{}", stat);
        let msg = message.unwrap_or_else(|| "<auto-generated>".to_string());
        println!("[dry-run] git commit -m \"{}\"", msg);
        return Ok(());
    }
    let msg = message.unwrap_or_else(|| "chore: update files".to_string());
    let out = git(&["commit", "-m", &msg])?;
    println!("{}", out);
    Ok(())
}

fn undo(soft: bool) -> anyhow::Result<()> {
    if soft {
        git(&["reset", "--soft", "HEAD~1"])?;
        println!("Soft reset: changes kept in stage");
    } else {
        git(&["reset", "--mixed", "HEAD~1"])?;
        println!("Mixed reset: changes kept in working tree");
    }
    Ok(())
}

// ===== v0.4.0: 暴露 VCS 元信息给 map 等命令(库 API, 不打印) =====

/// 当前 HEAD commit 完整 hash(失败返回 None, 不退出)
pub fn current_head() -> Option<String> {
    if !is_git_repo() { return None; }
    git(&["rev-parse", "HEAD"]).ok().map(|s| s.trim().to_string())
}

/// 当前 HEAD 短 hash(前 7 位)
pub fn current_head_short() -> Option<String> {
    current_head().map(|h| h.chars().take(7).collect())
}

/// 当前分支名(分离 HEAD 时返回 None)
pub fn current_branch() -> Option<String> {
    if !is_git_repo() { return None; }
    let b = git(&["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let b = b.trim();
    if b.is_empty() || b == "HEAD" {
        None  // 分离 HEAD
    } else {
        Some(b.to_string())
    }
}

/// 工作区是否有未提交改动
pub fn is_dirty() -> bool {
    if !is_git_repo() { return false; }
    git(&["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// 自上次 HEAD 以来变更的文件清单(用于 map 增量缓存)
/// 返回相对路径列表; 失败或非 git 返回空
pub fn changed_files_since_head() -> Vec<String> {
    if !is_git_repo() { return vec![]; }
    git(&["status", "--porcelain"])
        .map(|s| {
            s.lines()
                .filter_map(|l| {
                    // porcelain 格式: "XY path" 或 "XY "path with space""
                    let l = l.strip_prefix("\"\"").unwrap_or(l);
                    if l.len() < 4 { return None; }
                    // 跳过前 2 个状态字符 + 1 空格
                    let path = l[3..].trim();
                    let path = path.trim_matches('"');
                    if path.is_empty() { None } else { Some(path.to_string()) }
                })
                .collect()
        })
        .unwrap_or_default()
}

// ===== push / pull / fetch / remote =====

fn push(remote: &str, force: bool, upstream: bool, branch: Option<&str>) -> anyhow::Result<()> {
    let br = branch.map(|b| b.to_string()).or_else(current_branch);
    let mut args: Vec<String> = vec!["push".into()];
    if force { args.push("--force".into()); }
    if upstream { args.push("-u".into()); }
    args.push(remote.into());
    if let Some(b) = &br { args.push(b.clone()); }

    // push 输出去 stderr,用 inherit 让用户直接看进度
    let status = Command::new("git").args(args.iter().map(|s| s.as_str()))
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if status.success() {
        let target = match &br { Some(b) => format!("{} {}", remote, b), None => remote.into() };
        println!("✓ 已推送到 {}", target);
        Ok(())
    } else {
        anyhow::bail!("push 失败 (exit {})", status.code().unwrap_or(-1))
    }
}

fn pull(remote: &str, rebase: bool, branch: Option<&str>) -> anyhow::Result<()> {
    let br = branch.map(|b| b.to_string()).or_else(current_branch);
    let mut args: Vec<String> = vec!["pull".into()];
    if rebase { args.push("--rebase".into()); }
    args.push(remote.into());
    if let Some(b) = &br { args.push(b.clone()); }

    let status = Command::new("git").args(args.iter().map(|s| s.as_str()))
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if status.success() { println!("✓ pull 完成"); Ok(()) }
    else { anyhow::bail!("pull 失败 (exit {})", status.code().unwrap_or(-1)) }
}

fn fetch(remote: &str, refspec: Option<&str>) -> anyhow::Result<()> {
    let mut args: Vec<String> = vec!["fetch".into(), remote.into()];
    if let Some(r) = refspec { args.push(r.into()); }
    let status = Command::new("git").args(args.iter().map(|s| s.as_str()))
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if status.success() { println!("✓ fetch 完成"); Ok(()) }
    else { anyhow::bail!("fetch 失败 (exit {})", status.code().unwrap_or(-1)) }
}

fn remote_cmd(add: Option<&str>, url: Option<&str>, del: Option<&str>, rename: Option<&str>, to: Option<&str>, set_url: Option<&str>) -> anyhow::Result<()> {
    if let Some(name) = add {
        let u = url.ok_or_else(|| anyhow::anyhow!("--add 需要 --url"))?;
        git(&["remote", "add", name, u])?;
        println!("✓ 已添加远程 {} -> {}", name, u);
        return Ok(());
    }
    if let Some(name) = del {
        git(&["remote", "remove", name])?;
        println!("✓ 已删除远程 {}", name);
        return Ok(());
    }
    if let Some(old) = rename {
        let new = to.ok_or_else(|| anyhow::anyhow!("--rename 需要 --to"))?;
        git(&["remote", "rename", old, new])?;
        println!("✓ 远程 {} -> {}", old, new);
        return Ok(());
    }
    if let Some(name) = set_url {
        let u = url.ok_or_else(|| anyhow::anyhow!("--set-url 需要 --url"))?;
        git(&["remote", "set-url", name, u])?;
        println!("✓ {} URL -> {}", name, u);
        return Ok(());
    }
    // 无参数: list
    let out = git(&["remote", "-v"])?;
    if out.trim().is_empty() {
        println!("(无远程仓库)");
    } else {
        print!("{}", out);
    }
    Ok(())
}