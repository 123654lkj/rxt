// rxt uniq — 去重
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

pub fn run(
    input: Option<&str>,
    count: bool,
    duplicates: bool,
    ignore_case: bool,
) -> anyhow::Result<()> {
    let lines: Vec<String> = if let Some(path) = input {
        let content = std::fs::read_to_string(path)?;
        content.lines().map(|s| s.to_string()).collect()
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<Vec<_>, _>>()?
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    if count {
        let mut freq: HashMap<String, usize> = HashMap::new();
        for line in &lines {
            let key = if ignore_case {
                line.to_lowercase()
            } else {
                line.clone()
            };
            *freq.entry(key).or_insert(0) += 1;
        }
        let mut seen = std::collections::HashSet::new();
        for line in &lines {
            let key = if ignore_case {
                line.to_lowercase()
            } else {
                line.clone()
            };
            if seen.insert(key.clone()) {
                let c = freq.get(&key).unwrap_or(&0);
                writeln!(handle, "{:>7} {}", c, line)?;
            }
        }
        return Ok(());
    }

    if duplicates {
        let mut freq: HashMap<String, usize> = HashMap::new();
        for line in &lines {
            let key = if ignore_case {
                line.to_lowercase()
            } else {
                line.clone()
            };
            *freq.entry(key).or_insert(0) += 1;
        }
        let mut seen = std::collections::HashSet::new();
        for line in &lines {
            let key = if ignore_case {
                line.to_lowercase()
            } else {
                line.clone()
            };
            if *freq.get(&key).unwrap_or(&0) > 1 && seen.insert(key.clone()) {
                writeln!(handle, "{}", line)?;
            }
        }
        return Ok(());
    }

    let mut seen = std::collections::HashSet::new();
    for line in &lines {
        let key = if ignore_case {
            line.to_lowercase()
        } else {
            line.clone()
        };
        if seen.insert(key) {
            writeln!(handle, "{}", line)?;
        }
    }

    Ok(())
}
