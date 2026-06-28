use std::path::Path;
use std::fs;
use std::collections::HashSet;

/// 差异对比 — 文件/目录/Git diff
pub fn run(first: &Path, second: Option<&Path>, context: usize, only_stat: bool, ai_mode: bool) -> anyhow::Result<()> {
    match second {
        None => git_diff(context, only_stat),
        Some(s) if first.is_dir() && s.is_dir() => dir_diff(first, s, context, only_stat, ai_mode),
        Some(s) if first.is_file() && s.is_file() => file_diff(first, s, context, only_stat, ai_mode),
        _ => anyhow::bail!("Both paths must be files or both directories"),
    }
}

fn git_diff(context: usize, only_stat: bool) -> anyhow::Result<()> {
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
    } else { println!("{}", stdout); }
    Ok(())
}

fn file_diff(first: &Path, second: &Path, context: usize, only_stat: bool, ai_mode: bool) -> anyhow::Result<()> {
    let a = fs::read_to_string(first)?;
    let b = fs::read_to_string(second)?;
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
    if ai_mode {
        return file_diff_ai(first, second, &a_lines, &b_lines, &changes, adds, dels);
    }
    println!("--- {}", first.display());
    println!("+++ {}", second.display());

    // Find indices of actual changes (additions/deletions)
    let change_idx: Vec<usize> = changes.iter().enumerate()
        .filter(|(_, c)| c.typ != 'e')
        .map(|(i, _)| i)
        .collect();

    if change_idx.is_empty() {
        println!("Files are identical");
        return Ok(());
    }

    // Group into hunks: consecutive changes within 2*context lines
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

    // Render each hunk
    let n = changes.len();
    for (&hs, &he) in hunk_starts.iter().zip(hunk_ends.iter()) {
        let ctx_lo = hs.saturating_sub(context);
        let ctx_hi = (he + context + 1).min(n);

        // @@ header: count lines for old and new file within this hunk
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



fn dir_diff(first: &Path, second: &Path, context: usize, only_stat: bool, ai_mode: bool) -> anyhow::Result<()> {
    let first_set: HashSet<String> = walk_for_diff(first).into_iter().collect();
    let second_set: HashSet<String> = walk_for_diff(second).into_iter().collect();
    let mut only_1: Vec<String> = first_set.difference(&second_set).cloned().collect();
    let mut only_2: Vec<String> = second_set.difference(&first_set).cloned().collect();
    let mut common: Vec<String> = first_set.intersection(&second_set).cloned().collect();
    only_1.sort(); only_2.sort(); common.sort();
    if only_stat {
        println!("Only in {}: {}", first.display(), only_1.len());
        println!("Only in {}: {}", second.display(), only_2.len());
        println!("Common files: {}", common.len());
    } else {
        if !only_1.is_empty() { println!("Only in {}:", first.display()); for f in &only_1 { println!("  {}", f); } println!(); }
        if !only_2.is_empty() { println!("Only in {}:", second.display()); for f in &only_2 { println!("  {}", f); } println!(); }
        for file in &common {
            println!("--- {} | {}", first.join(file).display(), second.join(file).display());
            file_diff(&first.join(file), &second.join(file), context, false, ai_mode)?;
            println!();
        }
    }
    Ok(())
}

fn walk_for_diff(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().to_string();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') && name != "target" && name != "node_modules" {
                    for f in walk_for_diff(&path) { files.push(format!("{}/{}", rel, f)); }
                }
            } else if path.is_file() { files.push(rel); }
        }
    }
    files
}


/// AI 模式: 输出结构化 JSON diff
fn file_diff_ai(first: &Path, second: &Path, a_lines: &[&str], b_lines: &[&str], changes: &[Change], adds: usize, dels: usize) -> anyhow::Result<()> {
    // 提取 hunks
    let context = 3usize;
    let change_idx: Vec<usize> = changes.iter().enumerate()
        .filter(|(_, c)| c.typ != 'e')
        .map(|(i, _)| i)
        .collect();

    if change_idx.is_empty() {
        let json = serde_json::json!({
            "type": "file_diff",
            "file": first.display().to_string(),
            "new_file": second.display().to_string(),
            "identical": true,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    let mut hunks: Vec<serde_json::Value> = Vec::new();
    let mut hs = change_idx[0];
    let mut he = change_idx[0];
    for &ci in &change_idx[1..] {
        if ci - he > 2 * context {
            hunks.push(make_hunk(a_lines, b_lines, changes, hs, he, context));
            hs = ci;
        }
        he = ci;
    }
    hunks.push(make_hunk(a_lines, b_lines, changes, hs, he, context));

    // 检测文件类型
    let ext = first.extension().and_then(|e| e.to_str()).unwrap_or("");
    let file_kind = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "ts" => "javascript",
        "toml" => "toml",
        "json" => "json",
        "md" => "markdown",
        "sh" => "shell",
        _ => "text",
    };

    let json = serde_json::json!({
        "type": "file_diff",
        "file": first.display().to_string(),
        "new_file": second.display().to_string(),
        "file_kind": file_kind,
        "stats": { "additions": adds, "deletions": dels, "hunks": hunks.len() },
        "hunks": hunks,
    });
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn make_hunk(a_lines: &[&str], b_lines: &[&str], changes: &[Change], hs: usize, he: usize, context: usize) -> serde_json::Value {
    let ctx_lo = hs.saturating_sub(context);
    let ctx_hi = (he + context + 1).min(changes.len());
    let a_start = changes[ctx_lo].old_idx.max(1);
    let b_start = changes[ctx_lo].new_idx.max(1);
    let mut a_cnt = 0usize;
    let mut b_cnt = 0usize;
    for ci in ctx_lo..ctx_hi {
        if changes[ci].typ != 'a' { a_cnt += 1; }
        if changes[ci].typ != 'd' { b_cnt += 1; }
    }
    let mut lines: Vec<serde_json::Value> = Vec::new();
    for ci in ctx_lo..ctx_hi {
        match changes[ci].typ {
            'a' => lines.push(serde_json::json!({"op": "add", "new_line": changes[ci].new_idx, "text": b_lines[changes[ci].new_idx - 1]})),
            'd' => lines.push(serde_json::json!({"op": "delete", "old_line": changes[ci].old_idx, "text": a_lines[changes[ci].old_idx - 1]})),
            _   => lines.push(serde_json::json!({"op": "context", "text": a_lines[changes[ci].old_idx - 1]})),
        }
    }
    serde_json::json!({
        "old_start": a_start,
        "old_count": a_cnt,
        "new_start": b_start,
        "new_count": b_cnt,
        "lines": lines,
    })
}
