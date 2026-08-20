//! 智能文件读取 — 自动编码检测 + 跨平台换行符处理
//! RXT 内部统一使用 UTF-8 + LF，读入时自动转换
//! v0.4.0: 真实文件行号 + token 预算截断

use std::fs;
use std::path::Path;

use crate::signature::{to_utf8_lf, FileSignature};

pub fn run(
    path: &Path,
    encoding: Option<String>,
    number: bool,
    head: Option<usize>,
    tail: Option<usize>,
    lines: Option<String>,
    budget: Option<usize>,
    json_output: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
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
            "utf-8" | "utf8" => String::from_utf8_lossy(&raw)
                .to_string()
                .replace("\r\n", "\n"),
            _ => to_utf8_lf(&raw, &sig),
        }
    } else {
        to_utf8_lf(&raw, &sig)
    };

    // v0.4.0: 带真实行号(1-indexed)的行列表
    let all_lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    let total_lines = all_lines.len();

    // 选择要显示的行(带真实行号)
    let mut show_lines: Vec<(usize, &str)> = match (head, tail, lines) {
        (Some(h), None, None) => all_lines.iter().take(h).cloned().collect(),
        (None, Some(t), None) => {
            let n = all_lines.len();
            let start = n.saturating_sub(t);
            all_lines.iter().skip(start).cloned().collect()
        }
        (None, None, Some(l)) => parse_line_range(&all_lines, &l),
        _ => all_lines.clone(),
    };

    // v0.4.0: token 预算截断
    let mut truncated = false;
    let mut est_tokens = 0usize;
    if let Some(b) = budget {
        if b > 0 {
            // 计算当前 token 并决定截断点
            let mut acc = 0usize;
            let mut cut = show_lines.len();
            for (i, (_, txt)) in show_lines.iter().enumerate() {
                let lt = crate::common::approx_tokens(txt);
                if acc + lt > b {
                    cut = i;
                    break;
                }
                acc += lt;
            }
            if cut < show_lines.len() {
                truncated = true;
            }
            est_tokens = acc;
            show_lines.truncate(cut);
        }
    }
    if est_tokens == 0 {
        est_tokens = show_lines
            .iter()
            .map(|(_, t)| crate::common::approx_tokens(t))
            .sum();
    }

    if json_output {
        // JSON 输出给 AI(行号是真实文件行号)
        let json = serde_json::json!({
            "path": path.display().to_string(),
            "encoding": sig.encoding.to_string(),
            "line_ending": sig.line_ending.to_string(),
            "bom": sig.has_bom,
            "indent": sig.indent.to_string(),
            "total_lines": total_lines,
            "shown_lines": show_lines.len(),
            "est_tokens": est_tokens,
            "truncated": truncated,
            "budget": budget,
            "content": show_lines.iter().map(|(line_no, text)| {
                serde_json::json!({
                    "line": line_no,
                    "text": text
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        // 人类可读输出
        eprintln!(
            "  encoding: {} | line_ending: {} | bom: {} | indent: {} | lines: {} | bytes: {}",
            sig.encoding,
            sig.line_ending,
            sig.has_bom,
            sig.indent,
            total_lines,
            raw.len()
        );

        for (line_no, line) in show_lines.iter() {
            if number {
                println!("{:>6}  {}", line_no, line);
            } else {
                println!("{}", line);
            }
        }
        if truncated {
            if let Some(b) = budget {
                eprintln!("\n  (truncated: {}/{} lines, {} tokens > budget {}; use a larger --budget or --lines to continue)", 
                    show_lines.len(), total_lines, est_tokens, b);
            }
        }
    }
    Ok(())
}

/// Parse line range string like "10-20", "15", "-30", "50-"
/// v0.4.0: 返回带真实行号的 Vec<(usize, &str)>
fn parse_line_range<'a>(lines: &'a [(usize, &'a str)], spec: &str) -> Vec<(usize, &'a str)> {
    let total = lines.len();
    let spec = spec.trim();

    if let Some(range) = spec.split_once('-') {
        let start = range.0.trim();
        let end = range.1.trim();
        let s = if start.is_empty() {
            0
        } else {
            start.parse::<usize>().unwrap_or(1).saturating_sub(1)
        };
        let e = if end.is_empty() {
            total
        } else {
            end.parse::<usize>().unwrap_or(total).min(total)
        };
        lines
            .iter()
            .skip(s)
            .take(e.saturating_sub(s))
            .cloned()
            .collect()
    } else if let Ok(n) = spec.parse::<usize>() {
        // Single line number
        if n >= 1 && n <= total {
            lines.iter().skip(n - 1).take(1).cloned().collect()
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
