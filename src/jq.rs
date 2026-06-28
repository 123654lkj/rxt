use std::io::Read;
use serde_json::Value;

/// JSON 查询/格式化 — 类似 jq
pub fn run(query: Option<&str>, file: Option<&std::path::Path>, fmt: bool, compact: bool) -> anyhow::Result<()> {
    // Read input
    let input = if let Some(f) = file {
        std::fs::read_to_string(f)?
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            anyhow::bail!("No input: pipe JSON or use -f <file>");
        }
        buf
    };

    let value: Value = serde_json::from_str(&input)?;

    if let Some(q) = query {
        let result = eval_path(&value, q);
        match result {
            Some(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            None => anyhow::bail!("Path '{}' not found", q),
        }
    } else if fmt {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else if compact {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn eval_path(value: &Value, path: &str) -> Option<Value> {
    if path == "." { return Some(value.clone()); }
    if path == "[]" {
        if let Value::Array(arr) = value {
            return Some(Value::Array(arr.clone()));
        }
        return Some(value.clone());
    }

    let mut current = value.clone();
    // Parse path segments: .foo[0].bar
    let mut remaining = path;
    while !remaining.is_empty() {
        if remaining.starts_with('.') {
            remaining = &remaining[1..];
            // Extract key (up to next . or [ or end)
            let end = remaining.find(|c: char| c == '.' || c == '[').unwrap_or(remaining.len());
            let key = &remaining[..end];
            remaining = &remaining[end..];
            current = current.get(key)?.clone();
        } else if remaining.starts_with('[') {
            // Handle array index like [0] or []
            let end = remaining.find(']')?;
            let idx_str = &remaining[1..end];
            remaining = &remaining[end+1..];
            if idx_str.is_empty() {
                // [] — keep array as-is
                if !current.is_array() { return None; }
            } else {
                let idx: usize = idx_str.parse().ok()?;
                current = current.get(idx)?.clone();
            }
        } else {
            return None;
        }
    }
    Some(current)
}
