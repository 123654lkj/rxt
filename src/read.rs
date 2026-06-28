//! 智能文件读取 — 自动编码检测 + 跨平台换行符处理
//! RXT 内部统一使用 UTF-8 + LF，读入时自动转换

use std::path::Path;
use std::fs;

use crate::signature::{FileSignature, to_utf8_lf};

pub fn run(path: &Path, encoding: Option<String>, number: bool, head: Option<usize>, tail: Option<usize>, lines: Option<String>, json_output: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    let raw = if let Some(remote) = remote {
        remote.read_file(path)?
    } else {
        fs::read(path)?
    };
    let sig = FileSignature::detect(&raw);

    // 使用签名模块转换为 UTF-8 + LF
    let text = if let Some(enc) = &encoding {
        // 强制指定编码
        match enc.as_str() {
            "gbk" | "gb2312" => {
                let (text, _, _) = encoding_rs::GBK.decode(&raw);
                text.to_string().replace("\r\n", "\n").replace("\r", "\n")
            }
            "utf-8" | "utf8" => {
                String::from_utf8_lossy(&raw).to_string().replace("\r\n", "\n")
            }
            _ => to_utf8_lf(&raw, &sig)
        }
    } else {
        to_utf8_lf(&raw, &sig)
    };

    let all_lines: Vec<&str> = text.lines().collect();

    let show_lines: Vec<&&str> = match (head, tail, lines) {
        (Some(h), None, None) => all_lines.iter().take(h).collect(),
        (None, Some(t), None) => all_lines.iter().rev().take(t).rev().collect(),
        (None, None, Some(l)) => parse_line_range(&all_lines, &l),
        _ => all_lines.iter().collect(),
    };

    if json_output {
        // JSON 输出给 AI
        let json = serde_json::json!({
            "path": path.display().to_string(),
            "encoding": sig.encoding.to_string(),
            "line_ending": sig.line_ending.to_string(),
            "bom": sig.has_bom,
            "indent": sig.indent.to_string(),
            "total_lines": all_lines.len(),
            "bytes": raw.len(),
            "content": show_lines.iter().enumerate().map(|(i, line)| {
                serde_json::json!({
                    "line": i + 1,
                    "text": *line
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        // 人类可读输出
        eprintln!("  encoding: {} | line_ending: {} | bom: {} | indent: {} | lines: {} | bytes: {}", 
                  sig.encoding, sig.line_ending, sig.has_bom, sig.indent, all_lines.len(), raw.len());

        for (i, line) in show_lines.iter().enumerate() {
            if number {
                println!("{:>6}  {}", i + 1, line);
            } else {
                println!("{}", line);
            }
        }
    }
    Ok(())
}

/// Parse line range string like "10-20", "15", "-30", "50-"
fn parse_line_range<'a>(lines: &'a [&'a str], spec: &str) -> Vec<&'a &'a str> {
    let total = lines.len();
    let spec = spec.trim();

    if let Some(range) = spec.split_once('-') {
        let start = range.0.trim();
        let end = range.1.trim();
        let s = if start.is_empty() { 0 } else { start.parse::<usize>().unwrap_or(1).saturating_sub(1) };
        let e = if end.is_empty() { total } else { end.parse::<usize>().unwrap_or(total).min(total) };
        lines.iter().skip(s).take(e.saturating_sub(s)).collect()
    } else if let Ok(n) = spec.parse::<usize>() {
        // Single line number
        if n >= 1 && n <= total {
            lines.iter().skip(n - 1).take(1).collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}

/// 公共 API：读取文件并返回 UTF-8 + LF 内容
pub fn read_utf8_lf(path: &Path) -> anyhow::Result<(String, FileSignature)> {
    let raw = fs::read(path)?;
    let sig = FileSignature::detect(&raw);
    let text = to_utf8_lf(&raw, &sig);
    Ok((text, sig))
}
