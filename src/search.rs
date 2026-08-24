//! search — 统一搜索 (find + grep 合并)
//!
//! 自动判断是搜文件名还是搜内容:
//!   - 包含 glob 字符 (* ? [) → 搜文件名
//!   - 否则 → 搜内容
//!   --name / --content 强制指定
//!
//! 用法:
//!   rxt search "TODO"               # 搜内容
//!   rxt search "*.rs"               # 搜文件名 (glob)
//!   rxt search "fn main" --type rs  # 搜内容，限定 .rs
//!   rxt search --name "*.rs"        # 强制搜文件名
//!   rxt search --content "TODO"     # 强制搜内容

use crate::common;
use std::path::Path;

pub fn run(
    query: &str,
    path: Option<&str>,
    file_type: Option<&str>,
    force_name: bool,
    force_content: bool,
    json: bool,
    max_results: usize,
) -> anyhow::Result<()> {
    let root = path.unwrap_or(".");
    let root_path = Path::new(root);

    // 自动判断搜索模式
    let search_name = if force_name {
        true
    } else if force_content {
        false
    } else {
        // 自动判断：包含 glob 字符 → 搜文件名
        query.contains('*') || query.contains('?') || query.contains('[')
    };

    if search_name {
        search_by_name(query, root_path, file_type, json, max_results)
    } else {
        search_by_content(query, root_path, file_type, json, max_results)
    }
}

/// 按文件名搜索 (glob 匹配)
fn search_by_name(
    pattern: &str,
    root: &Path,
    file_type: Option<&str>,
    json: bool,
    max: usize,
) -> anyhow::Result<()> {
    // 用 walk_clean 遍历
    let exts: Option<Vec<&str>> = file_type.map(|t| t.split(',').map(|s| s.trim()).collect());
    let exts_ref = exts.as_deref();

    let files = common::walk_clean(root, exts_ref, None);
    let results: Vec<_> = files
        .into_iter()
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            common::glob_match(pattern, name)
        })
        .take(max)
        .collect();

    if json {
        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|p| {
                let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                serde_json::json!({
                    "path": p.to_string_lossy(),
                    "name": name,
                    "relative": rel,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "mode": "name",
                "pattern": pattern,
                "count": entries.len(),
                "results": entries,
            })
        );
    } else {
        if results.is_empty() {
            println!("未找到匹配 '{}' 的文件", pattern);
        } else {
            for p in &results {
                let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy();
                println!("{}", rel);
            }
            eprintln!("--- {} 个结果 ---", results.len());
        }
    }
    Ok(())
}

/// 按内容搜索 (grep)
fn search_by_content(
    pattern: &str,
    root: &Path,
    file_type: Option<&str>,
    json: bool,
    max: usize,
) -> anyhow::Result<()> {
    let exts: Option<Vec<&str>> = file_type.map(|t| t.split(',').map(|s| s.trim()).collect());
    let exts_ref = exts.as_deref();

    let files = common::walk_clean(root, exts_ref, None);
    let mut matches: Vec<(std::path::PathBuf, usize, String)> = Vec::new();

    let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("正则编译失败: {}", e))?;

    for file in files {
        if matches.len() >= max {
            break;
        }
        if crate::common::skip_heavy_file(&file) {
            continue;
        }
        let content = match std::fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (lineno, line) in content.lines().enumerate() {
            if matches.len() >= max {
                break;
            }
            if re.is_match(line) {
                matches.push((file.clone(), lineno + 1, line.trim().to_string()));
            }
        }
    }

    if json {
        let results: Vec<serde_json::Value> = matches
            .iter()
            .map(|(path, lineno, line)| {
                let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
                serde_json::json!({
                    "file": rel,
                    "line": lineno,
                    "text": line,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "mode": "content",
                "pattern": pattern,
                "count": results.len(),
                "results": results,
            })
        );
    } else {
        if matches.is_empty() {
            println!("未找到包含 '{}' 的行", pattern);
        } else {
            let mut last_file = String::new();
            for (path, lineno, line) in &matches {
                let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
                if rel != last_file {
                    last_file = rel.to_string();
                    println!("\n{}", rel);
                }
                println!("  {}: {}", lineno, line);
            }
            eprintln!("\n--- {} 处匹配 ---", matches.len());
        }
    }
    Ok(())
}
