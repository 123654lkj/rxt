use std::path::Path;
use std::fs;
use std::collections::BTreeMap;

/// 代码结构分析 — 列出 fn/struct/enum/impl/mod
pub fn run(path: &Path, only_functions: bool, only_types: bool, deep: bool, extract: Option<&str>) -> anyhow::Result<()> {
    if path.is_dir() {
        return if deep { analyze_dir_deep(path, only_functions, only_types, extract) }
               else { analyze_dir(path, only_functions, only_types, extract) };
    }
    analyze_file(path, only_functions, only_types, extract)
}

fn analyze_dir(dir: &Path, only_functions: bool, only_types: bool, extract: Option<&str>) -> anyhow::Result<()> {
    println!("Directory: {}", dir.display());
    if let Ok(entries) = fs::read_dir(dir) {
        let mut files: Vec<_> = entries.flatten().filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("rs")).collect();
        files.sort_by_key(|e| e.file_name());
        for entry in &files {
            if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) { println!("\n\u{2500}\u{2500} {} \u{2500}\u{2500}", name); }
            analyze_file(&entry.path(), only_functions, only_types, extract.clone())?;
        }
    }
    Ok(())
}

fn analyze_dir_deep(dir: &Path, only_functions: bool, only_types: bool, extract: Option<&str>) -> anyhow::Result<()> {
    let mut all: Vec<(String, String, Vec<String>)> = Vec::new();
    fn walk(dir: &Path, base: &str, all: &mut Vec<(String, String, Vec<String>)>, only_fn: bool, only_ty: bool, extract: &Option<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "target" {
                        let sub = if base.is_empty() { name.to_string() } else { format!("{}/{}", base, name) };
                        walk(&p, &sub, all, only_fn, only_ty, extract);
                    }
                } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        let items = extract_items(&content, only_fn, only_ty);
                        if !items.is_empty() {
                            let display = if base.is_empty() { p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string() }
                                          else { format!("{}/{}", base, p.file_name().and_then(|n| n.to_str()).unwrap_or("")) };
                            all.push((display, content, items));
                        }
                    }
                }
            }
        }
    }
    walk(dir, "", &mut all, only_functions, only_types, &extract.map(|s| s.to_string()));
    all.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, content, items) in &all {
        println!("\n\u{2500}\u{2500} {} \u{2500}\u{2500}", name);
        for item in items { println!("  {}", item); }
        if let Some(ref target) = extract {
            if items.iter().any(|i| i.to_lowercase().contains(&target.to_lowercase())) {
                println!("\n--- Extracted: {} ---\n{}", target, highlight_item(content, target));
            }
        }
    }
    Ok(())
}

fn analyze_file(path: &Path, only_functions: bool, only_types: bool, extract: Option<&str>) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?;
    let items = extract_items(&content, only_functions, only_types);
    println!("File: {}", path.display());
    if items.is_empty() { println!("  (no items found)"); return Ok(()); }
    let mut by_category: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in items {
        let cat = if item.starts_with("fn ") { "Functions" } else if item.starts_with("struct ") { "Structs" }
            else if item.starts_with("enum ") { "Enums" } else if item.starts_with("impl") { "Impls" }
            else if item.starts_with("mod ") { "Modules" } else if item.starts_with("trait ") { "Traits" }
            else if item.starts_with("type ") { "Type Aliases" } else if item.starts_with("pub use") || item.starts_with("use ") { "Imports" }
            else if item.starts_with("const ") || item.starts_with("static ") { "Constants" } else { "Other" };
        by_category.entry(cat.to_string()).or_default().push(item);
    }
    for (cat, items) in &by_category {
        println!("  {} ({}):", cat, items.len());
        for item in items { println!("    {}", item); }
    }
    if let Some(ref target) = extract {
        if by_category.values().flatten().any(|i| i.to_lowercase().contains(&target.to_lowercase())) {
            println!("\n--- Extracted: {} ---\n{}", target, highlight_item(&content, target));
        }
    }
    Ok(())
}

fn extract_items(content: &str, only_functions: bool, only_types: bool) -> Vec<String> {
    let mut items = Vec::new();
    let mut in_block = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("//") { continue; }
        if t.starts_with("/*") { in_block = true; continue; }
        if in_block { if t.contains("*/") { in_block = false; } continue; }
        let item = if (t.starts_with("pub fn ") || t.starts_with("fn ")) && !only_types { extract_sig(t, "fn") }
            else if (t.starts_with("pub struct ") || t.starts_with("struct ")) && !only_functions { extract_sig(t, "struct") }
            else if (t.starts_with("pub enum ") || t.starts_with("enum ")) && !only_functions { extract_sig(t, "enum") }
            else if (t.starts_with("pub impl") || t.starts_with("impl ")) && !only_functions && !only_types { Some(impl_sig(t)) }
            else if (t.starts_with("pub trait ") || t.starts_with("trait ")) && !only_functions { extract_sig(t, "trait") }
            else if (t.starts_with("pub mod ") || t.starts_with("mod ")) && !only_functions && !only_types { extract_sig(t, "mod") }
            else if (t.starts_with("pub type ") || t.starts_with("type ")) && !only_functions { extract_sig(t, "type") }
            else if (t.starts_with("pub use ") || t.starts_with("use ")) && !only_functions && !only_types { Some(t.to_string()) }
            else if (t.starts_with("pub const ") || t.starts_with("const ") || t.starts_with("pub static ") || t.starts_with("static ")) && !only_functions { extract_sig(t, "const") }
            else { None };
        if let Some(i) = item { if !i.is_empty() { items.push(i); } }
    }
    items
}

fn extract_sig(line: &str, kw: &str) -> Option<String> {
    let idx = line.find(kw)?;
    let rest = &line[idx..];
    let end = rest.find(|c: char| c == '{' || c == ';').unwrap_or_else(|| rest.len().min(80));
    let sig = rest[..end].trim().to_string();
    if sig.len() > 3 { Some(sig) } else { None }
}

fn impl_sig(line: &str) -> String {
    let end = line.find('{').unwrap_or(line.len().min(80));
    let sig = line[..end].trim().to_string();
    if sig.len() > 4 { sig } else { line.to_string() }
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
            if (t.starts_with("fn ") || t.starts_with("struct ") || t.starts_with("enum ") ||
                t.starts_with("impl") || t.starts_with("trait ") || t.starts_with("mod ") ||
                t.starts_with("pub fn ") || t.starts_with("pub struct ") || t.starts_with("pub enum ") ||
                t.starts_with("pub trait ") || t.starts_with("pub mod ")) && lower.contains(&name_lower) {
                in_item = true; depth = 0;
            }
        }
        if in_item {
            result.push_str(line); result.push('\n');
            for ch in t.chars() { match ch { '{' => depth += 1, '}' => depth -= 1, _ => {} } }
            if depth <= 0 && t.ends_with('}') { break; }
            if t.ends_with(';') && depth == 0 { break; }
        }
    }
    result
}
