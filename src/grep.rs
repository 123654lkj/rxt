use crate::signature::{to_utf8_lf, FileSignature};
use rayon::prelude::*;
use regex::Regex;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// 单条搜索结果
type Match = (String, usize, String, Vec<(usize, String, bool)>);

/// 在文本内容中搜索, 返回匹配结果 (抽取自原来的单文件/目录两处重复逻辑)
fn search_in_content(
    content: &str,
    compiled_re: &Option<Regex>,
    pattern_lower: &str,
    context: usize,
    invert: bool,
    display_path: &str,
) -> Vec<Match> {
    let lines: Vec<&str> = content.lines().collect();
    let mut results = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let matched = match compiled_re {
            Some(re) => re.is_match(line),
            None => line.to_lowercase().contains(pattern_lower),
        };
        if (matched && !invert) || (!matched && invert) {
            let mut ctx = Vec::new();
            let start = i.saturating_sub(context);
            for ci in start..i {
                ctx.push((ci + 1, lines[ci].to_string(), false));
            }
            ctx.push((i + 1, line.to_string(), true));
            let end = (i + context + 1).min(lines.len());
            for ci in (i + 1)..end {
                ctx.push((ci + 1, lines[ci].to_string(), false));
            }
            results.push((display_path.to_string(), i + 1, line.to_string(), ctx));
        }
    }
    results
}

/// Enhanced search — cross-encoding grep with context
pub fn run(
    pattern: &str,
    path: &Path,
    context: usize,
    file_type: Option<&str>,
    only_count: bool,
    invert: bool,
    json_output: bool,
    use_regex: bool,
    max_results: Option<usize>,
    head: Option<usize>,
    offset: usize,
    jsonl: bool,
    no_ignore: bool,
    remote: Option<&crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    let compiled_re = if use_regex {
        match Regex::new(pattern) {
            Ok(r) => Some(r),
            Err(e) => anyhow::bail!("Invalid regex '{}': {}", pattern, e),
        }
    } else {
        None
    };
    let pattern_lower = pattern.to_lowercase();

    if let Some(remote) = remote {
        return run_remote(
            pattern,
            path,
            context,
            only_count,
            invert,
            json_output,
            remote,
        );
    }
    if !path.exists() {
        anyhow::bail!("Path not found: {}", path.display());
    }

    let ext_filter: Option<&str> = file_type.and_then(|t| match t {
        "rs" | "py" | "md" | "toml" | "json" | "js" | "ts" | "c" | "h" | "go" | "sh" => Some(t),
        "yaml" | "yml" => Some("yml"),
        _ => None,
    });

    fn collect_files(
        dir: &Path,
        ext_filter: Option<&str>,
        no_ignore: bool,
        files: &mut Vec<PathBuf>,
    ) {
        let ignore_dirs: &[&str] = if no_ignore {
            &[]
        } else {
            &[".git", "target", "node_modules", "vendor"]
        };
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !no_ignore && (name.starts_with('.') || ignore_dirs.contains(&name)) {
                        continue;
                    }
                    collect_files(&p, ext_filter, no_ignore, files);
                } else if p.is_file() {
                    if let Some(ext) = ext_filter {
                        if p.extension().and_then(|e| e.to_str()) != Some(ext) {
                            continue;
                        }
                    }
                    files.push(p);
                }
            }
        }
    }

    // 统一的搜索逻辑: 单文件和目录都走 search_in_content
    let all_results: Vec<Match> = if path.is_dir() {
        let mut files = Vec::new();
        collect_files(path, ext_filter, no_ignore, &mut files);

        // v0.5: 用 rayon map + flatten 替代 Mutex 锁 (消灭并行锁竞争)
        files
            .par_iter()
            .filter_map(|p| {
                let raw = fs::read(p).ok()?;
                let sample = raw.len().min(8192);
                let nulls = raw[..sample].iter().filter(|&&b| b == 0).count();
                if sample > 0 && nulls * 20 > sample {
                    return None;
                }
                let sig = FileSignature::detect(&raw);
                let content = to_utf8_lf(&raw, &sig);
                let display = p
                    .strip_prefix(path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string();
                let results = search_in_content(
                    &content,
                    &compiled_re,
                    &pattern_lower,
                    context,
                    invert,
                    &display,
                );
                if results.is_empty() {
                    None
                } else {
                    Some(results)
                }
            })
            .flatten()
            .collect()
    } else {
        let raw = fs::read(path)?;
        let sig = FileSignature::detect(&raw);
        let content = to_utf8_lf(&raw, &sig);
        search_in_content(
            &content,
            &compiled_re,
            &pattern_lower,
            context,
            invert,
            &path.to_string_lossy(),
        )
    };

    // 后续: 排序 + 分页 + 输出 (不变)
    let mut all_results = all_results;
    all_results.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    if offset > 0 {
        if offset >= all_results.len() {
            all_results.clear();
        } else {
            all_results = all_results.split_off(offset);
        }
    }
    if let Some(h) = head {
        all_results.truncate(h);
    }
    if let Some(m) = max_results {
        all_results.truncate(m);
    }

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    if jsonl {
        for (f, ln, txt, _) in &all_results {
            writeln!(
                out,
                "{}",
                serde_json::json!({"path": f, "line": ln, "text": txt})
            )?;
        }
    } else if json_output {
        let j: Vec<_> = all_results
            .iter()
            .map(|(f, ln, txt, _)| serde_json::json!({"path": f, "line": ln, "text": txt}))
            .collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&j)?)?;
    } else if only_count {
        let files: std::collections::HashSet<&str> =
            all_results.iter().map(|(f, _, _, _)| f.as_str()).collect();
        writeln!(
            out,
            "{} matches in {} files",
            all_results.len(),
            files.len()
        )?;
    } else {
        let mut current_file = String::new();
        for (f, _ln, _txt, ctx) in &all_results {
            if *f != current_file {
                writeln!(out, "\n{}:", f)?;
                current_file = f.clone();
            }
            for (cln, ct, is_match) in ctx {
                if *is_match {
                    writeln!(out, "-> {:<6}| {}", cln, ct)?;
                } else {
                    writeln!(out, "   {:<6}| {}", cln, ct)?;
                }
            }
            writeln!(out)?;
        }
        eprintln!("  {} matches", all_results.len());
    }
    out.flush()?;
    Ok(())
}

fn run_remote(
    pattern: &str,
    path: &Path,
    _context: usize,
    only_count: bool,
    invert: bool,
    json_output: bool,
    remote: &crate::remote::RemoteChannel,
) -> anyhow::Result<()> {
    let mut cmd = String::from("grep -n");
    if invert {
        cmd.push_str(" -v");
    }
    cmd.push_str(&format!(" '{}'", pattern.replace("'", "'\"'\"'")));
    cmd.push_str(&format!(" '{}'", path.display()));
    if path.is_dir()
        || remote
            .exec(&format!("test -d '{}'", path.display()))
            .is_ok()
    {
        cmd = cmd.replace("grep -n", "grep -rn");
    }
    let output = remote.exec(&cmd).unwrap_or_default();

    if only_count {
        println!("{} matches", output.lines().count());
        return Ok(());
    }
    if json_output {
        let results: Vec<_> = output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() != 3 {
                    return None;
                }
                let ln = parts[1].parse::<usize>().ok()?;
                Some(serde_json::json!({ "path": parts[0], "line": ln, "text": parts[2] }))
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        let mut current_file = String::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                if parts[0] != current_file {
                    println!("\n{}:", parts[0]);
                    current_file = parts[0].to_string();
                }
                println!("{}", parts[1]);
            }
        }
        eprintln!("  {} matches", output.lines().count());
    }
    Ok(())
}
