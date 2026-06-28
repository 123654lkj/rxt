use std::path::{Path, PathBuf};
use std::fs;
use crate::signature::{FileSignature, to_utf8_lf};
use std::collections::BTreeMap;
use regex::Regex;

pub fn run(
    query: Option<&str>,
    path: Option<&Path>,
    name_pattern: Option<&str>,
    file_type: Option<&str>,
    context: usize,
    case_sensitive: bool,
    count: bool,
    do_stats: bool,
    replace: Option<&str>,
    replace_with: Option<&str>,
    preview: bool,
    use_regex: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    let compiled_re = if use_regex {
        match query {
            Some(q) => match Regex::new(q) {
                Ok(r) => Some(r),
                Err(e) => anyhow::bail!("Invalid regex: {}", e),
            },
            None => None,
        }
    } else {
        None
    };
    let search_dir = path.unwrap_or(Path::new("."));

    // --replace + --with 批量替换（独立分支，query 无关）
    if let (Some(old), Some(new)) = (replace, replace_with) {
        return run_replace(search_dir, name_pattern, file_type, old, new, preview);
    }

    // --stats 模式：query 可选
    if do_stats {
        return stats_by_type(search_dir);
    }

    // --name <pattern> 模式：query 可选
    if let Some(pattern) = name_pattern {
        return search_by_name(search_dir, pattern, file_type);
    }

    // <query> 内容搜索模式
    if let Some(q) = query {
        let ext_filter = file_type.and_then(|t| ext_for_type(t));
        return search_content(search_dir, q, ext_filter, context, case_sensitive, count, &compiled_re);
    }

    // 全部未指定 -> 错误退出，让 shell 拿到非 0 退出码
    anyhow::bail!("no action: provide a <query>, --name <pattern>, --stats, or --replace <old> --with <new>")
}

fn ext_for_type(t: &str) -> Option<&'static str> {
    match t {
        "rs" => Some("rs"), "py" => Some("py"), "md" => Some("md"),
        "toml" => Some("toml"), "json" => Some("json"), "js" => Some("js"),
        "ts" => Some("ts"), "c" => Some("c"), "h" => Some("h"),
        "cpp" => Some("cpp"), "go" => Some("go"), "sh" => Some("sh"),
        "yaml" | "yml" => Some("yml"),
        _ => None,
    }
}

fn is_text_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).map(|e| matches!(e,
        "rs"|"py"|"md"|"toml"|"json"|"js"|"ts"|"c"|"h"|"cpp"|"go"|"sh"|"yml"|"yaml"|"txt"|"cfg"|"ini"|"conf"|"lock"|"css"|"html"|"java"|"kt"|"swift"|"rb"|"php"
    )).unwrap_or(false)
}

fn walk_files(dir: &Path, ext_filter: Option<&str>, cb: &mut impl FnMut(&Path)) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') || name == "node_modules" || name == "target" || name == "vendor" || name == ".git" { continue; }
                walk_files(&p, ext_filter, cb);
            } else if p.is_file() {
                if let Some(ext) = ext_filter {
                    if p.extension().and_then(|e| e.to_str()) == Some(ext) { cb(&p); }
                } else if is_text_file(&p) { cb(&p); }
            }
        }
    }
}

fn search_content(dir: &Path, query: &str, ext_filter: Option<&str>, context: usize, case_sensitive: bool, only_count: bool, compiled_re: &Option<Regex>) -> anyhow::Result<()> {
    let query_lower = query.to_lowercase();
    let mut total_matches = 0usize;
    let mut files_with_matches = 0usize;
    walk_files(dir, ext_filter, &mut |path| {
        if let Ok(raw) = fs::read(path) {
            let sig = FileSignature::detect(&raw);
            let content = to_utf8_lf(&raw, &sig);
            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = 0usize;
            for (i, line) in lines.iter().enumerate() {
                let matched = if let Some(ref re) = compiled_re {
                    re.is_match(line)
                } else if case_sensitive {
                    line.contains(query)
                } else {
                    line.to_lowercase().contains(&query_lower)
                };
                if matched {
                    file_matches += 1;
                    if !only_count {
                        if file_matches == 1 { println!("\n{}:", path.display()); }
                        let start = if i >= context { i - context } else { 0 };
                        for ci in start..i { println!("  {:<6}| {}", ci + 1, lines[ci]); }
                        println!("→ {:<6}| {}", i + 1, line);
                        let end = (i + context + 1).min(lines.len());
                        for ci in (i + 1)..end { println!("  {:<6}| {}", ci + 1, lines[ci]); }
                        println!();
                    }
                }
            }
            if file_matches > 0 { files_with_matches += 1; total_matches += file_matches; }
        }
    });
    if only_count { println!("Matches: {} in {} files", total_matches, files_with_matches); }
    else { eprintln!("  {} matches in {} files", total_matches, files_with_matches); }
    Ok(())
}

fn search_by_name(dir: &Path, pattern: &str, file_type: Option<&str>) -> anyhow::Result<()> {
    let ext_filter = file_type.and_then(|t| ext_for_type(t));
    let mut results: Vec<(PathBuf, u64)> = Vec::new();
    // glob 模式检测: 含 * ? [ 则用 glob 匹配, 否则字面 contains
    let is_glob = pattern.contains('*') || pattern.contains('?') || pattern.contains('[');
    // 预编译 glob -> regex
    let glob_re: Option<regex::Regex> = if is_glob {
        let mut s = String::from("^");
        for c in pattern.chars() {
            match c {
                '*' => s.push_str(".*"),
                '?' => s.push('.'),
                '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | '\\' => {
                    s.push('\\');
                    s.push(c);
                }
                '[' => s.push_str("\\["),
                ']' => s.push_str("\\]"),
                _ => s.push(c),
            }
        }
        s.push('$');
        regex::Regex::new(&s).ok()
    } else {
        None
    };
    let pattern_lower = pattern.to_lowercase();
    walk_files(dir, None, &mut |path| {
        if let Some(ext) = ext_filter {
            if path.extension().and_then(|e| e.to_str()) != Some(ext) { return; }
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let matched = if let Some(ref re) = glob_re {
            re.is_match(name)
        } else {
            name.to_lowercase().contains(&pattern_lower)
        };
        if matched {
            if let Ok(meta) = path.metadata() { results.push((path.to_path_buf(), meta.len())); }
        }
    });
    results.sort_by_key(|(_, size)| *size);
    println!("Found {} files matching '{}':", results.len(), pattern);
    for (path, size) in &results {
        let s = if *size > 1048576 { format!("{:.1} MB", *size as f64 / 1048576.0) }
                else if *size > 1024 { format!("{:.1} KB", *size as f64 / 1024.0) }
                else { format!("{} B", size) };
        println!("  {} ({})", path.display(), s);
    }
    Ok(())
}

fn stats_by_type(dir: &Path) -> anyhow::Result<()> {
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut total_files = 0usize; let mut total_lines = 0usize;
    walk_files(dir, None, &mut |path| {
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_string()).unwrap_or_else(|| "_none".to_string());
        if let Ok(raw) = fs::read(path) {
            let sig = FileSignature::detect(&raw);
            let content = to_utf8_lf(&raw, &sig);
            let lines = content.lines().count();
            let entry = stats.entry(ext).or_insert((0, 0));
            entry.0 += 1; entry.1 += lines;
            total_files += 1; total_lines += lines;
        }
    });
    println!("Code Stats for: {}", dir.display());
    println!("{:<10} {:>8} {:>10}", "Type", "Files", "Lines");
    println!("{:-<10} {:-<8} {:-<10}", "", "", "");
    for (ext, (f, l)) in &stats { println!("{:<10} {:>8} {:>10}", ext, f, l); }
    println!("{:-<10} {:-<8} {:-<10}", "", "", "");
    println!("{:<10} {:>8} {:>10}", "TOTAL", total_files, total_lines);
    Ok(())
}

fn run_replace(dir: &Path, name_pattern: Option<&str>, file_type: Option<&str>, old: &str, new: &str, preview: bool) -> anyhow::Result<()> {
    let ext_filter = file_type.and_then(|t| ext_for_type(t));
    let pattern_lower = name_pattern.map(|p| p.to_lowercase());
    let mut changed_files = 0usize;
    let mut total_replacements = 0usize;

    walk_files(dir, ext_filter, &mut |path| {
        if let Some(ref pat) = pattern_lower {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.to_lowercase().contains(pat) { return; }
        }

        let raw = match fs::read(path) {
            Ok(b) => b,
            Err(_) => return,
        };
        let sig = FileSignature::detect(&raw);
        let content = to_utf8_lf(&raw, &sig);

        let new_content = content.replace(old, new);
        if new_content == content { return; }

        changed_files += 1;
        let count = content.matches(old).count();
        total_replacements += count;

        if preview {
            println!("[{}] {} changes", count, path.display());
            for (i, line) in content.lines().enumerate() {
                if line.contains(old) {
                    let new_line = line.replace(old, new);
                    println!("  -{:<4}| {}", i + 1, line);
                    println!("  +{:<4}| {}", i + 1, new_line);
                }
            }
        } else {
            if let Err(e) = fs::write(path, new_content.as_bytes()) {
                eprintln!("  error writing {}: {}", path.display(), e);
                return;
            }
            println!("  replaced {} occurrences in {}", count, path.display());
        }
    });

    if preview {
        println!("\n  [preview] would change {} files, {} replacements", changed_files, total_replacements);
    } else {
        println!("  changed {} files, {} replacements total", changed_files, total_replacements);
    }
    Ok(())
}
