use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Write, BufWriter};
use crate::signature::{FileSignature, to_utf8_lf};
use rayon::prelude::*;
use std::sync::Mutex;
use regex::Regex;

/// Enhanced search — cross-encoding grep with context
///
/// Signature v2: added max_results, head, offset, jsonl, no_ignore, json (alias json_output)
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
        return run_remote(pattern, path, context, file_type, only_count, invert, json_output, remote);
    }

    if !path.exists() { anyhow::bail!("Path not found: {}", path.display()); }

    let ext_filter: Option<&str> = file_type.and_then(|t| match t {
        "rs" | "py" | "md" | "toml" | "json" | "js" | "ts" | "c" | "h" | "go" | "sh" => Some(t),
        "yaml" | "yml" => Some("yml"),
        _ => None,
    });

    fn collect_files(dir: &Path, ext_filter: Option<&str>, no_ignore: bool, files: &mut Vec<PathBuf>) {
        let ignore_dirs: &[&str] = if no_ignore { &[] } else { &[".git", "target", "node_modules", "vendor"] };
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !no_ignore && (name.starts_with('.') || ignore_dirs.contains(&name)) { continue; }
                    collect_files(&p, ext_filter, no_ignore, files);
                } else if p.is_file() {
                    if let Some(ext) = ext_filter {
                        if p.extension().and_then(|e| e.to_str()) != Some(ext) { continue; }
                    }
                    // Skip binary files (heuristic: null byte ratio > 5% in first 8KB)
                    if let Ok(bytes) = fs::read(&p) {
                        let sample_len = bytes.len().min(8192);
                        let nulls = bytes[..sample_len].iter().filter(|&&b| b == 0).count();
                        if nulls * 20 > sample_len { continue; }
                    }
                    files.push(p);
                }
            }
        }
    }

    let mut all_results: Vec<(String, usize, String, Vec<(usize, String, bool)>)> = Vec::new();

    if path.is_dir() {
        let mut files = Vec::new();
        collect_files(path, ext_filter, no_ignore, &mut files);
        let results_mutex = Mutex::new(Vec::<(String, usize, String, Vec<(usize, String, bool)>)>::new());
        files.par_iter().for_each(|p| {
            let raw = match fs::read(p) { Ok(b) => b, Err(_) => return };
            let sig = FileSignature::detect(&raw);
            let content = to_utf8_lf(&raw, &sig);
            let lines: Vec<&str> = content.lines().collect();
            let mut file_results = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let matched = if let Some(ref re) = compiled_re {
                    re.is_match(line)
                } else {
                    line.to_lowercase().contains(&pattern_lower)
                };
                if (matched && !invert) || (!matched && invert) {
                    let mut ctx = Vec::new();
                    let start = if i >= context { i - context } else { 0 };
                    for ci in start..i { ctx.push((ci + 1, lines[ci].to_string(), false)); }
                    ctx.push((i + 1, line.to_string(), true));
                    let end = (i + context + 1).min(lines.len());
                    for ci in (i + 1)..end { ctx.push((ci + 1, lines[ci].to_string(), false)); }
                    let display_path = p.strip_prefix(path).unwrap_or(p).to_string_lossy().to_string();
                    file_results.push((display_path, i + 1, line.to_string(), ctx));
                }
            }
            if !file_results.is_empty() {
                results_mutex.lock().unwrap().extend(file_results);
            }
        });
        all_results.extend(results_mutex.into_inner().unwrap());
    } else if path.is_file() {
        let raw = fs::read(path)?;
        let sig = FileSignature::detect(&raw);
        let content = to_utf8_lf(&raw, &sig);
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let matched = if let Some(ref re) = compiled_re {
                re.is_match(line)
            } else {
                line.to_lowercase().contains(&pattern_lower)
            };
            if (matched && !invert) || (!matched && invert) {
                let mut ctx = Vec::new();
                let start = if i >= context { i - context } else { 0 };
                for ci in start..i { ctx.push((ci + 1, lines[ci].to_string(), false)); }
                ctx.push((i + 1, line.to_string(), true));
                let end = (i + context + 1).min(lines.len());
                for ci in (i + 1)..end { ctx.push((ci + 1, lines[ci].to_string(), false)); }
                all_results.push((path.to_string_lossy().to_string(), i + 1, line.to_string(), ctx));
            }
        }
    }

    all_results.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Apply offset / head / max_results pagination
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
            let v = serde_json::json!({"path": f, "line": ln, "text": txt});
            writeln!(out, "{}", v)?;
        }
    } else if json_output {
        let j: Vec<serde_json::Value> = all_results.iter().map(|(f, ln, txt, _)| {
            serde_json::json!({"path": f, "line": ln, "text": txt})
        }).collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&j)?);
    } else if only_count {
        let files: std::collections::HashSet<&str> = all_results.iter().map(|(f, _, _, _)| f.as_str()).collect();
        writeln!(out, "{} matches in {} files", all_results.len(), files.len())?;
    } else {
        let mut current_file = String::new();
        for (f, _ln, _txt, ctx) in &all_results {
            if *f != current_file {
                writeln!(out, "\n{}:", f)?;
                current_file = f.clone();
            }
            for (cln, ct, is_match) in ctx {
                if *is_match { writeln!(out, "→ {:<6}| {}", cln, ct)?; }
                else { writeln!(out, "  {:<6}| {}", cln, ct)?; }
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
    context: usize,
    file_type: Option<&str>,
    only_count: bool,
    invert: bool,
    json_output: bool,
    remote: &crate::remote::RemoteChannel,
) -> anyhow::Result<()> {
    let mut cmd = String::from("grep -n");

    if invert { cmd.push_str(" -v"); }
    cmd.push_str(&format!(" '{}'", pattern.replace("'", "'\"'\"'")));
    cmd.push_str(&format!(" '{}'", path.display()));

    if path.is_dir() || remote.exec(&format!("test -d '{}'", path.display())).is_ok() {
        cmd = cmd.replace("grep -n", "grep -rn");
    }

    let output = match remote.exec(&cmd) {
        Ok(out) => out,
        Err(_) => String::new(),
    };

    if only_count {
        let count = output.lines().count();
        println!("{} matches", count);
        return Ok(());
    }

    if json_output {
        let results: Vec<serde_json::Value> = output.lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let file = parts[0];
                    let line_info: Vec<&str> = parts[1].splitn(2, ':').collect();
                    if line_info.len() == 2 {
                        let line_num = line_info[0].parse::<usize>().ok()?;
                        let text = line_info[1];
                        return Some(serde_json::json!({
                            "path": file,
                            "line": line_num,
                            "text": text
                        }));
                    }
                }
                None
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        let mut current_file = String::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                let file = parts[0];
                if file != current_file {
                    println!("\n{}:", file);
                    current_file = file.to_string();
                }
                println!("{}", parts[1]);
            }
        }
        let count = output.lines().count();
        eprintln!("  {} matches", count);
    }

    Ok(())
}
