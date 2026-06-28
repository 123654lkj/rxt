//! langparse — 多语言代码结构解析
//!
//! 统一接口: 各语言解析器实现 parse(), 返回 Vec<CodeItem>。
//! struct/digest/refs 共享同一套语言检测 + 解析。
//!
//! 支持语言: Rust / Go / Python / JS-TS / Zero / C
//! (各语言解析器在 langparse/ 子模块)

use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeItem {
    pub kind: String,      // "fn" / "method" / "struct" / "class" / "interface" / "enum" / ...
    pub name: String,      // 符号名
    pub signature: String, // 完整签名(用于显示)
    pub line: usize,       // 行号(1-indexed)
}

/// 检测文件语言(按扩展名)
pub fn detect_lang(path: &Path) -> Option<Lang> {
    let ext = path.extension().and_then(|e| e.to_str())?.to_lowercase();
    let ext_str = ext.as_str();
    Some(match ext_str {
        "rs" => Lang::Rust,
        "go" => Lang::Go,
        "py" | "pyw" => Lang::Python,
        "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
        "ts" | "tsx" => Lang::TypeScript,
        "zero" => Lang::Zero,
        "c" | "h" => Lang::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => Lang::Cpp,
        "java" => Lang::Java,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Zero,
    C,
    Cpp,
    Java,
}

impl Lang {
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["rs"],
            Lang::Go => &["go"],
            Lang::Python => &["py", "pyw"],
            Lang::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Lang::TypeScript => &["ts", "tsx"],
            Lang::Zero => &["zero"],
            Lang::C => &["c", "h"],
            Lang::Cpp => &["cpp", "cc", "cxx", "hpp", "hh"],
            Lang::Java => &["java"],
        }
    }

    pub fn all_known_exts() -> Vec<&'static str> {
        [
            "rs", "go", "py", "pyw", "js", "jsx", "mjs", "cjs",
            "ts", "tsx", "zero", "c", "h", "cpp", "cc", "cxx", "hpp", "hh", "java",
        ].to_vec()
    }
}

/// 主入口: 按语言分派到对应解析器
pub fn parse(content: &str, lang: Lang) -> Vec<CodeItem> {
    match lang {
        Lang::Rust => rust_parse(content),
        Lang::Go => go_parse(content),
        Lang::Python => python_parse(content),
        Lang::JavaScript | Lang::TypeScript => js_parse(content),
        Lang::Zero => zero_parse(content),
        Lang::C | Lang::Cpp => c_parse(content),
        Lang::Java => java_parse(content),
    }
}

/// 过滤 helper: 按 only_functions / only_types 筛选
pub fn filter(items: Vec<CodeItem>, only_functions: bool, only_types: bool) -> Vec<CodeItem> {
    if !only_functions && !only_types {
        return items;
    }
    items.into_iter().filter(|i| {
        let is_fn = matches!(i.kind.as_str(),
            "fn" | "method" | "function" | "constructor" | "destructor" | "getter" | "setter" | "async");
        if only_functions { is_fn } else { !is_fn }
    }).collect()
}

// ============ 通用工具函数(各语言解析器共享) ============

/// 跳过行注释和块注释的简单状态机,返回每行是否"有效代码"
pub fn code_lines(content: &str, line_comment: &str, block_start: &str, block_end: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut in_block = false;
    for (idx, line) in content.lines().enumerate() {
        let t = line.trim();
        if in_block {
            if t.contains(block_end) { in_block = false; }
            continue;
        }
        if t.starts_with(line_comment) { continue; }
        if t.starts_with(block_start) {
            if !t.contains(block_end) { in_block = true; }
            continue;
        }
        result.push((idx + 1, line.to_string()));
    }
    result
}

/// 从签名提取第一个标识符名(跳过关键字)
pub fn first_ident_after(s: &str, keyword: &str) -> String {
    let after = s.find(keyword).map(|i| &s[i + keyword.len()..]).unwrap_or(s);
    let after = after.trim_start();
    let mut name = String::new();
    for c in after.chars() {
        if c.is_alphanumeric() || c == '_' { name.push(c); }
        else { break; }
    }
    if name.is_empty() { "<anonymous>".to_string() } else { name }
}

/// 截取签名到 { 或 ; 或行尾(去掉 trailing)
pub fn trim_sig(line: &str) -> String {
    let end = line.find(|c: char| c == '{' || c == ';').unwrap_or(line.len());
    line[..end].trim().trim_end_matches(',').trim().to_string()
}

// ============ 各语言解析器(实现在同文件,保持单文件无依赖) ============

// ---- Rust ----
fn rust_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        let (kw, kind) = if t.starts_with("pub fn ") || t.starts_with("fn ") {
            ("fn", "fn")
        } else if t.starts_with("pub struct ") || t.starts_with("struct ") {
            ("struct", "struct")
        } else if t.starts_with("pub enum ") || t.starts_with("enum ") {
            ("enum", "enum")
        } else if t.starts_with("pub trait ") || t.starts_with("trait ") {
            ("trait", "trait")
        } else if t.starts_with("pub type ") || t.starts_with("type ") {
            ("type", "type")
        } else if t.starts_with("impl ") {
            ("impl", "impl")
        } else if t.starts_with("pub const ") || t.starts_with("const ") || t.starts_with("pub static ") || t.starts_with("static ") {
            ("const", "const")
        } else if t.starts_with("pub mod ") || t.starts_with("mod ") {
            ("mod", "mod")
        } else {
            continue;
        };
        let sig = trim_sig(t);
        if sig.len() <= 3 { continue; }
        let name = if kind == "impl" {
            first_ident_after(&sig, "impl")
        } else {
            let kw_full = if t.starts_with("pub ") {
                format!("pub {}", kw)
            } else {
                kw.to_string()
            };
            first_ident_after(&sig, &kw_full)
        };
        items.push(CodeItem { kind: kind.to_string(), name, signature: sig, line: *ln });
    }
    items
}

// ---- Go ----
fn go_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        // func Name(...) 或 func (r Receiver) Method(...)
        if t.starts_with("func ") {
            let after = &t[5..];
            let sig = trim_sig(t);
            // method: func (recv Type) Name(...)
            let (kind, name) = if after.trim_start().starts_with('(') {
                // 提取 method 名: 找 ) 后面的标识符
                if let Some(close) = after.find(')') {
                    let after_recv = after[close+1..].trim_start();
                    let mname = first_ident(after_recv);
                    ("method", mname)
                } else {
                    ("fn", first_ident_after(after, ""))
                }
            } else {
                ("fn", first_ident(after))
            };
            items.push(CodeItem { kind: kind.to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("type ") {
            let sig = trim_sig(t);
            let rest = &t[5..];
            // type Name struct {...}  /  type Name interface {...}  /  type Name = X  /  type Name OtherType
            let name = first_ident(rest);
            let kind = if sig.contains("struct") { "struct" }
                       else if sig.contains("interface") { "interface" }
                       else { "type" };
            items.push(CodeItem { kind: kind.to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("var ") || t.starts_with("const ") {
            let kw = if t.starts_with("var ") { "var" } else { "const" };
            let sig = trim_sig(t);
            let name = first_ident_after(&sig, kw);
            items.push(CodeItem { kind: kw.to_string(), name, signature: sig, line: *ln });
        }
    }
    items
}

fn first_ident(s: &str) -> String {
    let s = s.trim_start();
    let mut name = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' { name.push(c); }
        else { break; }
    }
    if name.is_empty() { "<anonymous>".to_string() } else { name }
}

// ---- Python ----
fn python_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.starts_with('#') { continue; }
        // 计算缩进级别(只认 class/def 顶层,不强制)
        if t.starts_with("def ") {
            let name = first_ident_after(t, "def");
            let sig = t.trim_end_matches(':').trim().to_string();
            items.push(CodeItem { kind: "fn".to_string(), name, signature: sig, line: idx + 1 });
        } else if t.starts_with("async def ") {
            let name = first_ident_after(t, "def");
            let sig = t.trim_end_matches(':').trim().to_string();
            items.push(CodeItem { kind: "async".to_string(), name, signature: sig, line: idx + 1 });
        } else if t.starts_with("class ") {
            let name = first_ident_after(t, "class");
            let sig = t.trim_end_matches(':').trim().to_string();
            items.push(CodeItem { kind: "class".to_string(), name, signature: sig, line: idx + 1 });
        }
    }
    items
}

// ---- JavaScript / TypeScript ----
fn js_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        // function name(
        if let Some(rest) = t.strip_prefix("function ") {
            let name = first_ident(rest);
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "fn".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("async function ") {
            let name = first_ident_after(t, "function");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "async".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("class ") {
            let name = first_ident_after(t, "class");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "class".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("export ") {
            // export function / export class / export const / export default
            let inner = t.trim_start_matches("export ").trim_start();
            let inner = inner.trim_start_matches("default ").trim_start();
            let (kind, kw) = if inner.starts_with("function ") {
                ("fn", "function")
            } else if inner.starts_with("async function ") {
                ("async", "function")
            } else if inner.starts_with("class ") {
                ("class", "class")
            } else if inner.starts_with("const ") || inner.starts_with("let ") || inner.starts_with("var ") {
                ("const", if inner.starts_with("const ") {"const"} else if inner.starts_with("let ") {"let"} else {"var"})
            } else {
                continue;
            };
            let name = first_ident_after(inner, kw);
            let sig = trim_sig(t);
            items.push(CodeItem { kind: kind.to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("const ") || t.starts_with("let ") {
            // const name = () => / const name = function / const name = (
            let kw = if t.starts_with("const ") { "const" } else { "let" };
            // 检测是否是箭头函数/函数赋值(含 => 或 = function)
            if t.contains("=>") || t.contains("= function") || t.contains("= (") {
                let name = first_ident_after(t, kw);
                let sig = trim_sig(t);
                items.push(CodeItem { kind: "fn".to_string(), name, signature: sig, line: *ln });
            }
        }
    }
    items
}

// ---- Zero ----
fn zero_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        // 计算缩进: 只收顶层(缩进0)的定义, 跳过函数体内的局部 let
        let indent = line.len() - line.trim_start().len();
        if indent > 0 { continue; }
        // async fn 必须在 fn 之前判断
        if t.starts_with("async fn ") {
            let name = first_ident_after(t, "fn");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "async".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("fn ") {
            let name = first_ident_after(t, "fn");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "fn".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("class ") {
            let name = first_ident_after(t, "class");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "class".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("enum ") {
            let name = first_ident_after(t, "enum");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "enum".to_string(), name, signature: sig, line: *ln });
        } else if t.starts_with("import ") {
            let name = first_ident_after(t, "import");
            items.push(CodeItem { kind: "import".to_string(), name, signature: t.to_string(), line: *ln });
        } else if t.starts_with("let ") {
            // 顶层全局变量(如 lexer.zero 的 let keywords = {...})
            let name = first_ident_after(t, "let");
            let sig = trim_sig(t);
            if sig.contains('=') {
                items.push(CodeItem { kind: "let".to_string(), name, signature: sig, line: *ln });
            }
        }
    }
    items
}

// ---- C / C++ ----
fn c_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        // 返回类型 函数名(...) {  ->  简化: 以 ( 为锚点
        // 排除常见非函数(control 关键字)
        let lower = t.to_lowercase();
        if lower.starts_with("if ") || lower.starts_with("for ") || lower.starts_with("while ")
           || lower.starts_with("switch ") || lower.starts_with("return ") || lower.starts_with("sizeof") {
            continue;
        }
        if t.contains('(') && t.contains(')') {
            // 可能是函数定义: type name(args) {
            if let Some(paren) = t.find('(') {
                let before = t[..paren].trim();
                // name 是 before 最后一个 token
                let name = before.rsplit(|c: char| c.is_whitespace() || c == '*').next().unwrap_or("").to_string();
                if !name.is_empty() && name.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
                    // 区分 struct/enum/class 定义
                    let kind = if t.starts_with("struct ") { "struct" }
                               else if t.starts_with("class ") { "class" }
                               else { "fn" };
                    let sig = trim_sig(t);
                    items.push(CodeItem { kind: kind.to_string(), name, signature: sig, line: *ln });
                }
            }
        } else if t.starts_with("typedef struct") || t.starts_with("struct ") || t.starts_with("class ") {
            let sig = trim_sig(t);
            let name = first_ident_after(&sig, if t.starts_with("struct") {"struct"} else {"class"});
            items.push(CodeItem { kind: "struct".to_string(), name, signature: sig, line: *ln });
        }
    }
    items
}

// ---- Java ----
fn java_parse(content: &str) -> Vec<CodeItem> {
    let mut items = Vec::new();
    let lines = code_lines(content, "//", "/*", "*/");
    for (ln, line) in &lines {
        let t = line.trim();
        // 修饰符 + class/interface/enum
        if t.contains(" class ") {
            let name = first_ident_after(t, "class");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "class".to_string(), name, signature: sig, line: *ln });
        } else if t.contains(" interface ") {
            let name = first_ident_after(t, "interface");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "interface".to_string(), name, signature: sig, line: *ln });
        } else if t.contains(" enum ") {
            let name = first_ident_after(t, "enum");
            let sig = trim_sig(t);
            items.push(CodeItem { kind: "enum".to_string(), name, signature: sig, line: *ln });
        } else if t.contains("(") && t.contains(")") {
            // 方法: 修饰符? 返回类型 name(...) {
            // 排除 if/for/while 等
            let lower = t.to_lowercase();
            if lower.starts_with("if ") || lower.starts_with("for ") || lower.starts_with("while ") {
                continue;
            }
            if let Some(p) = t.find('(') {
                let before = t[..p].trim();
                let parts: Vec<&str> = before.rsplitn(2, char::is_whitespace).collect();
                let name = parts.first().unwrap_or(&"").to_string();
                // 排除 main 控制流(返回类型存在)
                if !name.is_empty() && name.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
                   && parts.len() == 2 {
                    let sig = trim_sig(t);
                    items.push(CodeItem { kind: "method".to_string(), name, signature: sig, line: *ln });
                }
            }
        }
    }
    items
}
