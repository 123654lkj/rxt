use std::path::Path;
use std::fs;

/// Directory tree — visualization or JSON output
pub fn run(path: &Path, max_depth: Option<usize>, ignore_patterns: &[String], only_dirs: bool, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        let mut entries: Vec<serde_json::Value> = Vec::new();
        collect_json(path, &mut entries, max_depth, ignore_patterns, only_dirs, 0)?;
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(entries))?);
        return Ok(());
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or(".");
    println!("{}", name);
    print_tree(path, "", 0, max_depth, ignore_patterns, only_dirs)
}

fn collect_json(dir: &Path, out: &mut Vec<serde_json::Value>, max_depth: Option<usize>, ignore: &[String], only_dirs: bool, depth: usize) -> anyhow::Result<()> {
    if let Some(md) = max_depth { if depth >= md { return Ok(()); } }
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
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
    for entry in entries {
        let path = entry.path();
        let is_dir = entry.file_type()?.is_dir();
        let name: String = entry.file_name().to_string_lossy().to_string();
        if only_dirs && !is_dir { continue; }
        let mut obj = serde_json::Map::new();
        obj.insert("name".to_string(), serde_json::Value::String(name.clone()));
        obj.insert("path".to_string(), serde_json::Value::String(path.display().to_string()));
        obj.insert("type".to_string(), serde_json::Value::String(if is_dir { "dir".to_string() } else { "file".to_string() }));
        obj.insert("depth".to_string(), serde_json::json!(depth));
        if let Ok(meta) = path.metadata() {
            obj.insert("size_bytes".to_string(), serde_json::json!(meta.len()));
        }
        if is_dir {
            out.push(serde_json::Value::Object(obj));
            collect_json(&path, out, max_depth, ignore, only_dirs, depth + 1)?;
        } else {
            out.push(serde_json::Value::Object(obj));
        }
    }
    Ok(())
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
