// rxt sort — 行排序
use std::io::{self, BufRead, Write};

pub fn run(
    input: Option<&str>,
    reverse: bool,
    numeric: bool,
    column: Option<usize>,
    sep: Option<String>,
    unique: bool,
) -> anyhow::Result<()> {
    let lines: Vec<String> = if let Some(path) = input {
        let content = std::fs::read_to_string(path)?;
        content.lines().map(|s| s.to_string()).collect()
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<Vec<_>, _>>()?
    };

    if lines.is_empty() {
        return Ok(());
    }

    let mut data: Vec<(Vec<&str>, &str)> = if column.is_some() {
        let sep_char = sep.as_deref().unwrap_or("\t");
        lines.iter().map(|l| {
            let parts: Vec<&str> = l.split(sep_char).collect();
            (parts, l.as_str())
        }).collect()
    } else {
        lines.iter().map(|l| (vec![l.as_str()], l.as_str())).collect()
    };

    let col_idx = column.unwrap_or(1).saturating_sub(1);

    if numeric {
        data.sort_by(|a, b| {
            let av = a.0.get(col_idx).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
            let bv = b.0.get(col_idx).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
            av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        data.sort_by(|a, b| {
            let ak = a.0.get(col_idx).unwrap_or(&"");
            let bk = b.0.get(col_idx).unwrap_or(&"");
            ak.cmp(bk)
        });
    }

    if reverse {
        data.reverse();
    }

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    if unique {
        let mut prev: Option<&str> = None;
        for (_, line) in &data {
            if prev.map_or(true, |p| p != *line) {
                writeln!(handle, "{}", line)?;
                prev = Some(line);
            }
        }
    } else {
        for (_, line) in &data {
            writeln!(handle, "{}", line)?;
        }
    }

    Ok(())
}
