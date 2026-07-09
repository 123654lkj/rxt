//! rxt refs — 引用查找(谁调用谁)
//!
//! 三种模式:
//!   默认      — 扫描所有源文件, 列出符号的全部出现, 语义分类 def/call
//!   --callers — 谁调用了 symbol: 只保留真实调用点(行内含 symbol( ),
//!               并标注每个调用点所属的外层函数, 方便看影响面
//!   --callees — symbol 调用了谁: 定位 def, 用 {} 深度/Python 缩进取函数体,
//!               扫描体内所有 ident( 调用, 去重输出
//!
//! 不依赖真 LSP, 文本启发式覆盖 80% 场景, 够 agent 顺着调用链走.
//! v0.7: 新增 --callers / --callees 双向调用链(灵感: codeseek 调用图 + loop-engineering impact 分析).

use std::path::PathBuf;
use std::path::Path;
use regex::Regex;
use serde_json::json;

/// refs 命令入口
///
/// - symbol: 要查找的符号名(精确匹配单词边界)
/// - root: 搜索根目录(默认当前目录)
/// - callers: 只列出真实调用 symbol 的位置(并标注所属函数)
/// - callees: 列出 symbol 函数体内调用的所有符号
/// - json_output: JSON 输出
pub fn run(symbol: &str, root: &Path, callers: bool, callees: bool, json_output: bool) -> anyhow::Result<()> {
    if callers {
        return run_callers(symbol, root, json_output);
    }
    if callees {
        return run_callees(symbol, root, json_output);
    }
    run_default(symbol, root, json_output)
}

// ============================== 默认模式 ==============================

/// 默认: 列出符号所有出现, 分 def / call
fn run_default(symbol: &str, root: &Path, json_output: bool) -> anyhow::Result<()> {
    // 单词边界精确匹配(避免 run 匹配到 runtime)
    let pattern = format!(r"\b{}\b", regex::escape(symbol));
    let re = Regex::new(&pattern)?;

    // 定义关键字(各语言)
    let def_keywords = [
        "fn", "def", "function", "func",
        "struct", "class", "interface", "enum", "trait",
        "type", "const", "let", "var",
    ];

    // 收集所有源文件
    let files = collect_source_files(root);

    let mut refs: Vec<Ref> = Vec::new();
    for f in &files {
        if let Ok(content) = std::fs::read_to_string(f) {
            let rel = rel_path(root, f);
            for (idx, line) in content.lines().enumerate() {
                let line_num = idx + 1;
                if !re.is_match(line) { continue; }
                let t = line.trim_start();
                // 判断是否定义行: 行首(去 pub/async/export 等修饰)是 def 关键字
                let stripped = strip_modifiers(t);
                let first_word = stripped.split_whitespace().next().unwrap_or("");
                let is_def = def_keywords.contains(&first_word);
                refs.push(Ref {
                    file: rel.clone(),
                    line: line_num,
                    kind: if is_def { "def" } else { "call" },
                    ctx: line.trim().to_string(),
                });
            }
        }
    }

    if json_output {
        let arr: Vec<_> = refs.iter().map(|r| json!({
            "file": r.file, "line": r.line, "kind": r.kind, "ctx": r.ctx,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&json!(arr))?);
    } else {
        print_text(&refs, symbol);
    }
    Ok(())
}

struct Ref {
    file: String,
    line: usize,
    kind: &'static str,  // "def" / "call"
    ctx: String,
}

/// 去掉行首修饰词, 暴露出真正的关键字
fn strip_modifiers(line: &str) -> &str {
    let mut s = line;
    loop {
        let trimmed = s.trim_start();
        let next = trimmed.split_whitespace().next().unwrap_or("");
        if matches!(next, "pub" | "async" | "export" | "static" | "const" | "final" | "private" | "public" | "protected" | "abstract" | "virtual" | "override" | "unsafe" | "extern" | "crate") {
            // 跳过这个修饰词
            s = &trimmed[next.len()..];
        } else {
            return trimmed;
        }
    }
}

// ============================== --callers: 谁调用了 symbol ==============================

/// 列出真实调用 symbol 的位置, 并标注每个调用点所属的外层函数.
///
/// "真实调用点"判定: 行内出现 `symbol(` —— 排除定义行、注释里的提及、
/// 字符串里的同名 token. 这比默认模式的 call(单词出现)更准.
/// 所属函数: 从调用点向上找最近的一个 def 行(用 langs 解析当前文件的符号表).
fn run_callers(symbol: &str, root: &Path, json_output: bool) -> anyhow::Result<()> {
    // 调用点: symbol 后紧跟 ( (允许中间空白)
    let call_pat = format!(r"\b{}\s*\(", regex::escape(symbol));
    let re_call = Regex::new(&call_pat)?;
    // 定义行排除: 行首(去修饰)是 def 关键字 + symbol
    let def_keywords = [
        "fn", "def", "function", "func", "struct", "class",
        "interface", "enum", "trait", "type", "const", "let", "var",
    ];

    let files = collect_source_files(root);
    let mut callers: Vec<Caller> = Vec::new();

    for f in &files {
        let content = match std::fs::read_to_string(f) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = rel_path(root, f);
        let lines: Vec<&str> = content.lines().collect();

        // 预解析当前文件的符号表, 用于反查"调用点属于哪个函数"
        let symbols = crate::langs::extract_symbols(f, &content).unwrap_or_default();
        // 按行号排序, 方便二分查找最近的 def
        let mut sym_sorted: Vec<&crate::langs::Symbol> = symbols.iter().collect();
        sym_sorted.sort_by_key(|s| s.line);

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let t = line.trim_start();
            // 跳过定义行
            let stripped = strip_modifiers(t);
            let first_word = stripped.split_whitespace().next().unwrap_or("");
            if def_keywords.contains(&first_word) {
                continue;
            }
            // 跳过注释行
            if t.starts_with("//") || t.starts_with('#') || t.starts_with("/*") { continue; }
            if !re_call.is_match(line) { continue; }

            // 反查所属函数: 当前行号之前最近的符号定义
            let owner = sym_sorted
                .iter()
                .rev()
                .find(|s| s.line < line_num)
                .map(|s| (s.name.as_str(), s.line))
                .unwrap_or(("<top-level>", 0));

            callers.push(Caller {
                file: rel.clone(),
                line: line_num,
                in_fn: owner.0.to_string(),
                in_fn_line: owner.1,
                ctx: line.trim().to_string(),
            });
        }
    }

    if callers.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("没有找到调用 '{}' 的位置.", symbol);
        }
        return Ok(());
    }

    if json_output {
        let arr: Vec<_> = callers.iter().map(|c| json!({
            "file": c.file, "line": c.line,
            "in_fn": c.in_fn, "in_fn_line": c.in_fn_line, "ctx": c.ctx,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&json!(arr))?);
    } else {
        println!("callers '{}' — {} 处调用", symbol, callers.len());
        println!();
        // 按 in_fn 分组, 看影响面更清楚
        let mut by_fn: std::collections::BTreeMap<String, Vec<&Caller>> = std::collections::BTreeMap::new();
        for c in &callers {
            by_fn.entry(c.in_fn.clone()).or_default().push(c);
        }
        for (fn_name, cs) in &by_fn {
            println!("── {} ({} 处) ──", fn_name, cs.len());
            for c in cs {
                println!("  {}:{}  {}", c.file, c.line, c.ctx);
            }
            println!();
        }
    }
    Ok(())
}

struct Caller {
    file: String,
    line: usize,
    in_fn: String,
    in_fn_line: usize,
    ctx: String,
}

// ============================== --callees: symbol 调用了谁 ==============================

/// 列出 symbol 函数体内调用的所有符号.
///
/// 流程:
///   1. 找到 symbol 的定义(用 langs 解析全项目符号表)
///   2. 用 digest::count_body 算出函数体范围 [def_line, def_line + body)
///   3. 扫描该范围内所有 `ident(` 形式的调用, 去掉关键字/控制流, 去重输出
fn run_callees(symbol: &str, root: &Path, json_output: bool) -> anyhow::Result<()> {
    let files = collect_source_files(root);

    // 第一步: 找 symbol 的定义. 遍历所有文件, 解析符号表, 命中名字相等且是可调用 kind.
    let callable_kinds = ["fn", "def", "function", "func"];
    let mut def_hit: Option<(PathBuf, usize, String, String)> = None; // (file, line, kind, content)
    'outer: for f in &files {
        let content = match std::fs::read_to_string(f) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(syms) = crate::langs::extract_symbols(f, &content) {
            for s in &syms {
                if s.name == symbol && callable_kinds.contains(&s.kind.as_str()) {
                    def_hit = Some((f.clone(), s.line, s.kind.clone(), content.clone()));
                    break 'outer;
                }
            }
        }
    }

    let (def_file, def_line, def_kind, content) = match def_hit {
        Some(h) => h,
        None => {
            if json_output { println!("[]"); } else {
                println!("找不到 '{}' 的函数定义(需要 fn/def/function/func).", symbol);
            }
            return Ok(());
        }
    };
    let rel = rel_path(root, &def_file);
    let lines: Vec<&str> = content.lines().collect();

    // 第二步: 函数体范围
    let body_len = crate::digest::count_body(&lines, def_line, &def_kind);
    let body_start_idx = def_line.saturating_sub(1);
    let body_end_idx = (def_line + body_len).min(lines.len());  // def_line 是 1-indexed

    // 第三步: 扫描 body 内的调用. 匹配 ident( , 排除关键字/控制流/属性访问(.ident).
    let call_re = Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(")?;
    let exclude = [
        "if", "for", "while", "match", "switch", "catch", "return", "print", "println",
        "eprintln", "eprint", "format", "vec", "Some", "Ok", "Err", "None", "assert",
    ];
    // 关键字/定义符不应被当作调用
    let def_keywords = [
        "fn", "def", "function", "func", "struct", "class", "interface", "enum",
        "trait", "type", "const", "let", "var", "async", "pub",
    ];

    // 记录: name -> (首次出现的 file:line, 出现次数)
    let mut callees: std::collections::BTreeMap<String, (String, usize)> = std::collections::BTreeMap::new();

    for i in body_start_idx..body_end_idx {
        if i >= lines.len() { break; }
        let line = lines[i];
        let line_num = i + 1;
        for cap in call_re.captures_iter(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() { continue; }
            if exclude.contains(&name) || def_keywords.contains(&name) { continue; }
            // 排除属性访问: .name( (前面是点) —— 这是方法调用, 属于别的对象的实现, 噪音大
            let m_start = cap.get(1).map(|m| m.start()).unwrap_or(0);
            if m_start > 0 {
                let before = &line[..m_start];
                if before.ends_with('.') { continue; }
            }
            let loc = format!("{}:{}", rel, line_num);
            let entry = callees.entry(name.to_string()).or_insert((loc.clone(), 0));
            entry.1 += 1;
            if entry.0.is_empty() { entry.0 = loc; }
        }
    }

    if callees.is_empty() {
        if json_output {
            println!("{{}}");
        } else {
            println!("'{}' 的函数体内没有发现调用.", symbol);
        }
        return Ok(());
    }

    // 按出现次数降序排
    let mut sorted: Vec<(&String, &(String, usize))> = callees.iter().collect();
    sorted.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

    if json_output {
        let arr: Vec<_> = sorted.iter().map(|(name, (loc, cnt))| json!({
            "name": name, "count": cnt, "first_at": loc,
        })).collect();
        let out = json!({
            "symbol": symbol,
            "defined_at": format!("{}:{}", rel, def_line),
            "body_lines": body_len,
            "callees": arr,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("callees '{}' (定义于 {}:{}, 函数体 {} 行)", symbol, rel, def_line, body_len);
        println!();
        println!("── 调用了 {} 个符号 ──", sorted.len());
        for (name, (loc, cnt)) in &sorted {
            println!("  {:>3}×  {:30}  @ {}", cnt, name, loc);
        }
    }
    Ok(())
}

// ============================== 公共工具 ==============================

fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    crate::common::walk_clean(root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .collect()
}

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_else(|_| p.display().to_string())
}

fn print_text(refs: &[Ref], symbol: &str) {
    if refs.is_empty() {
        println!("No references to '{}' found.", symbol);
        return;
    }
    let defs: Vec<&Ref> = refs.iter().filter(|r| r.kind == "def").collect();
    let calls: Vec<&Ref> = refs.iter().filter(|r| r.kind == "call").collect();
    println!("refs '{}' — {} def, {} call", symbol, defs.len(), calls.len());
    println!();
    if !defs.is_empty() {
        println!("── Definitions ──");
        for r in &defs {
            println!("  {}:{}  {}", r.file, r.line, r.ctx);
        }
        println!();
    }
    if !calls.is_empty() {
        println!("── References ({}) ──", calls.len());
        for r in calls.iter().take(50) {
            println!("  {}:{}  {}", r.file, r.line, r.ctx);
        }
        if calls.len() > 50 {
            println!("  ... and {} more (use --json for all)", calls.len() - 50);
        }
    }
}

/// v0.8.0: 从函数体文本中提取所有调用 (供 callgraph 模块复用).
/// 返回 (被调用的符号名, 行号 1-indexed) 列表.
/// 排除控制流(if/for/while)、定义关键字(fn/struct)、属性访问(.method).
pub fn extract_calls_from_body(lines: &[&str], start_idx: usize, end_idx: usize) -> Vec<(String, usize)> {
    let call_re = match Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let exclude = [
        "if", "for", "while", "match", "switch", "catch", "return", "print", "println",
        "eprintln", "eprint", "format", "vec", "Some", "Ok", "Err", "None", "assert",
    ];
    let def_keywords = [
        "fn", "def", "function", "func", "struct", "class", "interface", "enum",
        "trait", "type", "const", "let", "var", "async", "pub",
    ];

    let mut calls = Vec::new();
    for i in start_idx..end_idx.min(lines.len()) {
        let line = lines[i];
        let line_num = i + 1;
        for cap in call_re.captures_iter(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() { continue; }
            if exclude.contains(&name) || def_keywords.contains(&name) { continue; }
            // 排除属性访问 .name(
            let m_start = cap.get(1).map(|m| m.start()).unwrap_or(0);
            if m_start > 0 && line[..m_start].ends_with('.') { continue; }
            calls.push((name.to_string(), line_num));
        }
    }
    calls
}
