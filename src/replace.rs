//! 块替换 — 格式保持版
//! 按文件块做结构化多行替换，不破坏换行符/BOM

use std::path::Path;
use std::fs;

use crate::signature::{FileSignature, to_utf8_lf, apply_format};

/// 块替换 — 按文件块做结构化多行替换
pub fn run(
    target: &Path,
    old_file: &Path,
    new_content: Option<&str>,
    replace_all: bool,
    preview: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    // 读取目标文件
    let raw = if let Some(remote) = remote {
        remote.read_file(target)?
    } else {
        fs::read(target)?
    };
    let sig = FileSignature::detect(&raw);
    
    // 转为内部 UTF-8 + LF
    let file_text = to_utf8_lf(&raw, &sig);

    // Read old block (also normalize) - 本地文件
    let old_raw = fs::read(old_file)?;
    let old_sig = FileSignature::detect(&old_raw);
    let old_text = to_utf8_lf(&old_raw, &old_sig);
    
    let old_lines: Vec<&str> = old_text.lines().collect();
    let file_lines: Vec<&str> = file_text.lines().collect();

    if old_lines.is_empty() {
        anyhow::bail!("Old block is empty");
    }

    // Split new content into lines
    let new_lines: Vec<&str> = match new_content {
        Some(s) => s.lines().collect(),
        None => vec![],  // delete mode
    };

    // Find all occurrences of old block
    let mut positions = Vec::new();
    let mut i = 0;
    while i + old_lines.len() <= file_lines.len() {
        if file_lines[i..i + old_lines.len()] == old_lines[..] {
            positions.push(i);
            if !replace_all {
                break;
            }
            i += old_lines.len();
        } else {
            i += 1;
        }
    }

    if positions.is_empty() {
        eprintln!("  (no match found for old block)");
        return Ok(());
    }

    // Build result
    let mut result_lines: Vec<String> = Vec::new();
    let mut last_end = 0;
    let mut total_matches = 0;

    for &pos in &positions {
        total_matches += 1;
        // Add lines before this match
        for j in last_end..pos {
            result_lines.push(file_lines[j].to_string());
        }
        // Preview: show what's being replaced
        if preview {
            for j in pos..pos + old_lines.len() {
                println!("- {}", file_lines[j]);
            }
            for j in 0..new_lines.len() {
                println!("+ {}", new_lines[j]);
            }
        }
        // Add new block
        for line in &new_lines {
            result_lines.push(line.to_string());
        }
        last_end = pos + old_lines.len();
    }

    // Add remaining lines after last match
    for j in last_end..file_lines.len() {
        result_lines.push(file_lines[j].to_string());
    }

    if preview {
        println!("\n  Preview: {} occurrences, {} -> {} lines{}",
            total_matches, old_lines.len(), new_lines.len(),
            if new_lines.is_empty() { " (delete)" } else { "" });
        return Ok(());
    }

    // Write result — 保持原始格式
    let output = result_lines.join("\n");
    let final_out = if file_text.ends_with('\n') {
        format!("{}\n", output.trim_end())
    } else {
        output
    };
    
    let formatted = apply_format(&final_out, &sig);
    if let Some(remote) = remote {
        remote.write_file(target, formatted.as_bytes())?;
    } else {
        fs::write(target, formatted.as_bytes())?;
    }
    println!("  Replaced block ({} occurrence{}) in {} (preserved: {} {})",
        total_matches, if total_matches > 1 { "s" } else { "" },
        target.display(), sig.encoding, sig.line_ending);
    Ok(())
}
