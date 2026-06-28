//! 文件指纹检测 — 编码/换行符/BOM/缩进风格
//! RXT 核心模块：让 AI 无感跨平台文本操作

use std::path::Path;

/// 文件编码
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    UTF8,
    UTF8BOM,
    GBK,
    GB2312,
    UTF16LE,
    UTF16BE,
    Latin1,
    Unknown,
}

impl std::fmt::Display for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Encoding::UTF8 => write!(f, "UTF-8"),
            Encoding::UTF8BOM => write!(f, "UTF-8-BOM"),
            Encoding::GBK => write!(f, "GBK"),
            Encoding::GB2312 => write!(f, "GB2312"),
            Encoding::UTF16LE => write!(f, "UTF-16LE"),
            Encoding::UTF16BE => write!(f, "UTF-16BE"),
            Encoding::Latin1 => write!(f, "Latin-1"),
            Encoding::Unknown => write!(f, "Unknown"),
        }
    }
}

/// 换行符风格
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    LF,      // \n (Linux/Mac)
    CRLF,    // \r\n (Windows)
    CR,      // \r (老 Mac，罕见)
    Mixed,   // 混合（警告）
}

impl std::fmt::Display for LineEnding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineEnding::LF => write!(f, "LF"),
            LineEnding::CRLF => write!(f, "CRLF"),
            LineEnding::CR => write!(f, "CR"),
            LineEnding::Mixed => write!(f, "Mixed"),
        }
    }
}

/// 缩进风格
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IndentStyle {
    Tab,
    Spaces(usize),  // 2, 4, 8 等
    None,           // 没有缩进或无法检测
}

impl std::fmt::Display for IndentStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndentStyle::Tab => write!(f, "tab"),
            IndentStyle::Spaces(n) => write!(f, "spaces-{}", n),
            IndentStyle::None => write!(f, "none"),
        }
    }
}

/// 文件指纹 — 描述文件的完整特征
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileSignature {
    pub encoding: Encoding,
    pub line_ending: LineEnding,
    pub has_bom: bool,
    pub indent: IndentStyle,
    pub lines: usize,
    pub bytes: usize,
}

impl FileSignature {
    /// 从字节检测文件指纹
    pub fn detect(raw: &[u8]) -> Self {
        let encoding = detect_encoding(raw);
        let has_bom = detect_bom(raw);
        let line_ending = detect_line_ending(raw);
        let indent = detect_indent(raw);
        let lines = raw.iter().filter(|&&b| b == b'\n').count();
        let bytes = raw.len();

        Self {
            encoding,
            line_ending,
            has_bom,
            indent,
            lines,
            bytes,
        }
    }

    /// 从文件路径检测
    pub fn detect_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read(path)?;
        Ok(Self::detect(&raw))
    }
}

/// 检测编码
fn detect_encoding(raw: &[u8]) -> Encoding {
    // UTF-16 BOM
    if raw.len() >= 2 {
        if raw[0] == 0xFF && raw[1] == 0xFE {
            return Encoding::UTF16LE;
        }
        if raw[0] == 0xFE && raw[1] == 0xFF {
            return Encoding::UTF16BE;
        }
    }

    // UTF-8 BOM
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return Encoding::UTF8BOM;
    }

    // 尝试 UTF-8
    if std::str::from_utf8(raw).is_ok() {
        return Encoding::UTF8;
    }

    // 尝试 GBK
    let (_, _, had_errors) = encoding_rs::GBK.decode(raw);
    if !had_errors {
        return Encoding::GBK;
    }

    Encoding::Unknown
}

/// 检测 BOM
fn detect_bom(raw: &[u8]) -> bool {
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return true;
    }
    if raw.len() >= 2 && ((raw[0] == 0xFF && raw[1] == 0xFE) || (raw[0] == 0xFE && raw[1] == 0xFF)) {
        return true;
    }
    false
}

/// 检测换行符风格
fn detect_line_ending(raw: &[u8]) -> LineEnding {
    let mut lf_count = 0;
    let mut crlf_count = 0;
    let mut cr_only_count = 0;

    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\r' {
            if i + 1 < raw.len() && raw[i + 1] == b'\n' {
                crlf_count += 1;
                i += 2;
            } else {
                cr_only_count += 1;
                i += 1;
            }
        } else if raw[i] == b'\n' {
            lf_count += 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    // 判断主导风格
    let total = lf_count + crlf_count + cr_only_count;
    if total == 0 {
        return LineEnding::LF; // 默认 LF
    }

    // 如果混合，检查是否有一种占绝对主导 (>90%)
    let max_count = lf_count.max(crlf_count).max(cr_only_count);
    if max_count as f64 / (total as f64) < 0.9 {
        return LineEnding::Mixed;
    }

    if crlf_count > lf_count && crlf_count > cr_only_count {
        LineEnding::CRLF
    } else if cr_only_count > lf_count && cr_only_count > crlf_count {
        LineEnding::CR
    } else {
        LineEnding::LF
    }
}

/// 检测缩进风格
fn detect_indent(raw: &[u8]) -> IndentStyle {
    // 只分析前 50 行
    let text = String::from_utf8_lossy(raw);
    let lines: Vec<&str> = text.lines().take(50).collect();

    let mut tab_count = 0;
    let mut space_counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }

        let leading = line.len() - line.trim_start().len();
        if leading == 0 {
            continue;
        }

        if line.starts_with('\t') {
            tab_count += 1;
        } else if line.starts_with(' ') {
            space_counts.entry(leading).and_modify(|c| *c += 1).or_insert(1);
        }
    }

    // 判断主导风格
    let total_tabs = tab_count;
    let total_spaces: usize = space_counts.values().sum();

    if total_tabs == 0 && total_spaces == 0 {
        return IndentStyle::None;
    }

    if total_tabs > total_spaces {
        return IndentStyle::Tab;
    }

    // 找到最常见的空格数（可能是 2, 4, 8 的倍数）
    if let Some((&indent_size, _)) = space_counts.iter().max_by_key(|(_, &count)| count) {
        // 规范化到常见缩进
        let normalized = if indent_size <= 2 { 2 }
                        else if indent_size <= 4 { 4 }
                        else { 8 };
        return IndentStyle::Spaces(normalized);
    }

    IndentStyle::None
}

/// 将原始字节转为 UTF-8 + LF（RXT 内部标准格式）
pub fn to_utf8_lf(raw: &[u8], sig: &FileSignature) -> String {
    // 第一步：解码为 UTF-8
    let text = match sig.encoding {
        Encoding::UTF8 | Encoding::UTF8BOM => {
            if sig.has_bom {
                String::from_utf8_lossy(&raw[3..]).to_string()
            } else {
                String::from_utf8_lossy(raw).to_string()
            }
        }
        Encoding::GBK | Encoding::GB2312 => {
            let (text, _, _) = encoding_rs::GBK.decode(raw);
            text.to_string()
        }
        Encoding::UTF16LE => {
            // TODO: 实现 UTF-16LE 解码
            String::from_utf8_lossy(raw).to_string()
        }
        Encoding::UTF16BE => {
            // TODO: 实现 UTF-16BE 解码
            String::from_utf8_lossy(raw).to_string()
        }
        _ => String::from_utf8_lossy(raw).to_string(),
    };

    // 第二步：统一换行符为 LF
    text.replace("\r\n", "\n").replace("\r", "\n")
}

/// 将 UTF-8 + LF 内容应用原始格式
pub fn apply_format(content: &str, sig: &FileSignature) -> String {
    // 确保内容是 LF
    let text = content.replace("\r\n", "\n").replace("\r", "\n");

    // 应用换行符风格
    let text = match sig.line_ending {
        LineEnding::CRLF => text.replace("\n", "\r\n"),
        LineEnding::LF | LineEnding::Mixed => text,
        LineEnding::CR => text.replace("\n", "\r"),
    };

    // 应用 BOM
    if sig.has_bom {
        format!("\u{FEFF}{}", text)
    } else {
        text
    }
}