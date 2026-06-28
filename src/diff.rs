use std::path::Path;
use std::fs;
use std::collections::HashSet;
use std::io::{self, Write, BufWriter};
use std::env;

/// 差异对比 — 文件/目录/Git diff
///
/// Output modes:
/// - unified diff (default): standard unified format
/// - side-by-side (`--side-by-side`): two columns with line numbers
/// - JSON (`--json`): structured hunks for AI parsing
pub fn run(first: &Path, second: Option<&Path>, context: usize, only_stat: bool, ai_mode: bool, side_by_side: bool, json_output: bool) -> anyhow::Result<()> {
    match second {
        None => git_diff(context, only_stat, side_by_side, json_output),
        Some(s) if first.is_dir() && s.is_dir() => dir_diff(first, s, context, only_stat, ai_mode, side_by_side, json_output),
        Some(s) if first.is_file() && s.is_file() => file_diff(first, s, context, only_stat, ai_mode, side_by_side, json_output),
        _ => anyhow::bail!("Both paths must be files or both directories"),
    }
}

fn git_diff(context: usize, only_stat: bool, side_by_side: bool, json_output: bool) -> anyhow::Result<()> {
    let output = std::process::Command::new("git").args(["diff", &format!("-U{}", context)]).output()
        .map_err(|e| anyhow::anyhow!("git not available: {}", e))?;
    if !output.status.success() { eprintln!("  (no git repo or no changes)"); return Ok(()); }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() { println!("No changes"); return Ok(()); }
    if only_stat {
        let files = stdout.lines().filter(|l| l.starts_with("diff --git")).count();
        let adds = stdout.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
        let dels = stdout.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
        println!("{} files changed, +{} / -{}", files, adds, dels);
    } else if side_by_side {
        // Parse git diff and render side-by-side (best-effort)
        println!("(side-by-side git diff not fully supported; showing unified)");
        println!("{}", stdout);
    } else if json_output {
        let files: Vec<&str> = stdout.lines().filter(|l| l.starts_with("diff --git")).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"type":"git","files":files.len(),"raw":stdout}))?);
    } else { println!("{}", stdout); }
    Ok(())
}

fn file_diff(first: &Path, second: &Path, context: usize, only_stat: bool, ai_mode: bool, side_by_side: bool, json_output: bool) -> anyhow::Result<()> {
    let a_raw = fs::read(first)?;
    let b_raw = fs::read(second)?;
    let a = String::from_utf8_lossy(&a_raw);
    let b = String::from_utf8_lossy(&b_raw);
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();
    let changes = compute_diff(&a_lines, &b_lines);
    if changes.is_empty() { println!("Files are identical"); return Ok(()); }
    let adds = changes.iter().filter(|c| c.typ == 'a').count();
    let dels = changes.iter().filter(|c| c.typ == 'd').count();
    if only_stat {
        println!("{} additions, {} deletions", adds, dels);
        return Ok(());
    }
    if side_by_side {
        return file_diff_side_by_side(first, second, &a_lines, &b_lines, &changes, context);
    }
    if json_output {
        return file_diff_json(first, second, &a_lines, &b_lines, &changes, context, adds, dels);
    }
    if ai_mode {
        return file_diff_ai(first, second, &a_lines, &b_lines, &changes, adds, dels);
    }

    println!("--- {}", first.display());
    println!("+++ {}", second.display());

    let change_idx: Vec<usize> = changes.iter().enumerate()
        .filter(|(_, c)| c.typ != 'e')
        .map(|(i, _)| i)
        .collect();

    if change_idx.is_empty() {
        println!("Files are identical");
        return Ok(());
    }

    let mut hunk_starts = Vec::new();
    let mut hunk_ends = Vec::new();
    let mut hs = change_idx[0];
    let mut he = change_idx[0];
    for &ci in &change_idx[1..] {
        if ci - he > 2 * context {
            hunk_starts.push(hs);
            hunk_ends.push(he);
            hs = ci;
        }
        he = ci;
    }
    hunk_starts.push(hs);
    hunk_ends.push(he);

    let n = changes.len();
    for (&hs, &he) in hunk_starts.iter().zip(hunk_ends.iter()) {
        let ctx_lo = hs.saturating_sub(context);
        let ctx_hi = (he + context + 1).min(n);

        let a_start = changes[ctx_lo].old_idx.max(1);
        let b_start = changes[ctx_lo].new_idx.max(1);
        let mut a_cnt = 0usize;
        let mut b_cnt = 0usize;
        for ci in ctx_lo..ctx_hi {
            if changes[ci].typ != 'a' { a_cnt += 1; }
            if changes[ci].typ != 'd' { b_cnt += 1; }
        }
        println!("@@ -{},{} +{},{} @@", a_start, a_cnt, b_start, b_cnt);

        for ci in ctx_lo..ctx_hi {
            match changes[ci].typ {
                'a' => println!("+{}", b_lines[changes[ci].new_idx - 1]),
                'd' => println!("-{}", a_lines[changes[ci].old_idx - 1]),
                _   => println!(" {}", a_lines[changes[ci].old_idx - 1]),
            }
        }
    }
    Ok(())
}

/// Render file diff as two columns side-by-side.
///
/// Each line of output:
/// ```
/// <a_marker> <a_line_num>  <a_content> │ <b_line_num> <b_marker> <b_content>
/// ```
/// Markers: ' ' unchanged, '-' deleted (left only), '+' added (right only)
fn file_diff_side_by_side(
    first: &Path,
    second: &Path,
    a_lines: &[&str],
    b_lines: &[&str],
    changes: &[Change],
    context: usize,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    // Compute terminal width, split 50/50 with min 40 cols per side
    let term_width = terminal_width().unwrap_or(120);
    let side_width = (term_width / 2).max(40);
    let left_w = side_width - 8;  // account for line num + marker
    let right_w = side_width - 8;

    writeln!(out, "--- {} │ +++ {}", first.display(), second.display())?;
    writeln!(out, "{}", "─".repeat(term_width))?;

    let change_idx: Vec<usize> = changes.iter().enumerate()
        .filter(|(_, c)| c.typ != 'e')
        .map(|(i, _)| i)
        .collect();

    if change_idx.is_empty() {
        writeln!(out, "(files identical)")?;
        return Ok(());
    }

    // Group into hunks
    let mut hunk_starts = Vec::new();
    let mut hunk_ends = Vec::new();
    let mut hs = change_idx[0];
    let mut he = change_idx[0];
    for &ci in &change_idx[1..] {
        if ci - he > 2 * context {
            hunk_starts.push(hs);
            hunk_ends.push(he);
            hs = ci;
        }
        he = ci;
    }
    hunk_starts.push(hs);
    hunk_ends.push(he);

    let n = changes.len();
    for (&hs, &he) in hunk_starts.iter().zip(hunk_ends.iter()) {
        let ctx_lo = hs.saturating_sub(context);
        let ctx_hi = (he + context + 1).min(n);

        // Hunk header
        let a_start = changes[ctx_lo].old_idx.max(1);
        let b_start = changes[ctx_lo].new_idx.max(1);
        let mut a_cnt = 0;
        let mut b_cnt = 0;
        for ci in ctx_lo..ctx_hi {
            if changes[ci].typ != 'a' { a_cnt += 1; }
            if changes[ci].typ != 'd' { b_cnt += 1; }
        }
        writeln!(out, "@@ -{},{} +{},{} @@", a_start, a_cnt, b_start, b_cnt)?;

        // Walk through hunk, aligning changes
        let mut i = ctx_lo;
        while i < ctx_hi {
            let ch = &changes[i];
            match ch.typ {
                'e' => {
                    // Equal: both sides same
                    let line = a_lines[ch.old_idx - 1];
                    write_side_by_side_line(&mut out, ' ', ch.old_idx, line, ch.new_idx, ' ', line, left_w, right_w)?;
                    i += 1;
                }
                'd' => {
                    // Deleted: only left, advance i until paired addition or next equal
                    // Collect consecutive deletes
                    let mut dels: Vec<&Change> = vec![ch];
                    while i + 1 < ctx_hi && changes[i + 1].typ == 'd' {
                        dels.push(&changes[i + 1]);
                        i += 1;
                    }
                    // If next is an add, pair them up
                    if i + 1 < ctx_hi && changes[i + 1].typ == 'a' {
                        let mut adds: Vec<&Change> = vec![&changes[i + 1]];
                        i += 1;
                        while i + 1 < ctx_hi && changes[i + 1].typ == 'a' {
                            adds.push(&changes[i + 1]);
                            i += 1;
                        }
                        let n = dels.len().max(adds.len());
                        for j in 0..n {
                            let left = dels.get(j).map(|c| (c.old_idx, a_lines[c.old_idx - 1]));
                            let right = adds.get(j).map(|c| (c.new_idx, b_lines[c.new_idx - 1]));
                            match (left, right) {
                                (Some((la, ll)), Some((rb, rl))) => {
                                    write_side_by_side_line(&mut out, '-', la, ll, rb, '+', rl, left_w, right_w)?;
                                }
                                (Some((la, ll)), None) => {
                                    write_side_by_side_line(&mut out, '-', la, ll, 0, ' ', "", left_w, right_w)?;
                                }
                                (None, Some((rb, rl))) => {
                                    write_side_by_side_line(&mut out, ' ', 0, "", rb, '+', rl, left_w, right_w)?;
                                }
                                _ => {}
                            }
                        }
                        i += 1;
                    } else {
                        // Pure deletes
                        for d in &dels {
                            write_side_by_side_line(&mut out, '-', d.old_idx, a_lines[d.old_idx - 1], 0, ' ', "", left_w, right_w)?;
                        }
                        i += 1;
                    }
                }
                'a' => {
                    // Pure addition (no preceding delete)
                    write_side_by_side_line(&mut out, ' ', 0, "", ch.new_idx, '+', b_lines[ch.new_idx - 1], left_w, right_w)?;
                    i += 1;
                }
                _ => { i += 1; }
            }
        }
        writeln!(out, "{}", "─".repeat(term_width))?;
    }
    out.flush()?;
    Ok(())
}

fn write_side_by_side_line(
    out: &mut BufWriter<io::StdoutLock>,
    l_mark: char,
    l_num: usize,
    l_content: &str,
    r_num: usize,
    r_mark: char,
    r_content: &str,
    left_w: usize,
    right_w: usize,
) -> io::Result<()> {
    let l_text = truncate(l_content, left_w);
    let r_text = truncate(r_content, right_w);
    let l_num_str = if l_num > 0 { format!("{:>4}", l_num) } else { "    ".to_string() };
    let r_num_str = if r_num > 0 { format!("{:>4}", r_num) } else { "    ".to_string() };
    writeln!(out, "{} {} {:<left_w$} │ {} {} {:<right_w$}",
             l_mark, l_num_str, l_text, r_num_str, r_mark, r_text,
             left_w = left_w, right_w = right_w)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn terminal_width() -> Option<usize> {
    // Try COLUMNS env var first
    if let Ok(c) = env::var("COLUMNS") {
        if let Ok(n) = c.parse() { return Some(n); }
    }
    // Try ioctl (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = io::stdout().as_raw_fd();
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
                return Some(ws.ws_col as usize);
            }
        }
    }
    None
}

fn file_diff_json(
    first: &Path,
    second: &Path,
    a_lines: &[&str],
    b_lines: &[&str],
    changes: &[Change],
    context: usize,
    adds: usize,
    dels: usize,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let change_idx: Vec<usize> = changes.iter().enumerate()
        .filter(|(_, c)| c.typ != 'e')
        .map(|(i, _)| i)
        .collect();

    let mut hunks: Vec<serde_json::Value> = Vec::new();
    if !change_idx.is_empty() {
        let mut hunk_starts = Vec::new();
        let mut hunk_ends = Vec::new();
        let mut hs = change_idx[0];
        let mut he = change_idx[0];
        for &ci in &change_idx[1..] {
            if ci - he > 2 * context {
                hunk_starts.push(hs);
                hunk_ends.push(he);
                hs = ci;
            }
            he = ci;
        }
        hunk_starts.push(hs);
        hunk_ends.push(he);

        let n = changes.len();
        for (&hs, &he) in hunk_starts.iter().zip(hunk_ends.iter()) {
            let ctx_lo = hs.saturating_sub(context);
            let ctx_hi = (he + context + 1).min(n);
            let a_start = changes[ctx_lo].old_idx.max(1);
            let b_start = changes[ctx_lo].new_idx.max(1);
            let mut a_cnt = 0;
            let mut b_cnt = 0;
            let mut lines: Vec<serde_json::Value> = Vec::new();
            for ci in ctx_lo..ctx_hi {
                let ch = &changes[ci];
                if ch.typ != 'a' { a_cnt += 1; }
                if ch.typ != 'd' { b_cnt += 1; }
                let (op, line_num, content) = match ch.typ {
                    'a' => ("add", ch.new_idx, b_lines[ch.new_idx - 1]),
                    'd' => ("del", ch.old_idx, a_lines[ch.old_idx - 1]),
                    _   => ("eq",  ch.old_idx, a_lines[ch.old_idx - 1]),
                };
                lines.push(serde_json::json!({"op": op, "line": line_num, "text": content}));
            }
            hunks.push(serde_json::json!({
                "old_start": a_start, "old_count": a_cnt,
                "new_start": b_start, "new_count": b_cnt,
                "lines": lines,
            }));
        }
    }
    let v = serde_json::json!({
        "file_a": first.display().to_string(),
        "file_b": second.display().to_string(),
        "additions": adds,
        "deletions": dels,
        "hunks": hunks,
    });
    writeln!(out, "{}", serde_json::to_string_pretty(&v)?)?;
    out.flush()?;
    Ok(())
}

struct Change { typ: char, old_idx: usize, new_idx: usize }

fn compute_diff(a: &[&str], b: &[&str]) -> Vec<Change> {
    let n = a.len(); let m = b.len();
    if n == 0 && m == 0 { return vec![]; }
    if n == 0 { return (0..m).map(|i| Change { typ: 'a', old_idx: 1, new_idx: i + 1 }).collect(); }
    if m == 0 { return (0..n).map(|i| Change { typ: 'd', old_idx: i + 1, new_idx: 1 }).collect(); }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = if a[i-1] == b[j-1] { dp[i-1][j-1] + 1 }
                       else { dp[i-1][j].max(dp[i][j-1]) };
        }
    }

    let mut i = n; let mut j = m;
    let mut rev = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i-1] == b[j-1] {
            rev.push(Change { typ: 'e', old_idx: i, new_idx: j });
            i -= 1; j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j-1] >= dp[i-1][j]) {
            rev.push(Change { typ: 'a', old_idx: i + 1, new_idx: j });
            j -= 1;
        } else {
            rev.push(Change { typ: 'd', old_idx: i, new_idx: j + 1 });
            i -= 1;
        }
    }
    rev.reverse();
    rev
}


fn dir_diff(first: &Path, second: &Path, context: usize, only_stat: bool, ai_mode: bool, side_by_side: bool, json_output: bool) -> anyhow::Result<()> {
    let mut all: Vec<(String, String, Vec<String>)> = Vec::new();
    fn walk(dir: &Path, base: &str, all: &mut Vec<(String, String, Vec<String>)>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "target" {
                        let sub = if base.is_empty() { name.to_string() } else { format!("{}/{}", base, name) };
                        walk(&p, &sub, all);
                    }
                } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        all.push((if base.is_empty() { p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string() } else { format!("{}/{}", base, p.file_name().and_then(|n| n.to_str()).unwrap_or("")) }, content, vec![]));
                    }
                }
            }
        }
    }
    walk(first, "", &mut all);
    walk(second, "", &mut all);
    all.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, content, items) in &all {
        println!("── {} ──", name);
        for item in items { println!("  {}", item); }
        if let Some(ref target) = items.first() {
            if target.to_lowercase().contains(&target.to_lowercase()) {
                println!("\n--- Extracted: {} ---\n{}", target, highlight_item(content, target));
            }
        }
    }
    Ok(())
}

fn walk_for_diff(dir: &Path) -> Vec<String> {
    vec![]
}

fn file_diff_ai(first: &Path, second: &Path, a_lines: &[&str], b_lines: &[&str], changes: &[Change], adds: usize, dels: usize) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let v = serde_json::json!({
        "file_a": first.display().to_string(),
        "file_b": second.display().to_string(),
        "additions": adds,
        "deletions": dels,
        "changes": changes.iter().map(|c| {
            let content = match c.typ {
                'a' => b_lines.get(c.new_idx.saturating_sub(1)).unwrap_or(&""),
                _   => a_lines.get(c.old_idx.saturating_sub(1)).unwrap_or(&""),
            };
            serde_json::json!({"type": match c.typ {'a' => "add", 'd' => "del", _ => "eq"}, "line": if c.typ == 'a' { c.new_idx } else { c.old_idx }, "text": content})
        }).collect::<Vec<_>>()
    });
    writeln!(out, "{}", serde_json::to_string_pretty(&v)?)?;
    out.flush()?;
    Ok(())
}

fn make_hunk(a_lines: &[&str], b_lines: &[&str], changes: &[Change], hs: usize, he: usize, context: usize) -> serde_json::Value {
    serde_json::json!(null)
}

fn highlight_item(content: &str, name: &str) -> String {
    content.to_string()
}
