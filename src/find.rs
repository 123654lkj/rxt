use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Write, BufWriter};
use crate::signature::{FileSignature, to_utf8_lf};
use std::collections::BTreeMap;
use regex::Regex;
use rayon::prelude::*;

/// 路径感观：用于 `rxt find /dir --name '*.rs'`（query 位当目录）
fn looks_like_path(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with(".\\")
        || s.starts_with("~/")
        || s.starts_with("~\\")
    {
        return true;
    }
    let b = s.as_bytes();
    // Windows 盘符 C:\ 或 C:/
    if b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        return true;
    }
    s.contains('/') || s.contains('\\')
}

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
    json_output: bool,
    max_results: Option<usize>,
    head: Option<usize>,
    offset: usize,
    remote: Option<&mut crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    // 远程：交给远端 rxt 算完回传（与 pack 同模式）
    if let Some(rc) = remote {
        let mut args: Vec<String> = vec!["find".into()];
        let (eff_query, eff_path) = resolve_query_path(query, path, name_pattern, do_stats, replace);
        if let Some(q) = eff_query {
            args.push(q.to_string());
        }
        if let Some(p) = path {
            args.push("--path".into());
            args.push(p.display().to_string());
        } else if eff_query.is_none() && eff_path.as_os_str() != std::ffi::OsStr::new(".") {
            args.push("--path".into());
            args.push(eff_path.display().to_string());
        }
        if let Some(n) = name_pattern {
            args.push("--name".into());
            args.push(n.to_string());
        }
        if let Some(t) = file_type {
            args.push("--type".into());
            args.push(t.to_string());
        }
        args.push("--context".into());
        args.push(context.to_string());
        if case_sensitive {
            args.push("--case-sensitive".into());
        }
        if count {
            args.push("--count".into());
        }
        if do_stats {
            args.push("--stats".into());
        }
        if let Some(old) = replace {
            args.push("--replace".into());
            args.push(old.to_string());
        }
        if let Some(nw) = replace_with {
            args.push("--with".into());
            args.push(nw.to_string());
        }
        if preview {
            args.push("--preview".into());
        }
        if use_regex {
            args.push("--regex".into());
        }
        if json_output {
            args.push("--json".into());
        }
        if let Some(m) = max_results {
            args.push("--max-results".into());
            args.push(m.to_string());
        }
        if let Some(h) = head {
            args.push("--head".into());
            args.push(h.to_string());
        }
        if offset > 0 {
            args.push("--offset".into());
            args.push(offset.to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        if let Some(out) = rc.try_exec_rxt(&arg_refs) {
            let mut stdout = io::stdout().lock();
            let _ = crate::common::maybe_write_bom(&mut stdout);
            let _ = write!(stdout, "{}", out);
            if !out.ends_with('\n') {
                let _ = writeln!(stdout);
            }
            return Ok(());
        }
        anyhow::bail!("远端无 rxt，无法 find。请先安装 rxt 0.8.6+ 或改用本地路径。");
    }

    let (eff_query, search_dir_owned) = resolve_query_path(query, path, name_pattern, do_stats, replace);
    let search_dir = search_dir_owned.as_path();

    let compiled_re = if use_regex {
        match eff_query {
            Some(q) => match Regex::new(q) {
                Ok(r) => Some(r),
                Err(e) => anyhow::bail!(
                    "无效正则: {}\n示例: rxt find 'fn\\s+main' --regex -p src",
                    e
                ),
            },
            None => None,
        }
    } else {
        None
    };

    if let (Some(old), Some(new)) = (replace, replace_with) {
        return run_replace(search_dir, name_pattern, file_type, old, new, preview);
    }

    if do_stats {
        return stats_by_type(search_dir, json_output);
    }

    if let Some(pattern) = name_pattern {
        return search_by_name(search_dir, pattern, file_type, json_output);
    }

    if let Some(q) = eff_query {
        let ext_filter = file_type.and_then(|t| ext_for_type(t));
        return search_content(
            search_dir,
            q,
            ext_filter,
            context,
            case_sensitive,
            count,
            &compiled_re,
            json_output,
            max_results,
            head,
            offset,
        );
    }

    anyhow::bail!(
        "缺少动作。用法示例:\n  \
         rxt find TODO -p src\n  \
         rxt find /path/to/dir --name '*.rs'\n  \
         rxt find /path/to/dir -n '*.md'   # -n/--name/-name 均可\n  \
         rxt find --stats -p .\n  \
         rxt --host huhu find /home/huhu --name '*.md'"
    )
}

/// 当 `--name` / `--stats` / `--replace` 且未给 `--path` 时，把像路径的 query 提升为目录。
fn resolve_query_path<'a>(
    query: Option<&'a str>,
    path: Option<&'a Path>,
    name_pattern: Option<&str>,
    do_stats: bool,
    replace: Option<&str>,
) -> (Option<&'a str>, PathBuf) {
    if let Some(p) = path {
        return (query, p.to_path_buf());
    }
    let pathish = name_pattern.is_some() || do_stats || replace.is_some();
    if pathish {
        if let Some(q) = query {
            if looks_like_path(q) {
                return (None, PathBuf::from(q));
            }
        }
    }
    (query, PathBuf::from("."))
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

/// Iterate files: handles both single-file and directory cases (fixes "single-file mode broken" bug).
fn for_each_file(target: &Path, ext_filter: Option<&str>, cb: &mut impl FnMut(&Path)) {
    if !target.exists() { return; }
    if target.is_file() {
        // Single-file mode — apply extension filter and text-file check
        if let Some(ext) = ext_filter {
            if target.extension().and_then(|e| e.to_str()) == Some(ext) {
                cb(target);
            }
        } else if is_text_file(target) {
            cb(target);
        }
        return;
    }
    if !target.is_dir() { return; }
    walk_files(target, ext_filter, cb);
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

fn search_content(
    dir: &Path,
    query: &str,
    ext_filter: Option<&str>,
    _context: usize,
    case_sensitive: bool,
    only_count: bool,
    compiled_re: &Option<Regex>,
    json_output: bool,
    max_results: Option<usize>,
    head: Option<usize>,
    offset: usize,
) -> anyhow::Result<()> {
    let query_lower = query.to_lowercase();
    let mut files = Vec::new();
    for_each_file(dir, ext_filter, &mut |path| files.push(path.to_path_buf()));
    let query_owned = query.to_string();
    let compiled = compiled_re.clone();
    let mut results: Vec<(PathBuf, usize, String)> = files
        .par_iter()
        .filter_map(|path| {
            let raw = fs::read(path).ok()?;
            let sig = FileSignature::detect(&raw);
            let content = to_utf8_lf(&raw, &sig);
            let mut hits = Vec::new();
            for (i, line) in content.lines().enumerate() {
                let matched = if let Some(ref re) = compiled {
                    re.is_match(line)
                } else if case_sensitive {
                    line.contains(&query_owned)
                } else {
                    line.to_lowercase().contains(&query_lower)
                };
                if matched {
                    hits.push((path.clone(), i + 1, line.to_string()));
                }
            }
            if hits.is_empty() { None } else { Some(hits) }
        })
        .flatten()
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Apply pagination
    let mut results = results;
    if offset > 0 {
        if offset >= results.len() { results.clear(); }
        else { results = results.split_off(offset); }
    }
    if let Some(h) = head { results.truncate(h); }
    if let Some(m) = max_results { results.truncate(m); }

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    if json_output {
        let j: Vec<serde_json::Value> = results.iter().map(|(p, ln, t)| {
            serde_json::json!({"path": p.display().to_string(), "line": ln, "text": t})
        }).collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&j)?)?;
        eprintln!("  {} matches", results.len());
    } else if only_count {
        writeln!(out, "Matches: {} (in {} files)", results.len(), {
            let mut s: std::collections::HashSet<&Path> = std::collections::HashSet::new();
            for (p, _, _) in &results { s.insert(p); }
            s.len()
        })?;
    } else {
        let mut current_file: Option<PathBuf> = None;
        for (path, ln, text) in &results {
            if current_file.as_ref() != Some(path) {
                writeln!(out, "\n{}:", path.display())?;
                current_file = Some(path.clone());
            }
            writeln!(out, "  {:<6}| {}", ln, text)?;
        }
        eprintln!("  {} matches", results.len());
    }
    out.flush()?;
    Ok(())
}

fn search_by_name(dir: &Path, pattern: &str, file_type: Option<&str>, json_output: bool) -> anyhow::Result<()> {
    let ext_filter = file_type.and_then(|t| ext_for_type(t));
    let mut results: Vec<(PathBuf, u64)> = Vec::new();
    let is_glob = pattern.contains('*') || pattern.contains('?') || pattern.contains('[');
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
    for_each_file(dir, ext_filter, &mut |path| {
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

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    if json_output {
        let j: Vec<serde_json::Value> = results.iter().map(|(p, s)| {
            serde_json::json!({"path": p.display().to_string(), "size_bytes": s})
        }).collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&j)?)?;
    } else {
        writeln!(out, "Found {} files matching '{}':", results.len(), pattern)?;
        for (path, size) in &results {
            let s = if *size > 1048576 { format!("{:.1} MB", *size as f64 / 1048576.0) }
                    else if *size > 1024 { format!("{:.1} KB", *size as f64 / 1024.0) }
                    else { format!("{} B", size) };
            writeln!(out, "  {} ({})", path.display(), s)?;
        }
    }
    out.flush()?;
    Ok(())
}

fn stats_by_type(dir: &Path, json_output: bool) -> anyhow::Result<()> {
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut total_files = 0usize;
    let mut total_lines = 0usize;
    for_each_file(dir, None, &mut |path| {
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_string()).unwrap_or_else(|| "_none".to_string());
        if let Ok(raw) = fs::read(path) {
            let sig = FileSignature::detect(&raw);
            let content = to_utf8_lf(&raw, &sig);
            let lines = content.lines().count();
            let entry = stats.entry(ext).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += lines;
            total_files += 1;
            total_lines += lines;
        }
    });

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    if json_output {
        let mut by_type_obj = serde_json::Map::new();
        for (ext, (f, l)) in &stats {
            by_type_obj.insert(ext.clone(), serde_json::json!({"files": f, "lines": l}));
        }
        let v = serde_json::json!({
            "path": dir.display().to_string(),
            "by_type": by_type_obj,
            "total": {"files": total_files, "lines": total_lines}
        });
        writeln!(out, "{}", serde_json::to_string_pretty(&v)?)?;
    } else {
        writeln!(out, "Code Stats for: {}", dir.display())?;
        writeln!(out, "{:<10} {:>8} {:>10}", "Type", "Files", "Lines")?;
        writeln!(out, "{:-<10} {:-<8} {:-<10}", "", "", "")?;
        for (ext, (f, l)) in &stats {
            writeln!(out, "{:<10} {:>8} {:>10}", ext, f, l)?;
        }
        writeln!(out, "{:-<10} {:-<8} {:-<10}", "", "", "")?;
        writeln!(out, "{:<10} {:>8} {:>10}", "TOTAL", total_files, total_lines)?;
    }
    out.flush()?;
    Ok(())
}

fn run_replace(dir: &Path, name_pattern: Option<&str>, file_type: Option<&str>, old: &str, new: &str, preview: bool) -> anyhow::Result<()> {
    let ext_filter = file_type.and_then(|t| ext_for_type(t));
    let pattern_lower = name_pattern.map(|p| p.to_lowercase());
    let mut changed_files = 0usize;
    let mut total_replacements = 0usize;

    for_each_file(dir, ext_filter, &mut |path| {
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
