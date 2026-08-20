// rxt cut — 列提取
use std::io::{self, BufRead, Write};

pub fn run(
    input: Option<&str>,
    delimiter: Option<String>,
    fields: &str,
    only_delimited: bool,
) -> anyhow::Result<()> {
    let sep = delimiter.as_deref().unwrap_or("\t");
    let field_indices: Vec<usize> = fields
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            if s.contains('-') {
                let parts: Vec<&str> = s.splitn(2, '-').collect();
                let start = parts[0].trim().parse::<usize>().unwrap_or(1);
                let end = if parts.len() > 1 && !parts[1].trim().is_empty() {
                    parts[1].trim().parse::<usize>().unwrap_or(start)
                } else {
                    start
                };
                Some(start..=end)
            } else {
                let n = s.parse::<usize>().ok()?;
                Some(n..=n)
            }
        })
        .flat_map(|r| r.collect::<Vec<_>>())
        .collect();

    let lines: Vec<String> = if let Some(path) = input {
        let content = std::fs::read_to_string(path)?;
        content.lines().map(|s| s.to_string()).collect()
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<Vec<_>, _>>()?
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for line in &lines {
        let parts: Vec<&str> = line.splitn(256, sep).collect();
        if only_delimited && parts.len() < 2 {
            continue;
        }
        let selected: Vec<&str> = field_indices
            .iter()
            .filter_map(|i| {
                let idx = i.saturating_sub(1);
                parts.get(idx).copied()
            })
            .collect();
        writeln!(handle, "{}", selected.join(sep))?;
    }

    Ok(())
}
