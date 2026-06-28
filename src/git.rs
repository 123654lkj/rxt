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
