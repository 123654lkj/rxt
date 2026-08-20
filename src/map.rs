//! rxt map — 项目结构简报 + 缓存引擎
//!
//! 一条命令吐出整个项目的结构化档案:
//!   kind / version / vcs / structure / stats / symbols / hotspots
//! 绑定 git HEAD 做缓存: HEAD 没变直接读缓存(零探测往返),
//! HEAD 变了只重算变更文件的符号(增量).

use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// map 命令入口
///
/// - dir: 项目根目录
/// - json_output: JSON 输出(否则文本简报)
/// - refresh: 强制全量重算, 忽略缓存
/// - depth: 结构树深度(默认 3)
pub fn run(dir: &Path, json_output: bool, refresh: bool, depth: usize) -> anyhow::Result<()> {
    let root = crate::common::safe_resolve(dir);
    let cache_path = root.join(".rxt-cache").join("map.json");

    // 1. 尝试读缓存(除非 --refresh)
    let head = crate::git::current_head_short();
    if !refresh {
        if let Some(cached) = load_cache(&cache_path) {
            if cached_head_matches(&cached, head.as_deref()) {
                // 命中缓存, 直接输出
                output(&cached.report, json_output, true)?;
                return Ok(());
            }
            // HEAD 变了, 增量更新
            if let Some(report) = incremental_update(&root, cached, depth) {
                let new_cache = Cache {
                    report: report.clone(),
                    head: head.clone(),
                };
                let _ = save_cache(&cache_path, &new_cache);
                output(&report, json_output, false)?;
                return Ok(());
            }
        }
    }

    // 2. 全量计算
    let report = build_full(&root, depth);
    let new_cache = Cache {
        report: report.clone(),
        head: head.clone(),
    };
    let _ = save_cache(&cache_path, &new_cache);
    output(&report, json_output, false)?;
    Ok(())
}

// ============================== 报告构建 ==============================

fn build_full(root: &Path, depth: usize) -> serde_json::Value {
    let kind = crate::common::detect_kind(root);
    let vcs = vcs_info();
    let structure = build_structure(root, depth);
    let files = collect_source_files(root);
    let stats = compute_stats(root, &files);
    let symbols = extract_all_symbols(root, &files);
    let hotspots = compute_hotspots(root, &files);

    json!({
        "rxt_version": env!("CARGO_PKG_VERSION"),
        "kind": kind.as_ref().map(|k| k.kind.clone()).unwrap_or_else(|| "unknown".into()),
        "name": kind.as_ref().map(|k| k.name.clone()).unwrap_or_else(|| {
            root.file_name().and_then(|n| n.to_str()).unwrap_or(".").to_string()
        }),
        "project_version": kind.as_ref().map(|k| k.version.clone()).unwrap_or_default(),
        "vcs": vcs,
        "structure": structure,
        "stats": stats,
        "symbols": symbols,
        "hotspots": hotspots,
    })
}

fn vcs_info() -> serde_json::Value {
    json!({
        "branch": crate::git::current_branch(),
        "head": crate::git::current_head_short(),
        "dirty": crate::git::is_dirty(),
    })
}

/// 构建 gitignore-aware 的项目结构树(文本形式, depth 层)
fn build_structure(root: &Path, depth: usize) -> String {
    let mut out = String::new();
    let name = root.file_name().and_then(|n| n.to_str()).unwrap_or(".");
    out.push_str(name);
    out.push('\n');
    let ignore = crate::common::load_gitignore_pub(root);
    structure_recurse(root, "", depth, 0, &ignore, &mut out);
    out.trim_end().to_string()
}

fn structure_recurse(
    dir: &Path,
    prefix: &str,
    max_depth: usize,
    cur: usize,
    ignore: &[String],
    out: &mut String,
) {
    if cur >= max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.flatten().collect();
    // 目录优先, 然后按名排序
    items.sort_by(|a, b| {
        let ad = a.path().is_dir();
        let bd = b.path().is_dir();
        match (ad, bd) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });
    let total = items.iter().filter(|e| should_show(e, ignore)).count();
    let mut shown = 0;
    for entry in &items {
        if !should_show(entry, ignore) {
            continue;
        }
        shown += 1;
        let last = shown == total;
        let name = entry.file_name().to_string_lossy().into_owned();
        let branch = if last { "└── " } else { "├── " };
        out.push_str(prefix);
        out.push_str(branch);
        out.push_str(&name);
        out.push('\n');
        if entry.path().is_dir() {
            let new_prefix = if last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            structure_recurse(&entry.path(), &new_prefix, max_depth, cur + 1, ignore, out);
        }
    }
}

fn should_show(entry: &std::fs::DirEntry, ignore: &[String]) -> bool {
    let name = entry.file_name().to_string_lossy().into_owned();
    if name.starts_with('.') {
        return false;
    }
    if crate::common::is_ignored_dir(&name) {
        return false;
    }
    if ignore
        .iter()
        .any(|p| crate::common::matches_gitignore_pub(p, &name))
    {
        return false;
    }
    true
}

/// 收集所有支持的源代码文件
fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    crate::common::walk_clean(root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .collect()
}

fn compute_stats(root: &Path, files: &[PathBuf]) -> serde_json::Value {
    let mut total_loc = 0usize;
    let mut total_bytes = 0usize;
    let mut top_files: Vec<(String, usize)> = Vec::new();
    for f in files {
        if let Ok(content) = std::fs::read_to_string(f) {
            let loc = content.lines().count();
            let bytes = content.len();
            total_loc += loc;
            total_bytes += bytes;
            let rel = rel_path(root, f);
            top_files.push((rel, loc));
        }
    }
    top_files.sort_by(|a, b| b.1.cmp(&a.1));
    top_files.truncate(10);
    let top = top_files
        .into_iter()
        .map(|(p, loc)| json!({"file": p, "loc": loc}))
        .collect::<Vec<_>>();
    json!({
        "files": files.len(),
        "loc": total_loc,
        "bytes": total_bytes,
        "top_files": top,
    })
}

fn extract_all_symbols(root: &Path, files: &[PathBuf]) -> Vec<serde_json::Value> {
    let mut all = Vec::new();
    for f in files {
        if let Ok(content) = std::fs::read_to_string(f) {
            if let Some(syms) = crate::langs::extract_symbols(f, &content) {
                let rel = rel_path(root, f);
                for s in syms {
                    all.push(json!({
                        "file": rel,
                        "kind": s.kind,
                        "name": s.name,
                        "line": s.line,
                    }));
                }
            }
        }
    }
    all
}

/// 最近修改的文件 top 10(hotspots)
fn compute_hotspots(root: &Path, files: &[PathBuf]) -> Vec<serde_json::Value> {
    let mut stamped: Vec<(PathBuf, u64)> = files
        .iter()
        .filter_map(|f| {
            let mt = std::fs::metadata(f).ok()?.modified().ok()?;
            let secs = mt.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some((f.clone(), secs))
        })
        .collect();
    stamped.sort_by(|a, b| b.1.cmp(&a.1));
    stamped.truncate(10);
    stamped
        .into_iter()
        .map(|(f, secs)| json!({"file": rel_path(root, &f), "mtime": secs}))
        .collect()
}

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.display().to_string())
        .unwrap_or_else(|_| p.display().to_string())
}

// ============================== 缓存引擎 ==============================

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Cache {
    report: serde_json::Value,
    head: Option<String>,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(".rxt-cache").join("map.json")
}

fn load_cache(path: &Path) -> Option<Cache> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_cache(path: &Path, cache: &Cache) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cache)?)?;
    Ok(())
}

fn cached_head_matches(cached: &Cache, current: Option<&str>) -> bool {
    match (&cached.head, current) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// 增量更新: 只重算变更文件的符号
fn incremental_update(root: &Path, mut cached: Cache, _depth: usize) -> Option<serde_json::Value> {
    let changed = crate::git::changed_files_since_head();
    if changed.is_empty() {
        return None; // 没有变更, 走全量
    }
    // 重建 symbols: 保留未变更文件, 重算变更文件
    let all_files = collect_source_files(root);
    let changed_set: std::collections::HashSet<String> = changed.iter().cloned().collect();
    let mut new_symbols: Vec<serde_json::Value> = Vec::new();
    for f in &all_files {
        let rel = rel_path(root, f);
        let is_changed = changed_set.contains(&rel) || changed.iter().any(|c| rel.ends_with(c));
        if is_changed {
            // 重算这个文件
            if let Ok(content) = std::fs::read_to_string(f) {
                if let Some(syms) = crate::langs::extract_symbols(f, &content) {
                    for s in syms {
                        new_symbols.push(
                            json!({"file": rel, "kind": s.kind, "name": s.name, "line": s.line}),
                        );
                    }
                }
            }
        } else {
            // 复用缓存的符号
            if let Some(arr) = cached.report.get("symbols").and_then(|v| v.as_array()) {
                for s in arr {
                    if s.get("file").and_then(|f| f.as_str()) == Some(&rel) {
                        new_symbols.push(s.clone());
                    }
                }
            }
        }
    }
    // 更新 stats 和 hotspots(便宜, 直接全算)
    let stats = compute_stats(root, &all_files);
    let hotspots = compute_hotspots(root, &all_files);
    if let Some(obj) = cached.report.as_object_mut() {
        obj.insert("symbols".into(), json!(new_symbols));
        obj.insert("stats".into(), stats);
        obj.insert("hotspots".into(), json!(hotspots));
        obj.insert("vcs".into(), vcs_info());
    }
    Some(cached.report)
}

// ============================== 输出 ==============================

fn output(report: &serde_json::Value, json_output: bool, cached: bool) -> anyhow::Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        print_text_report(report, cached);
    }
    Ok(())
}

fn print_text_report(report: &serde_json::Value, cached: bool) {
    let g = |k: &str| report.get(k);
    println!("╔══ rxt map ═════════════════════════════════════════════════╗");
    println!(
        "║ {} {}{}",
        g("kind").and_then(|v| v.as_str()).unwrap_or("?"),
        g("name").and_then(|v| v.as_str()).unwrap_or(""),
        {
            let v = g("project_version").and_then(|v| v.as_str()).unwrap_or("");
            if v.is_empty() {
                String::new()
            } else {
                format!(" v{}", v)
            }
        },
    );
    if let Some(vcs) = g("vcs") {
        println!(
            "║ {}{}{}",
            vcs.get("branch").and_then(|v| v.as_str()).unwrap_or("-"),
            vcs.get("head")
                .and_then(|v| v.as_str())
                .map(|h| format!(" @{}", h))
                .unwrap_or_default(),
            if vcs.get("dirty").and_then(|v| v.as_bool()).unwrap_or(false) {
                " *"
            } else {
                ""
            },
        );
    }
    if cached {
        println!("║ (cached, HEAD unchanged)");
    }
    println!("╚════════════════════════════════════════════════════════════╝");

    if let Some(stats) = g("stats") {
        println!(
            "\n📊 Stats: {} files, {} LOC",
            stats.get("files").and_then(|v| v.as_u64()).unwrap_or(0),
            stats.get("loc").and_then(|v| v.as_u64()).unwrap_or(0),
        );
    }

    if let Some(structure) = g("structure").and_then(|v| v.as_str()) {
        println!("\n🗂  Structure:");
        for line in structure.lines().take(30) {
            println!("  {}", line);
        }
    }

    if let Some(syms) = g("symbols").and_then(|v| v.as_array()) {
        let count = syms.len();
        println!("\n🔤 Symbols: {} total", count);
        // 按 kind 分类统计
        let mut by_kind: HashMap<String, usize> = HashMap::new();
        for s in syms {
            if let Some(k) = s.get("kind").and_then(|v| v.as_str()) {
                *by_kind.entry(k.to_string()).or_default() += 1;
            }
        }
        let mut sorted: Vec<_> = by_kind.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        let summary = sorted
            .iter()
            .map(|(k, c)| format!("{}({})", k, c))
            .collect::<Vec<_>>()
            .join(", ");
        println!("   {}", summary);
    }

    if let Some(hot) = g("hotspots").and_then(|v| v.as_array()) {
        if !hot.is_empty() {
            println!("\n🔥 Hotspots (recently changed):");
            for h in hot.iter().take(5) {
                println!(
                    "   {}",
                    h.get("file").and_then(|v| v.as_str()).unwrap_or("?")
                );
            }
        }
    }
    println!("");
}
