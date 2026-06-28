//! AI 上下文生成器
//! 一次调用,输出 AI 理解代码所需的一切:
//! - 文件指纹 (编码/换行符/BOM/缩进)
//! - 函数/类型签名列表
//! - 完整内容(可限制行数)
//! - 依赖关系 (imports)

use std::path::Path;
use std::fs;
use crate::signature::{FileSignature, to_utf8_lf};

pub fn run(path: &Path, max_lines: Option<usize>, json_output: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    let raw = if let Some(r) = remote {
        // 远程: 只读 max_lines*2 行 (需要 head+tail), 避免读整个大文件
        let limit = max_lines.map(|m| m * 2 + 100).unwrap_or(10000);
        r.exec(&format!("head -n {} {}", limit, path.display()))?.into_bytes()
    } else {
        fs::read(path)?
    };
    let sig = FileSignature::detect(&raw);
    let text = to_utf8_lf(&raw, &sig);
    let all_lines: Vec<&str> = text.lines().collect();
    let total = all_lines.len();
    
    // 限制行数
    let display_lines: Vec<&str> = if let Some(max) = max_lines {
        if total > max {
            let head = max / 2;
            let tail = max - head;
            let mut v: Vec<&str> = all_lines.iter().take(head).copied().collect();
            v.push("... [truncated] ...");
            v.extend(all_lines.iter().skip(total - tail).copied());
            v
        } else {
            all_lines.clone()
        }
    } else {
        all_lines.clone()
    };
    
    // 提取签名(简单版)
    let mut signatures: Vec<&str> = Vec::new();
    for line in &all_lines {
        let t = line.trim();
        if t.starts_with("pub fn ") || t.starts_with("fn ") ||
           t.starts_with("pub struct ") || t.starts_with("struct ") ||
           t.starts_with("pub enum ") || t.starts_with("enum ") ||
           t.starts_with("pub trait ") || t.starts_with("trait ") {
            signatures.push(line);
        }
    }
    
    // 提取 imports
    let mut imports: Vec<&str> = Vec::new();
    for line in &all_lines {
        let t = line.trim();
        if t.starts_with("use ") || t.starts_with("pub use ") {
            imports.push(line);
        }
    }
    
    if json_output {
        let json = serde_json::json!({
            "path": path.display().to_string(),
            "fingerprint": {
                "encoding": sig.encoding.to_string(),
                "line_ending": sig.line_ending.to_string(),
                "bom": sig.has_bom,
                "indent": sig.indent.to_string(),
                "lines": sig.lines,
                "bytes": sig.bytes
            },
            "total_lines": total,
            "truncated": max_lines.map_or(false, |m| total > m),
            "hint": if max_lines.map_or(false, |m| total > m) { Some("Use rxt read --lines for ranges, or rxt grep for sections") } else { None },
            "hint": if max_lines.map_or(false, |m| total > m) { Some("Use rxt read --lines for ranges, or rxt grep for focused sections") } else { None },
            "signatures": signatures,
            "imports": imports,
            "content": display_lines,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("Path:        {}", path.display());
        println!("Fingerprint: {} {} bom={} indent={}", sig.encoding, sig.line_ending, sig.has_bom, sig.indent);
        println!("Lines:       {} ({} bytes)", total, sig.bytes);
        let truncated = max_lines.map_or(false, |m| total > m);
        println!("Truncated:   {}", truncated);
        if truncated {
            eprintln!("
💡 Tip: file is large. Use  with bigger limit,
or  to read specific ranges,
or  to focus on relevant sections.");
        }
        println!();
        if !imports.is_empty() {
            println!("=== Imports ===");
            for imp in &imports { println!("{}", imp); }
            println!();
        }
        if !signatures.is_empty() {
            println!("=== Signatures ({}) ===", signatures.len());
            for sig in &signatures { println!("{}", sig); }
            println!();
        }
        println!("=== Content ===");
        for line in &display_lines {
            println!("{}", line);
        }
    }
    Ok(())
}
