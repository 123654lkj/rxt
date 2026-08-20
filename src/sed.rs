//! 安全替换 — 格式保持的文本替换
//! 不破坏原始换行符/BOM/编码

use std::fs;
use std::path::Path;

use crate::signature::{apply_format, to_utf8_lf, FileSignature};
use regex::Regex;

/// 安全替换 — 类似 sed 的文本替换
pub fn run(
    path: &Path,
    pattern: &str,
    replacement: &str,
    preview: bool,
    line: Option<usize>,
    use_regex: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    let compiled_re = if use_regex {
        match Regex::new(pattern) {
            Ok(r) => Some(r),
            Err(e) => anyhow::bail!("Invalid regex: {}", e),
        }
    } else {
        None
    };
    // 读取原始文件
    let raw = if let Some(remote) = remote {
        remote.read_file(path)?
    } else {
        fs::read(path)?
    };

    let sig = FileSignature::detect(&raw);

    // 转为内部 UTF-8 + LF 格式
    let text = to_utf8_lf(&raw, &sig);
    let lines: Vec<&str> = text.lines().collect();
    let mut changed = Vec::new();
    let mut has_change = false;

    for (i, l) in lines.iter().enumerate() {
        if let Some(ln) = line {
            if i + 1 != ln {
                changed.push(l.to_string());
                continue;
            }
        }
        let matched = if let Some(ref re) = compiled_re {
            re.is_match(l)
        } else {
            l.contains(pattern)
        };
        if matched {
            let new = if let Some(ref re) = compiled_re {
                re.replace_all(l, replacement).to_string()
            } else {
                l.replace(pattern, replacement)
            };
            changed.push(new.clone());
            has_change = true;
            if preview {
                println!("  L{}: {} → {}", i + 1, l, new);
            }
        } else {
            changed.push(l.to_string());
        }
    }

    if !has_change {
        eprintln!("  No matches for '{}'", pattern);
        return Ok(());
    }

    if preview {
        println!(
            "\n  Preview: {} lines, {} matched. Use --preview to see changes.",
            lines.len(),
            changed.iter().filter(|c| c.contains(replacement)).count()
        );
        return Ok(());
    }

    // 关键：保持原始格式写回
    let result = changed.join("\n");
    let formatted = apply_format(&result, &sig);

    if let Some(remote) = remote {
        remote.write_file(path, formatted.as_bytes())?;
    } else {
        fs::write(path, formatted.as_bytes())?;
    }

    let count = changed
        .iter()
        .filter(|c| {
            if let Some(ref re) = compiled_re {
                re.is_match(c)
            } else {
                c.contains(replacement)
            }
        })
        .count();
    println!(
        "  Replaced {} occurrence(s) of '{}' -> '{}' in {} ({} bytes, {} {})",
        count,
        pattern,
        replacement,
        path.display(),
        raw.len(),
        sig.encoding,
        sig.line_ending
    );
    Ok(())
}
