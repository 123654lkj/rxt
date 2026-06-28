//! rxt refs — 引用查找(谁调用谁)
//!
//! 扫描所有源文件, 找出某个符号的所有出现, 并语义分类:
//!   def  — 定义处(行首有 fn/def/function/func 等关键字)
//!   call — 调用/引用处(带上下文)
//!
//! 不依赖真 LSP, 文本启发式覆盖 80% 场景, 够 agent 顺着调用链走.

use std::path::PathBuf;
use std::path::Path;
use regex::Regex;
use serde_json::json;

/// refs 命令入口
/// 
/// - symbol: 要查找的符号名(精确匹配单词边界)
/// - root: 搜索根目录(默认当前目录)
/// - json_output: JSON 输出
pub fn run(symbol: &str, root: &Path, json_output: bool) -> anyhow::Result<()> {
    // 单词边界精确匹配(避免 run 匹配到 runtime)
    let pattern = format!(r"\b{}\b", regex::escape(symbol));
    let re = Regex::new(&pattern)?;

    // 定义关键字(各语言)
    let def_keywords = [
        "fn", "def", "function", "func",
        "struct", "class", "interface", "enum", "trait",
        "type", "const", "let", "var",
    ];

    // 收集所有源文件
    let files = collect_source_files(root);

    let mut refs: Vec<Ref> = Vec::new();
    for f in &files {
        if let Ok(content) = std::fs::read_to_string(f) {
            let rel = rel_path(root, f);
            for (idx, line) in content.lines().enumerate() {
                let line_num = idx + 1;
                if !re.is_match(line) { continue; }
                let t = line.trim_start();
                // 判断是否定义行: 行首(去 pub/async/export 等修饰)是 def 关键字
                let stripped = strip_modifiers(t);
                let first_word = stripped.split_whitespace().next().unwrap_or("");
                let is_def = def_keywords.contains(&first_word);
                refs.push(Ref {
                    file: rel.clone(),
                    line: line_num,
                    kind: if is_def { "def" } else { "call" },
                    ctx: line.trim().to_string(),
                });
            }
        }
    }

    if json_output {
        let arr: Vec<_> = refs.iter().map(|r| json!({
            "file": r.file, "line": r.line, "kind": r.kind, "ctx": r.ctx,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&json!(arr))?);
    } else {
        print_text(&refs, symbol);
    }
    Ok(())
}

struct Ref {
    file: String,
    line: usize,
    kind: &'static str,  // "def" / "call"
    ctx: String,
}

/// 去掉行首修饰词, 暴露出真正的关键字
fn strip_modifiers(line: &str) -> &str {
    let mut s = line;
    loop {
        let trimmed = s.trim_start();
        let next = trimmed.split_whitespace().next().unwrap_or("");
        if matches!(next, "pub" | "async" | "export" | "static" | "const" | "final" | "private" | "public" | "protected" | "abstract" | "virtual" | "override" | "unsafe" | "extern" | "crate") {
            // 跳过这个修饰词
            s = &trimmed[next.len()..];
        } else {
            return trimmed;
        }
    }
}

fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    crate::common::walk_clean(root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .collect()
}

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_else(|_| p.display().to_string())
}

fn print_text(refs: &[Ref], symbol: &str) {
    if refs.is_empty() {
        println!("No references to '{}' found.", symbol);
        return;
    }
    let defs: Vec<&Ref> = refs.iter().filter(|r| r.kind == "def").collect();
    let calls: Vec<&Ref> = refs.iter().filter(|r| r.kind == "call").collect();
    println!("refs '{}' — {} def, {} call", symbol, defs.len(), calls.len());
    println!();
    if !defs.is_empty() {
        println!("── Definitions ──");
        for r in &defs {
            println!("  {}:{}  {}", r.file, r.line, r.ctx);
        }
        println!();
    }
    if !calls.is_empty() {
        println!("── References ({}) ──", calls.len());
        for r in calls.iter().take(50) {
            println!("  {}:{}  {}", r.file, r.line, r.ctx);
        }
        if calls.len() > 50 {
            println!("  ... and {} more (use --json for all)", calls.len() - 50);
        }
    }
}