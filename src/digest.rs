//! rxt digest — 文件骨架视图
//!
//! 输出文件的符号骨架, 函数体折叠成 {folded, N lines},
//! 让 AI 用 15 行看清 1000 行文件的结构, 省 70% token.
//! 
//! 用 langs 提取符号, 用 {} 括号深度配对计算函数体大小.

use std::path::{Path, PathBuf};
use serde_json::json;

/// digest 命令入口
///
/// - path: 单个源文件 **或目录**(v0.7: 目录模式一次输出整目录符号骨架)
/// - threshold: 函数体超过 N 行才折叠(默认 8)
/// - budget: token 预算, 超了截断(目录模式下限制每个文件展示的符号数)
/// - json_output: JSON 输出
pub fn run(path: &Path, threshold: usize, budget: Option<usize>, json_output: bool) -> anyhow::Result<()> {
    // v0.7: 目录模式 — 一次 digest 整个模块, 灵感来自 headroom 的整目录 AST 压缩.
    // AI 第一次进大型代码库时, 一条命令拿到结构地图, 省 70%+ token.
    if path.is_dir() {
        return run_dir(path, threshold, budget, json_output);
    }
    let content = std::fs::read_to_string(path)?;
    let symbols = match crate::langs::extract_symbols(path, &content) {
        Some(s) => s,
        None => {
            eprintln!("unsupported file type: {}", path.display());
            return Ok(());
        }
    };
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    // 计算每个符号的函数体行数
    let entries: Vec<DigestEntry> = symbols.iter().map(|s| {
        let body_lines = count_body(&lines, s.line, &s.kind);
        let folded = body_lines > threshold;
        DigestEntry {
            kind: s.kind.clone(),
            name: s.name.clone(),
            signature: s.signature.clone(),
            line: s.line,
            body_lines,
            folded,
        }
    }).collect();

    if json_output {
        output_json(path, total, &entries, budget);
    } else {
        output_text(path, total, &entries, budget);
    }
    Ok(())
}

struct DigestEntry {
    kind: String,
    name: String,
    signature: String,
    line: usize,
    body_lines: usize,
    folded: bool,
}

/// 从符号定义行开始, 用 {} 深度配对计算函数体行数
/// Python 用缩进(无 {}), 特殊处理
pub fn count_body(lines: &[&str], start_line: usize, kind: &str) -> usize {
    if start_line == 0 || start_line > lines.len() { return 0; }
    let start_idx = start_line - 1;  // 转 0-indexed

    // Python: 没有大括号, 用缩进判断函数体结束
    if kind == "def" || kind == "class" {
        return count_body_python(lines, start_idx);
    }

    // {} 语言(Rust/JS/Go): 深度配对
    let mut depth = 0i32;
    let mut found_open = false;
    for (i, line) in lines.iter().enumerate() {
        if i < start_idx { continue; }
        let t = line.trim();
        for ch in t.chars() {
            match ch {
                '{' => { depth += 1; found_open = true; }
                '}' => { depth -= 1; }
                _ => {}
            }
        }
        if found_open && depth <= 0 {
            return i - start_idx + 1;
        }
        // 单行定义 (const x = ...; )
        if !found_open && t.ends_with(';') {
            return i - start_idx + 1;
        }
    }
    lines.len() - start_idx  // 兜底: 到文件末尾
}

/// Python 函数体: 找到下一行缩进 <= 定义行缩进的位置
fn count_body_python(lines: &[&str], start_idx: usize) -> usize {
    let def_indent = indent_of(lines[start_idx]);
    for (i, line) in lines.iter().enumerate() {
        if i <= start_idx { continue; }
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        let cur_indent = indent_of(line);
        // 遇到缩进 <= def 缩进的同级/外层语句, 函数体结束
        if cur_indent <= def_indent {
            return i - start_idx;
        }
    }
    lines.len() - start_idx
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

fn output_text(path: &Path, total: usize, entries: &[DigestEntry], budget: Option<usize>) {
    println!("{} ({} lines)", path.display(), total);
    println!("");
    let mut shown: Vec<&DigestEntry> = entries.iter().collect();
    // token 预算: 限制展示条数(粗略, 每条约 15 token)
    if let Some(b) = budget {
        let max_items = b.max(3) / 15;
        shown.truncate(max_items);
    }
    for e in &shown {
        if e.folded {
            println!("  {:>5}  {} {{{} — folded, {} lines}}", e.line, e.signature.trim_end_matches('{').trim_end(), e.name, e.body_lines);
        } else {
            println!("  {:>5}  {}", e.line, e.signature);
        }
    }
    if entries.len() > shown.len() {
        eprintln!("\n(truncated: {}/{} symbols shown — increase --budget)", shown.len(), entries.len());
    }
}

fn output_json(path: &Path, total: usize, entries: &[DigestEntry], budget: Option<usize>) {
    let mut shown: Vec<&DigestEntry> = entries.iter().collect();
    if let Some(b) = budget {
        let max_items = b.max(3) / 15;
        shown.truncate(max_items);
    }
    let arr: Vec<serde_json::Value> = shown.iter().map(|e| {
        json!({
            "kind": e.kind,
            "name": e.name,
            "signature": e.signature,
            "line": e.line,
            "body_lines": e.body_lines,
            "folded": e.folded,
        })
    }).collect();
    let out = json!({
        "file": path.display().to_string(),
        "total_lines": total,
        "symbols_total": entries.len(),
        "symbols_shown": shown.len(),
        "digest": arr,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

// ============================== v0.7: 目录模式 ==============================

/// 目录模式: 遍历目录下所有支持的源文件, 每个文件输出折叠后的符号骨架.
///
/// 设计:
///   - 复用 langs::extract_symbols + count_body, 逻辑和单文件一致
///   - 每文件只展示签名(不展示函数体), 按文件分组
///   - budget 在目录模式下 = 每个文件最多展示多少符号(避免大文件淹没输出)
///   - 默认按文件路径排序, 稳定可读
fn run_dir(root: &Path, threshold: usize, budget: Option<usize>, json_output: bool) -> anyhow::Result<()> {
    // 收集所有支持的源文件
    let mut files: Vec<PathBuf> = crate::common::walk_clean(root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .collect();
    files.sort();

    if files.is_empty() {
        if json_output {
            println!("{}", serde_json::to_string_pretty(&json!({
                "root": root.display().to_string(),
                "files": 0,
                "digest": [],
            }))?);
        } else {
            println!("目录 {} 下没有支持的源文件.", root.display());
        }
        return Ok(());
    }

    // budget 在目录模式: 每文件展示符号上限(默认 40, 够看结构又不太长)
    let per_file_limit = budget.unwrap_or(40);

    // 统计
    let mut total_symbols = 0usize;
    let mut total_shown = 0usize;
    let mut file_digests: Vec<serde_json::Value> = Vec::new();
    let mut text_out: Vec<(String, usize, Vec<DigestEntry>)> = Vec::new();

    for f in &files {
        let content = match std::fs::read_to_string(f) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let symbols = match crate::langs::extract_symbols(f, &content) {
            Some(s) => s,
            None => continue,
        };
        let lines: Vec<&str> = content.lines().collect();
        let file_total = lines.len();
        let mut entries: Vec<DigestEntry> = symbols.iter().map(|s| {
            let body_lines = count_body(&lines, s.line, &s.kind);
            DigestEntry {
                kind: s.kind.clone(),
                name: s.name.clone(),
                signature: s.signature.clone(),
                line: s.line,
                body_lines,
                folded: body_lines > threshold,
            }
        }).collect();
        total_symbols += entries.len();

        // 限制每文件展示数
        let shown_count = entries.len().min(per_file_limit);
        entries.truncate(shown_count);
        total_shown += shown_count;

        let rel = f.strip_prefix(root)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| f.display().to_string());

        if json_output {
            let arr: Vec<serde_json::Value> = entries.iter().map(|e| {
                json!({
                    "kind": e.kind, "name": e.name, "signature": e.signature,
                    "line": e.line, "body_lines": e.body_lines, "folded": e.folded,
                })
            }).collect();
            file_digests.push(json!({
                "file": rel,
                "total_lines": file_total,
                "symbols": arr,
            }));
        } else {
            text_out.push((rel, file_total, entries));
        }
    }

    if json_output {
        let out = json!({
            "root": root.display().to_string(),
            "files": files.len(),
            "symbols_total": total_symbols,
            "symbols_shown": total_shown,
            "digest": file_digests,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("digest {} — {} 个文件, {}/{} 符号", root.display(), files.len(), total_shown, total_symbols);
        println!();
        for (rel, file_total, entries) in &text_out {
            println!("── {} ({} lines) ──", rel, file_total);
            for e in entries {
                if e.folded {
                    println!("  {:>5}  {} {{{} — folded, {} lines}}", e.line,
                        e.signature.trim_end_matches('{').trim_end(), e.name, e.body_lines);
                } else {
                    println!("  {:>5}  {}", e.line, e.signature);
                }
            }
            println!();
        }
        if total_shown < total_symbols {
            println!("(部分符号被省略: {}/{} 显示, 用 --budget <N> 调高每文件上限)", total_shown, total_symbols);
        }
    }
    Ok(())
}