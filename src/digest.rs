//! rxt digest — 文件骨架视图
//!
//! 输出文件的符号骨架, 函数体折叠成 {folded, N lines},
//! 让 AI 用 15 行看清 1000 行文件的结构, 省 70% token.
//! 
//! 用 langs 提取符号, 用 {} 括号深度配对计算函数体大小.

use std::path::Path;
use serde_json::json;

/// digest 命令入口
/// 
/// - path: 单个源文件
/// - threshold: 函数体超过 N 行才折叠(默认 8)
/// - budget: token 预算, 超了截断
/// - json_output: JSON 输出
pub fn run(path: &Path, threshold: usize, budget: Option<usize>, json_output: bool) -> anyhow::Result<()> {
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
fn count_body(lines: &[&str], start_line: usize, kind: &str) -> usize {
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