use std::path::Path;
use std::fs;

/// 目录树 — 可视化目录结构
pub fn run(path: &Path, max_depth: Option<usize>, ignore_patterns: &[String], only_dirs: bool) -> anyhow::Result<()> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or(".");
    println!("{}", name);
    print_tree(path, "", 0, max_depth, ignore_patterns, only_dirs)
}

fn print_tree(dir: &Path, prefix: &str, depth: usize, max_depth: Option<usize>, ignore: &[String], only_dirs: bool) -> anyhow::Result<()> {
    if let Some(md) = max_depth { if depth >= md { return Ok(()); } }

    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name_os = e.file_name();
            let name = name_os.to_string_lossy();
            let is_ignored = ignore.iter().any(|pat| {
                if pat.starts_with("*.") {
                    name.ends_with(&pat[1..])
                } else {
                    name == pat.as_str()
                }
            });
            !is_ignored && !name.starts_with('.')
        })
        .collect();
    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name().to_string_lossy().to_string())
    });

    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "\u{2514}\u{2500}\u{2500} " } else { "\u{251c}\u{2500}\u{2500} " };
        let next_prefix = if is_last { format!("{}    ", prefix) } else { format!("{}{}   ", prefix, "\u{2502}") };

        let path = entry.path();
        let is_dir = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().to_string();

        if only_dirs && !is_dir { continue; }

        if is_dir {
            println!("{}{}{}/", prefix, connector, name);
            print_tree(&path, &next_prefix, depth + 1, max_depth, ignore, only_dirs)?;
        } else {
            println!("{}{}{}", prefix, connector, name);
        }
    }
    Ok(())
}
