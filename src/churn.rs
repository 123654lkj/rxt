//! churn — git 历史热点分析 (v0.8.0)
//!
//! 分析 git log, 找出高频改动的文件 (= 最易碎的热点).
//! 灵感: graphcode 的 churn 追踪 + loop-engineering 的变更模式分析.
//!
//! rxt churn                    # 全部历史
//! rxt churn --since="1 month"  # 时间范围
//! rxt churn --by-author        # 按作者分组
//! rxt churn --json

use std::collections::HashMap;
use serde_json::json;

/// 文件 churn 统计
#[derive(Default)]
struct FileChurn {
    commits: usize,
    added: usize,
    deleted: usize,
}

pub fn run(since: Option<&str>, by_author: bool, json: bool) -> anyhow::Result<()> {
    // 不是 git 仓库就退出
    let in_repo = crate::git::git(&["rev-parse", "--git-dir"]).is_ok();
    if !in_repo {
        anyhow::bail!("当前目录不是 git 仓库");
    }

    // 构建 git log 命令
    // --numstat: 每行 "added\tdeleted\tpath"
    // --format: 提交分隔符 + 作者 + 日期
    let mut args = vec![
        "log".to_string(),
        "--numstat".to_string(),
        "--format=__COMMIT__|%an|%ad".to_string(),
        "--date=short".to_string(),
    ];
    if let Some(s) = since {
        args.push(format!("--since={}", s));
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = crate::git::git(&arg_refs)?;

    if by_author {
        run_by_author(&output, json)
    } else {
        run_by_file(&output, json)
    }
}

/// 按文件聚合 churn
fn run_by_file(output: &str, json: bool) -> anyhow::Result<()> {
    let mut churns: HashMap<String, FileChurn> = HashMap::new();

    for line in output.lines() {
        if line.starts_with("__COMMIT__") {
            continue; // 提交分隔行
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 {
            continue;
        }
        // 二进制文件 numstat 是 "-\t-\tpath"
        let added: usize = parts[0].parse().unwrap_or(0);
        let deleted: usize = parts[1].parse().unwrap_or(0);
        let path = parts[2].to_string();

        let entry = churns.entry(path).or_default();
        entry.commits += 1;
        entry.added += added;
        entry.deleted += deleted;
    }

    if churns.is_empty() {
        println!("没有找到提交记录.");
        return Ok(());
    }

    // 按提交次数降序排
    let mut sorted: Vec<(String, FileChurn)> = churns.into_iter().collect();
    sorted.sort_by(|a, b| b.1.commits.cmp(&a.1.commits));

    if json {
        let arr: Vec<_> = sorted.iter().map(|(path, c)| json!({
            "file": path,
            "commits": c.commits,
            "added": c.added,
            "deleted": c.deleted,
            "churn": c.added + c.deleted,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&json!(arr))?);
    } else {
        println!("🔥 churn 热点 (按提交次数排序, top 30)");
        println!();
        println!("{:<8} {:<10} {:<10} {}", "COMMITS", "+LINES", "-LINES", "FILE");
        println!("{}", "-".repeat(70));
        for (path, c) in sorted.iter().take(30) {
            // 高 churn (>=10 commits) 标 🔥
            let marker = if c.commits >= 10 { "🔥" } else { "  " };
            println!("{:<8} {:<10} {:<10} {} {}", c.commits, c.added, c.deleted, marker, path);
        }
        if sorted.len() > 30 {
            println!("\n  ... 共 {} 个文件 (用 --json 看全部)", sorted.len());
        }
    }
    Ok(())
}

/// 按作者聚合 churn
fn run_by_author(output: &str, json: bool) -> anyhow::Result<()> {
    let mut author_stats: HashMap<String, (usize, usize, usize)> = HashMap::new(); // (commits, added, deleted)
    let mut current_author = String::new();
    let mut current_counted = false;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("__COMMIT__|") {
            let parts: Vec<&str> = rest.split('|').collect();
            if parts.len() >= 1 {
                current_author = parts[0].to_string();
                current_counted = false;
            }
            continue;
        }
        // numstat 行
        let nparts: Vec<&str> = line.splitn(3, '\t').collect();
        if nparts.len() == 3 {
            let added: usize = nparts[0].parse().unwrap_or(0);
            let deleted: usize = nparts[1].parse().unwrap_or(0);
            let entry = author_stats.entry(current_author.clone()).or_insert((0, 0, 0));
            if !current_counted {
                entry.0 += 1; // 每个提交只算一次
                current_counted = true;
            }
            entry.1 += added;
            entry.2 += deleted;
        }
    }

    let mut sorted: Vec<(String, (usize, usize, usize))> = author_stats.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

    if json {
        let arr: Vec<_> = sorted.iter().map(|(author, (commits, added, deleted))| json!({
            "author": author, "commits": commits, "added": added, "deleted": deleted,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&json!(arr))?);
    } else {
        println!("👥 churn 按作者");
        println!();
        println!("{:<20} {:<8} {:<10} {:<10}", "AUTHOR", "COMMITS", "+LINES", "-LINES");
        println!("{}", "-".repeat(55));
        for (author, (commits, added, deleted)) in &sorted {
            println!("{:<20} {:<8} {:<10} {:<10}", author, commits, added, deleted);
        }
    }
    Ok(())
}
