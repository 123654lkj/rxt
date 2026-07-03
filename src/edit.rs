//! 结构化文件编辑 — 格式保持版
//! 支持 JSON 脚本多步变换 + 行模式编辑

use std::path::Path;
use std::fs;
use serde::Deserialize;

use crate::signature::{FileSignature, to_utf8_lf, apply_format};
use regex::Regex;

#[derive(Debug, Deserialize)]
struct ScriptOp {
    op: String,
    old: Option<String>,
    new: Option<String>,
    #[serde(alias = "match")]
    match_: Option<String>,
    content: Option<String>,
}

/// 从 JSON 脚本文件执行多步变换
pub fn run_script(path: &Path, script_path: &Path, preview: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    // 读取原始文件指纹
    let storage = crate::storage::Storage::from_remote(remote);
    let raw = storage.read_file(path)?;
    let sig = FileSignature::detect(&raw);
    
    // 转为内部 UTF-8 + LF
    let mut file_content = to_utf8_lf(&raw, &sig);

    // 读取脚本
    let script_raw = fs::read(script_path)?;
    let script_text = if script_raw.len() >= 3 && script_raw[0] == 0xEF && script_raw[1] == 0xBB && script_raw[2] == 0xBF {
        String::from_utf8_lossy(&script_raw[3..]).to_string()
    } else {
        String::from_utf8_lossy(&script_raw).to_string()
    };
    let ops: Vec<ScriptOp> = serde_json::from_str(&script_text)?;
    let original = file_content.clone();

    for (i, op) in ops.iter().enumerate() {
        let before = file_content.clone();
        match op.op.as_str() {
            "replace" => {
                let old = op.old.as_deref().unwrap_or("");
                let new = op.new.as_deref().unwrap_or("");
                file_content = file_content.replace(old, new);
            }
            "insert_after" => {
                let match_str = op.match_.as_deref().unwrap_or("");
                let text = op.content.as_deref().unwrap_or("");
                if !match_str.is_empty() && !text.is_empty() {
                    let lines: Vec<&str> = file_content.lines().collect();
                    let mut new_lines: Vec<String> = Vec::new();
                    for &line in &lines {
                        new_lines.push(line.to_string());
                        if line.contains(match_str) {
                            for tline in text.split('\n') {
                                new_lines.push(tline.to_string());
                            }
                        }
                    }
                    file_content = new_lines.join("\n");
                }
            }
            "insert_before" => {
                let match_str = op.match_.as_deref().unwrap_or("");
                let text = op.content.as_deref().unwrap_or("");
                if !match_str.is_empty() && !text.is_empty() {
                    let lines: Vec<&str> = file_content.lines().collect();
                    let mut new_lines: Vec<String> = Vec::new();
                    for &line in &lines {
                        if line.contains(match_str) {
                            for tline in text.split('\n') {
                                new_lines.push(tline.to_string());
                            }
                        }
                        new_lines.push(line.to_string());
                    }
                    file_content = new_lines.join("\n");
                }
            }
            "delete" => {
                let match_str = op.match_.as_deref().unwrap_or("");
                let lines: Vec<&str> = file_content.lines().collect();
                let mut new_lines: Vec<String> = Vec::new();
                for line in lines {
                    if !line.contains(match_str) {
                        new_lines.push(line.to_string());
                    }
                }
                file_content = new_lines.join("\n");
            }
            other => {
                eprintln!("  Warning: unknown op '{}' in script step {}", other, i + 1);
            }
        }
        if preview && file_content != before {
            println!("  Step {} ({}) changed", i + 1, op.op);
        }
    }

    let changed = file_content != original;
    if changed {
        if preview {
            println!("  Preview: file would be updated");
        } else {
            // 应用原始格式写回
            let formatted = apply_format(&file_content, &sig);
            storage.write_file(path, formatted.as_bytes())?;
            println!("  Updated {} ({} script ops, preserved: {} {})", 
                     path.display(), ops.len(), sig.encoding, sig.line_ending);
        }
    } else {
        eprintln!("  (no changes made)");
    }
    Ok(())
}

/// 结构化文件编辑 — 按内容模式插入/删除/替换行
pub fn run(
    path: &Path,
    after: Option<&str>,
    before: Option<&str>,
    delete: Option<&str>,
    replace: Option<(&str, &str)>,
    content: &[String],
    preview: bool,
    use_regex: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    let re_after = match (use_regex, after) {
        (true, Some(a)) => match Regex::new(a) {
            Ok(r) => Some(r),
            Err(e) => { eprintln!("Warning: invalid regex in --after: {}. Falling back to literal.", e); None },
        },
        _ => None,
    };
    let re_before = match (use_regex, before) {
        (true, Some(b)) => match Regex::new(b) {
            Ok(r) => Some(r),
            Err(e) => { eprintln!("Warning: invalid regex in --before: {}. Falling back to literal.", e); None },
        },
        _ => None,
    };
    let re_delete = match (use_regex, delete) {
        (true, Some(d)) => match Regex::new(d) {
            Ok(r) => Some(r),
            Err(e) => { eprintln!("Warning: invalid regex in --delete: {}. Falling back to literal.", e); None },
        },
        _ => None,
    };
    // 读取原始文件指纹
    let storage = crate::storage::Storage::from_remote(remote);
    let raw = storage.read_file(path)?;
    let sig = FileSignature::detect(&raw);
    
    // 转为内部 UTF-8 + LF
    let file_content = to_utf8_lf(&raw, &sig);
    let lines: Vec<&str> = file_content.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut changed = false;

    let insert_text: String = content.join("\n");

    for &line in &lines {
        // Delete mode
        if let Some(pat) = delete {
            let m = if let Some(ref re) = re_delete { re.is_match(line) } else { line.contains(pat) };
            if m { changed = true; continue; }
        }

        // Insert before mode
        if let Some(pat) = before {
            let m = if let Some(ref re) = re_before { re.is_match(line) } else { line.contains(pat) };
            if m && !insert_text.is_empty() { result.push(insert_text.clone()); changed = true; }
        }

        // Replace mode
        if let Some((old, new)) = replace {
            if line.contains(old) {
                result.push(line.replace(old, new));
                changed = true;
                if !insert_text.is_empty() && after.is_none() && before.is_none() {
                    result.push(insert_text.clone());
                }
                continue;
            }
        }

        // Add current line
        result.push(line.to_string());

        // Insert after mode
        if let Some(pat) = after {
            let m = if let Some(ref re) = re_after { re.is_match(line) } else { line.contains(pat) };
            if m && !insert_text.is_empty() { result.push(insert_text.clone()); changed = true; }
        }
    }

    let output = result.join("\n");
    if output != file_content { changed = true; }

    if changed {
        if preview {
            let old_l: Vec<&str> = file_content.lines().collect();
            let new_l: Vec<&str> = output.lines().collect();
            let max = old_l.len().max(new_l.len());
            for i in 0..max {
                let a = old_l.get(i).copied().unwrap_or("");
                let b = new_l.get(i).copied().unwrap_or("");
                if a != b {
                    if !a.is_empty() { println!("- {}", a); }
                    if !b.is_empty() { println!("+ {}", b); }
                }
            }
        } else {
            // 应用原始格式写回
            let final_out = if file_content.ends_with('\n') {
                format!("{}\n", output.trim_end())
            } else {
                output
            };
            let formatted = apply_format(&final_out, &sig);
            storage.write_file(path, formatted.as_bytes())?;
            println!("  Updated {} (preserved: {} {})", path.display(), sig.encoding, sig.line_ending);
        }
    } else {
        eprintln!("  (no changes made)");
    }
    Ok(())
}


/// 行范围替换 — 精确改指定行
pub fn run_line_range(
    path: &Path,
    range_spec: &str,
    content: &[String],
    preview: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    let storage = crate::storage::Storage::from_remote(remote);
    let raw = storage.read_file(path)?;
    let sig = FileSignature::detect(&raw);
    let text = to_utf8_lf(&raw, &sig);
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    
    let (start, end) = if let Some((s, e)) = range_spec.split_once('-') {
        let s: usize = s.trim().parse().unwrap_or(1).max(1);
        let e: usize = e.trim().parse().unwrap_or(total);
        (s, e.min(total))
    } else {
        let n: usize = range_spec.trim().parse().unwrap_or(1);
        (n, n.min(total))
    };
    
    let new_text: String = content.join("
");
    let mut result_lines: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let ln = i + 1;
        if ln == start {
            result_lines.push(new_text.clone());
        } else if ln < start || ln > end {
            result_lines.push(line.to_string());
        }
    }
    
    let output = result_lines.join("
");
    if output == text {
        eprintln!("  (no changes made)");
        return Ok(());
    }
    
    if preview {
        println!("  Lines {}-{} replaced ({} lines → {} lines)", start, end, end - start + 1, content.len());
        for c in content { println!("+ {}", c); }
        return Ok(());
    }
    
    let formatted = apply_format(&output, &sig);
    storage.write_file(path, formatted.as_bytes())?;
    println!("  Updated lines {}-{} in {} (preserved: {} {})", start, end, path.display(), sig.encoding, sig.line_ending);
    Ok(())
}
