use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// 代码结构分析 — 列出 fn/struct/enum/impl/mod/trait/type/use/const
#[derive(Debug, Clone)]
struct StructItem {
    kind: String, // "fn" / "struct" / "enum" / "impl" / "trait" / "mod" / "type" / "use" / "const"
    name: String, // 函数名 / 结构名 / impl target 等
    signature: String, // 完整签名
    line: usize,  // 源码行号 (1-indexed)
}

pub fn run(
    path: &Path,
    only_functions: bool,
    only_types: bool,
    deep: bool,
    extract: Option<&str>,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        // JSON mode: collect all items across files, output single JSON array
        let stdout = io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        let mut all: Vec<serde_json::Value> = Vec::new();
        if path.is_dir() {
            collect_json_dir(path, only_functions, only_types, extract, deep, &mut all)?;
        } else {
            collect_json_file(path, only_functions, only_types, extract, &mut all)?;
        }
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(all))?
        )?;
        out.flush()?;
        return Ok(());
    }

    // Text mode: existing behavior unchanged
    if path.is_dir() {
        return if deep {
            analyze_dir_deep(path, only_functions, only_types, extract)
        } else {
            analyze_dir(path, only_functions, only_types, extract)
        };
    }
    analyze_file(path, only_functions, only_types, extract)
}

fn collect_json_file(
    path: &Path,
    only_fn: bool,
    only_ty: bool,
    extract: Option<&str>,
    out: &mut Vec<serde_json::Value>,
) -> anyhow::Result<()> {
    let items = extract_items_struct(path, only_fn, only_ty)?;
    push_items(out, path, &items, extract);
    Ok(())
}

fn collect_json_dir(
    dir: &Path,
    only_fn: bool,
    only_ty: bool,
    extract: Option<&str>,
    deep: bool,
    out: &mut Vec<serde_json::Value>,
) -> anyhow::Result<()> {
    if deep {
        walk_dir_deep(dir, "", only_fn, only_ty, extract, out)?;
    } else {
        let mut files: Vec<_> = fs::read_dir(dir)?
            .flatten()
            .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("rs"))
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in &files {
            collect_json_file(&entry.path(), only_fn, only_ty, extract, out)?;
        }
    }
    Ok(())
}

fn walk_dir_deep(
    dir: &Path,
    base: &str,
    only_fn: bool,
    only_ty: bool,
    extract: Option<&str>,
    out: &mut Vec<serde_json::Value>,
) -> anyhow::Result<()> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') && name != "target" {
                    let sub = if base.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}/{}", base, name)
                    };
                    walk_dir_deep(&p, &sub, only_fn, only_ty, extract, out)?;
                }
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .map(|x| {
                    crate::langparse::Lang::all_known_exts().contains(&x.to_lowercase().as_str())
                })
                .unwrap_or(false)
            {
                let items = extract_items_struct(&p, only_fn, only_ty)?;
                push_items(out, &p, &items, extract);
            }
        }
    }
    Ok(())
}

fn push_items(
    out: &mut Vec<serde_json::Value>,
    path: &Path,
    items: &[StructItem],
    extract: Option<&str>,
) {
    for item in items {
        if let Some(target) = extract {
            if !item.name.to_lowercase().contains(&target.to_lowercase()) {
                continue;
            }
        }
        out.push(serde_json::json!({
            "file": path.display().to_string(),
            "kind": item.kind,
            "name": item.name,
            "signature": item.signature,
            "line": item.line,
        }));
    }
}

fn extract_items_struct(
    path: &Path,
    only_functions: bool,
    only_types: bool,
) -> anyhow::Result<Vec<StructItem>> {
    let content = fs::read_to_string(path)?;
    // 按文件扩展名检测语言,走 langparse 多语言解析器
    let lang = crate::langparse::detect_lang(path);
    let items = if let Some(l) = lang {
        let mut ci = crate::langparse::parse(&content, l);
        ci = crate::langparse::filter(ci, only_functions, only_types);
        ci.into_iter()
            .map(|c| StructItem {
                kind: c.kind,
                name: c.name,
                signature: c.signature,
                line: c.line,
            })
            .collect()
    } else {
        // 未知语言: 兜底用 Rust 规则(保持向后兼容)
        extract_items_from_str(&content, only_functions, only_types)
    };
    Ok(items)
}

fn extract_items_from_str(
    content: &str,
    only_functions: bool,
    only_types: bool,
) -> Vec<StructItem> {
    let mut items = Vec::new();
    let mut in_block = false;
    for (idx, line) in content.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("/*") {
            in_block = true;
            continue;
        }
        if in_block {
            if t.contains("*/") {
                in_block = false;
            }
            continue;
        }
        let line_num = idx + 1;
        let item = if (t.starts_with("pub fn ") || t.starts_with("fn ")) && !only_types {
            parse_sig(t, "fn", line_num)
        } else if (t.starts_with("pub struct ") || t.starts_with("struct ")) && !only_functions {
            parse_sig(t, "struct", line_num)
        } else if (t.starts_with("pub enum ") || t.starts_with("enum ")) && !only_functions {
            parse_sig(t, "enum", line_num)
        } else if (t.starts_with("pub impl") || t.starts_with("impl "))
            && !only_functions
            && !only_types
        {
            Some(parse_impl(t, line_num))
        } else if (t.starts_with("pub trait ") || t.starts_with("trait ")) && !only_functions {
            parse_sig(t, "trait", line_num)
        } else if (t.starts_with("pub mod ") || t.starts_with("mod "))
            && !only_functions
            && !only_types
        {
            parse_sig(t, "mod", line_num)
        } else if (t.starts_with("pub type ") || t.starts_with("type ")) && !only_functions {
            parse_sig(t, "type", line_num)
        } else if (t.starts_with("pub use ") || t.starts_with("use "))
            && !only_functions
            && !only_types
        {
            Some(parse_simple(t.to_string(), "use", line_num))
        } else if (t.starts_with("pub const ")
            || t.starts_with("const ")
            || t.starts_with("pub static ")
            || t.starts_with("static "))
            && !only_functions
        {
            parse_sig(t, "const", line_num)
        } else {
            None
        };
        if let Some(i) = item {
            items.push(i);
        }
    }
    items
}

fn parse_sig(line: &str, kw: &str, line_num: usize) -> Option<StructItem> {
    let idx = line.find(kw)?;
    let rest = &line[idx..];
    let end = rest
        .find(|c: char| c == '{' || c == ';')
        .unwrap_or_else(|| rest.len().min(80));
    let sig = rest[..end].trim().to_string();
    if sig.len() <= 3 {
        return None;
    }
    let name = extract_name_from_sig(&sig, kw);
    Some(StructItem {
        kind: kw.to_string(),
        name,
        signature: sig,
        line: line_num,
    })
}

fn parse_impl(line: &str, line_num: usize) -> StructItem {
    let end = line.find('{').unwrap_or(line.len().min(80));
    let sig = line[..end].trim().to_string();
    let final_sig = if sig.len() > 4 { sig } else { line.to_string() };
    let name = extract_name_from_sig(&final_sig, "impl");
    StructItem {
        kind: "impl".to_string(),
        name,
        signature: final_sig,
        line: line_num,
    }
}

fn parse_simple(line: String, kw: &str, line_num: usize) -> StructItem {
    let name = extract_name_from_sig(&line, kw);
    StructItem {
        kind: kw.to_string(),
        name,
        signature: line,
        line: line_num,
    }
}

fn extract_name_from_sig(sig: &str, kw: &str) -> String {
    // After "fn " / "struct " / etc, find next identifier
    let after_kw = &sig[kw.len()..];
    let after_kw = after_kw.trim_start_matches("pub ").trim_start();
    let mut name = String::new();
    for c in after_kw.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            name.push(c);
        } else {
            break;
        }
    }
    if name.is_empty() {
        "<anonymous>".to_string()
    } else {
        name
    }
}

// =============== Text mode (existing behavior, unchanged) ===============

fn analyze_dir(
    dir: &Path,
    only_functions: bool,
    only_types: bool,
    extract: Option<&str>,
) -> anyhow::Result<()> {
    println!("Directory: {}", dir.display());
    if let Ok(entries) = fs::read_dir(dir) {
        let known_exts = crate::langparse::Lang::all_known_exts();
        let mut files: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| known_exts.contains(&x.to_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in &files {
            if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                println!("\n── {} ──", name);
            }
            analyze_file(&entry.path(), only_functions, only_types, extract)?;
        }
    }
    Ok(())
}

fn analyze_dir_deep(
    dir: &Path,
    only_functions: bool,
    only_types: bool,
    extract: Option<&str>,
) -> anyhow::Result<()> {
    let mut all: Vec<(String, String, Vec<String>)> = Vec::new();
    fn walk(
        dir: &Path,
        base: &str,
        all: &mut Vec<(String, String, Vec<String>)>,
        only_fn: bool,
        only_ty: bool,
        extract: &Option<String>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "target" {
                        let sub = if base.is_empty() {
                            name.to_string()
                        } else {
                            format!("{}/{}", base, name)
                        };
                        walk(&p, &sub, all, only_fn, only_ty, extract);
                    }
                } else if p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|x| {
                        crate::langparse::Lang::all_known_exts()
                            .contains(&x.to_lowercase().as_str())
                    })
                    .unwrap_or(false)
                {
                    if let Ok(content) = fs::read_to_string(&p) {
                        let items = extract_items_text(&content, only_fn, only_ty);
                        if !items.is_empty() {
                            let display = if base.is_empty() {
                                p.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                format!(
                                    "{}/{}",
                                    base,
                                    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
                                )
                            };
                            all.push((display, content, items));
                        }
                    }
                }
            }
        }
    }
    walk(
        dir,
        "",
        &mut all,
        only_functions,
        only_types,
        &extract.map(|s| s.to_string()),
    );
    all.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, content, items) in &all {
        println!("\n── {} ──", name);
        for item in items {
            println!("  {}", item);
        }
        if let Some(ref target) = extract {
            if items
                .iter()
                .any(|i| i.to_lowercase().contains(&target.to_lowercase()))
            {
                println!(
                    "\n--- Extracted: {} ---\n{}",
                    target,
                    highlight_item(content, target)
                );
            }
        }
    }
    Ok(())
}

fn analyze_file(
    path: &Path,
    only_functions: bool,
    only_types: bool,
    extract: Option<&str>,
) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?;
    // 走文件路径版(自动检测语言)
    let items = extract_items_struct(path, only_functions, only_types)?;
    let item_sigs: Vec<String> = items.iter().map(|i| i.signature.clone()).collect();
    println!("File: {}", path.display());
    if item_sigs.is_empty() {
        println!("  (no items found)");
        return Ok(());
    }
    // 按 kind 分类(用解析结果的 kind,不再硬编码 Rust 关键字)
    let mut by_category: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for it in &items {
        let cat = match it.kind.as_str() {
            "fn" | "function" | "method" | "async" | "getter" | "setter" => "Functions",
            "struct" | "record" => "Structs",
            "enum" => "Enums",
            "impl" => "Impls",
            "mod" | "module" => "Modules",
            "trait" | "interface" => {
                if it.kind == "trait" {
                    "Traits"
                } else {
                    "Interfaces"
                }
            }
            "type" => "Type Aliases",
            "import" | "use" => "Imports",
            "const" | "static" | "var" | "let" => "Constants",
            "class" => "Classes",
            _ => "Other",
        };
        by_category
            .entry(cat.to_string())
            .or_default()
            .push(it.signature.clone());
    }
    for (cat, items) in &by_category {
        println!("  {} ({}):", cat, items.len());
        for item in items {
            println!("    {}", item);
        }
    }
    if let Some(ref target) = extract {
        if by_category
            .values()
            .flatten()
            .any(|i| i.to_lowercase().contains(&target.to_lowercase()))
        {
            println!(
                "\n--- Extracted: {} ---\n{}",
                target,
                highlight_item(&content, target)
            );
        }
    }
    Ok(())
}

fn extract_items_text(content: &str, only_functions: bool, only_types: bool) -> Vec<String> {
    let items = extract_items_from_str(content, only_functions, only_types);
    items.into_iter().map(|i| i.signature).collect()
}

fn highlight_item(content: &str, name: &str) -> String {
    let name_lower = name.to_lowercase();
    let mut result = String::new();
    let mut in_item = false;
    let mut depth = 0i32;
    for line in content.lines() {
        let t = line.trim();
        if !in_item {
            let lower = t.to_lowercase();
            if (t.starts_with("fn ")
                || t.starts_with("struct ")
                || t.starts_with("enum ")
                || t.starts_with("impl")
                || t.starts_with("trait ")
                || t.starts_with("mod ")
                || t.starts_with("pub fn ")
                || t.starts_with("pub struct ")
                || t.starts_with("pub enum ")
                || t.starts_with("pub trait ")
                || t.starts_with("pub mod "))
                && lower.contains(&name_lower)
            {
                in_item = true;
                depth = 0;
            }
        }
        if in_item {
            result.push_str(line);
            result.push('\n');
            for ch in t.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if depth <= 0 && t.ends_with('}') {
                break;
            }
            if t.ends_with(';') && depth == 0 {
                break;
            }
        }
    }
    result
}
