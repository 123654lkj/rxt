/// 代码切块 — 把源码文件拆分为语义块
///
/// 复用现有模块:
/// - common::walk_clean() — 遍历文件
/// - langparse::detect_lang() + parse() — 提取符号
/// - 大括号深度配对 — 提取完整函数体

use std::path::{Path, PathBuf};
use super::provider::CodeChunk;

/// 扫描项目目录，返回所有代码块
pub fn scan_project(root: &Path) -> anyhow::Result<Vec<CodeChunk>> {
    let files = collect_source_files(root);
    let mut all_chunks = Vec::new();

    for file in &files {
        match chunk_file(root, file) {
            Ok(chunks) => all_chunks.extend(chunks),
            Err(e) => {
                // 单个文件失败不影响整体
                eprintln!("  警告: {} 切块失败: {}", file.display(), e);
            }
        }
    }

    Ok(all_chunks)
}

/// 收集项目中的源码文件
fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    crate::common::walk_clean(root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .collect()
}

/// 把单个文件切分为代码块
pub fn chunk_file(root: &Path, file: &Path) -> anyhow::Result<Vec<CodeChunk>> {
    let content = std::fs::read_to_string(file)?;
    let rel_path = file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();

    // 检测语言
    let language = lang_from_ext(
        file.extension().and_then(|e| e.to_str()).unwrap_or("")
    );

    // 用 detect_lang + parse 提取符号
    let symbols = match crate::langparse::detect_lang(file) {
        Some(lang) => crate::langparse::parse(&content, lang),
        None => return Ok(Vec::new()),
    };
    if symbols.is_empty() {
        return Ok(Vec::new());
    }

    let lines: Vec<&str> = content.lines().collect();
    let file_md5 = compute_md5(content.as_bytes());
    let mut chunks = Vec::new();

    for sym in &symbols {
        // 提取符号的完整内容（签名 + 函数体）
        let body = extract_symbol_body(&lines, sym.line, sym.kind.as_str());

        let chunk_content = format!(
            "[file: {}] [lang: {}] [kind: {}]\n{}\n---\n{}",
            rel_path, language, sym.kind,
            sym.signature,
            body
        );

        chunks.push(CodeChunk {
            file: rel_path.clone(),
            line: sym.line,
            end_line: sym.line + body.lines().count(),
            name: format!("{} {}", sym.kind, sym.name),
            kind: sym.kind.clone(),
            language: language.to_string(),
            content: chunk_content,
            md5: file_md5.clone(),
        });
    }

    Ok(chunks)
}

/// 从行数组中提取符号的完整内容（签名 + 函数体）
fn extract_symbol_body(lines: &[&str], start_line: usize, kind: &str) -> String {
    if start_line == 0 || start_line > lines.len() {
        return String::new();
    }

    let start_idx = start_line - 1; // 转为 0-indexed

    // 大括号语言: Rust, Go, JS/TS, C/C++, Java
    if is_brace_lang(kind) {
        return extract_brace_body(lines, start_idx);
    }

    // Python: 按缩进提取
    if kind == "def" || kind == "class" {
        return extract_python_body(lines, start_idx);
    }

    // 兜底: 取签名行 + 后面 20 行
    let end = (start_idx + 21).min(lines.len());
    lines[start_idx..end].join("\n")
}

/// 大括号语言: 从起始行找到匹配的 }
fn extract_brace_body(lines: &[&str], start_idx: usize) -> String {
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut end_idx = start_idx;

    for i in start_idx..lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
                if found_open && depth == 0 {
                    end_idx = i;
                    return lines[start_idx..=end_idx].join("\n");
                }
            }
        }
        // 没找到 { 的行（如 trait/method 声明），最多往后看 5 行
        if !found_open && i - start_idx > 5 {
            break;
        }
    }

    // 没找到匹配的 }，取起始行 + 30 行
    let end = (start_idx + 30).min(lines.len());
    lines[start_idx..end].join("\n")
}

/// Python: 按缩进提取函数体
fn extract_python_body(lines: &[&str], start_idx: usize) -> String {
    if start_idx >= lines.len() {
        return String::new();
    }

    // 获取函数定义行的缩进
    let base_indent = lines[start_idx].len() - lines[start_idx].trim_start().len();
    let mut end_idx = start_idx;

    for i in (start_idx + 1)..lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= base_indent {
            break;
        }
        end_idx = i;
    }

    lines[start_idx..=end_idx].join("\n")
}

fn is_brace_lang(kind: &str) -> bool {
    matches!(kind, "fn" | "struct" | "enum" | "trait" | "impl" | "function" | "class" | "interface" | "method")
}

fn lang_from_ext(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" | "pyw" => "python",
        "go" => "go",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "java" => "java",
        _ => "unknown",
    }
}

/// 计算内容哈希 (用 sha2, 项目已有依赖)
fn compute_md5(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    // 取前 16 字节作为短哈希
    hex::encode(&result[..16])
}
