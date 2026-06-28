//! v0.4.0 多语言符号解析 — 统一 trait + 4 语言实现
//! map/digest/refs 共用的代码理解基础
//!
//! 设计: 行首关键字前缀匹配(比真 LSP 轻, 但够 agent 用),
//! 复用 struct.rs 验证过的注释跳过 + 名字提取逻辑.

use std::path::Path;

/// 一个解析出的代码符号
#[derive(Debug, Clone, serde::Serialize)]
pub struct Symbol {
    pub kind: String,    // "fn" / "def" / "function" / "struct" / "class" ...
    pub name: String,    // 符号名
    pub signature: String, // 签名(到 { 或 ; 为止, 截断 100 字符)
    pub line: usize,     // 1-indexed 行号
}

/// 语言解析器 trait — 每种语言一个实现
pub trait LanguageParser {
    fn extensions(&self) -> &[&str];
    fn parse(&self, content: &str) -> Vec<Symbol>;
}

/// 按文件扩展名选解析器(返回 None 表示不支持)
pub fn parser_for(path: &Path) -> Option<Box<dyn LanguageParser>> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    match ext {
        "rs" => Some(Box::new(RustParser)),
        "py" => Some(Box::new(PythonParser)),
        "js" | "jsx" | "mjs" | "cjs" => Some(Box::new(JsParser { typescript: false })),
        "ts" | "tsx" => Some(Box::new(JsParser { typescript: true })),
        "go" => Some(Box::new(GoParser)),
        _ => None,
    }
}

/// 便捷封装: 读取文件 + 按 ext 分发解析. 不支持的文件返回 None.
pub fn extract_symbols(path: &Path, content: &str) -> Option<Vec<Symbol>> {
    // 统一走 langparse(全 9 语言, 更准确), 保留 Symbol 类型兼容
    crate::langparse::detect_lang(path).map(|lang| {
        crate::langparse::parse(content, lang).into_iter().map(|c| Symbol {
            kind: c.kind,
            name: c.name,
            signature: c.signature,
            line: c.line,
        }).collect()
    })
}

/// 该文件扩展名是否被支持
pub fn is_supported(path: &Path) -> bool {
    parser_for(path).is_some()
}

// ============================== Rust ==============================

pub struct RustParser;

impl LanguageParser for RustParser {
    fn extensions(&self) -> &[&str] { &["rs"] }
    fn parse(&self, content: &str) -> Vec<Symbol> {
        parse_generic(content, &rust_keywords(), CommentStyle::Slash)
    }
}

fn rust_keywords() -> Vec<Keyword> {
    vec![
        kw("fn"), kw("struct"), kw("enum"), kw("impl"),
        kw("trait"), kw("mod"), kw("type"), kw("const"),
        kw("static"),
    ]
}

// ============================== Python ==============================

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn extensions(&self) -> &[&str] { &["py"] }
    fn parse(&self, content: &str) -> Vec<Symbol> {
        parse_python(content)
    }
}

fn parse_python(content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let t = raw.trim_start();
        let indent = raw.len() - t.len();
        // 跳过注释和空行; 只认顶层(class 内方法用 self, 不算独立符号)
        if t.starts_with('#') || t.is_empty() { continue; }
        let line_num = idx + 1;
        // async def / def
        let def_line = if t.starts_with("async def ") {
            Some(("def", "async def "))
        } else if t.starts_with("def ") {
            Some(("def", "def "))
        } else { None };
        if let Some((kind, prefix)) = def_line {
            if indent == 0 {  // 只收顶层函数
                if let Some(name) = ident_after(t, prefix) {
                    symbols.push(Symbol {
                        kind: kind.into(),
                        name,
                        signature: trim_sig(t, ':'),
                        line: line_num,
                    });
                }
            }
            continue;
        }
        // class
        if t.starts_with("class ") && indent == 0 {
            if let Some(name) = ident_after(t, "class ") {
                symbols.push(Symbol {
                    kind: "class".into(),
                    name,
                    signature: trim_sig(t, ':'),
                    line: line_num,
                });
            }
        }
    }
    symbols
}

// ============================== JS / TS ==============================

pub struct JsParser { typescript: bool }

impl LanguageParser for JsParser {
    fn extensions(&self) -> &[&str] {
        if self.typescript { &["ts","tsx"] } else { &["js","jsx","mjs","cjs"] }
    }
    fn parse(&self, content: &str) -> Vec<Symbol> {
        let mut kws: Vec<Keyword> = vec![
            kw("function"), kw("class"),
            kw("const"), kw("let"), kw("var"),
        ];
        if self.typescript {
            kws.push(kw("interface"));
            kws.push(kw("enum"));
            kws.push(kw("type"));
        }
        parse_generic(content, &kws, CommentStyle::Slash)
    }
}

// ============================== Go ==============================

pub struct GoParser;

impl LanguageParser for GoParser {
    fn extensions(&self) -> &[&str] { &["go"] }
    fn parse(&self, content: &str) -> Vec<Symbol> {
        parse_generic(content, &go_keywords(), CommentStyle::Slash)
    }
}

fn go_keywords() -> Vec<Keyword> {
    vec![kw("func"), kw("struct"), kw("interface"), kw("type")]
}

// ============================== 通用解析引擎 ==============================

struct Keyword {
    word: &'static str,
}

fn kw(word: &'static str) -> Keyword { Keyword { word } }

#[derive(Clone, Copy)]
enum CommentStyle {
    Slash,  // // 行注释, /* */ 块注释 (Rust/JS/Go 通用)
}

/// 通用行首前缀解析器 — 复用 struct.rs 验证过的逻辑
fn parse_generic(content: &str, keywords: &[Keyword], _comment: CommentStyle) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut in_block = false;
    for (idx, raw) in content.lines().enumerate() {
        let t = raw.trim();
        // 注释跳过
        if t.starts_with("//") { continue; }
        if t.starts_with("/*") {
            if !t.contains("*/") { in_block = true; }
            continue;
        }
        if in_block {
            if t.contains("*/") { in_block = false; }
            continue;
        }
        let line_num = idx + 1;
        // 尝试每个关键字(pub 前缀和裸前缀都试)
        for k in keywords {
            let kw_word = k.word;
            // 构建 "pub fn " / "fn " 两种前缀(JS/Go 无 pub, 但 "pub " 前缀匹配会自然失败, 无害)
            let with_pub = format!("pub {} ", kw_word);
            let bare = format!("{} ", kw_word);
            if let Some(sym) = try_match(t, kw_word, &[&with_pub, &bare], line_num) {
                symbols.push(sym);
                break;  // 一行只算一个符号
            }
        }
    }
    symbols
}

/// 尝试匹配某行的关键字前缀, 成功返回 Symbol
fn try_match(line: &str, kw_word: &str, prefixes: &[&str], line_num: usize) -> Option<Symbol> {
    for prefix in prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            let rest = rest.trim_start();
            let name = first_ident(rest);
            if name.is_empty() { return None; }
            // 签名: 从关键字开始, 到 { 或 ; 为止, 截断 100
            let kw_idx = line.find(kw_word)?;
            let from_kw = &line[kw_idx..];
            let end = from_kw.find(|c: char| c == '{' || c == ';').unwrap_or(from_kw.len().min(100));
            let sig = from_kw[..end].trim().to_string();
            if sig.len() <= kw_word.len() + 1 { return None; }
            return Some(Symbol {
                kind: kw_word.to_string(),
                name,
                signature: sig,
                line: line_num,
            });
        }
    }
    None
}

/// 从字符串提取第一个标识符(字母/数字/下划线), 遇到 ( 或 { 等停止
fn first_ident(s: &str) -> String {
    let mut name = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            name.push(c);
        } else {
            break;
        }
    }
    name
}

/// 提取某前缀后的第一个标识符(Python 专用)
fn ident_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    let name = first_ident(rest);
    if name.is_empty() { None } else { Some(name) }
}

/// 截取签名到指定终止符(Python 用 : 作为签名结束)
fn trim_sig(line: &str, terminator: char) -> String {
    let end = line.find(terminator).unwrap_or(line.len().min(100));
    line[..end].trim().to_string()
}