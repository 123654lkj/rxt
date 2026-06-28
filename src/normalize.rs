//! 文件格式统一 — 跨平台文本标准化
//! 将文件转换为指定格式（UTF-8 + LF/CRLF，可选去 BOM）

use std::path::Path;
use std::fs;

use crate::signature::{FileSignature, to_utf8_lf, apply_format, LineEnding, Encoding};

/// 统一文件格式
pub fn run(path: &Path, target_ending: Option<&str>, remove_bom: bool, json_output: bool) -> anyhow::Result<()> {
    let raw = fs::read(path)?;
    let sig = FileSignature::detect(&raw);
    
    // 转为内部 UTF-8 + LF
    let text = to_utf8_lf(&raw, &sig);
    
    // 创建目标格式
    let target_le = match target_ending {
        Some("crlf") | Some("windows") => LineEnding::CRLF,
        Some("lf") | Some("unix") | Some("linux") => LineEnding::LF,
        None => sig.line_ending, // 保持原有
        _ => anyhow::bail!("Unknown line ending: {}. Use: lf, crlf, unix, windows", target_ending.unwrap()),
    };
    
    let target_sig = FileSignature {
        encoding: Encoding::UTF8,
        line_ending: target_le,
        has_bom: if remove_bom { false } else { sig.has_bom },
        indent: sig.indent.clone(),
        lines: sig.lines,
        bytes: sig.bytes,
    };
    
    // 应用格式
    let formatted = apply_format(&text, &target_sig);
    
    if json_output {
        let json = serde_json::json!({
            "path": path.display().to_string(),
            "original": {
                "encoding": sig.encoding.to_string(),
                "line_ending": sig.line_ending.to_string(),
                "bom": sig.has_bom,
                "lines": sig.lines,
                "bytes": sig.bytes
            },
            "normalized": {
                "encoding": "UTF-8",
                "line_ending": target_le.to_string(),
                "bom": target_sig.has_bom,
                "bytes": formatted.len()
            },
            "changed": raw != formatted.as_bytes()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    
    // 写入
    fs::write(path, formatted.as_bytes())?;
    
    if !json_output {
        println!("  Normalized: {} -> {}", 
                 format!("{} {} bom={}", sig.encoding, sig.line_ending, sig.has_bom),
                 format!("UTF-8 {} bom={}", target_le, target_sig.has_bom));
        if raw == formatted.as_bytes() {
            println!("  (no changes needed)");
        }
    }
    
    Ok(())
}
