//! tail -f 替代 — 监控文件追加新行
//!
//! - `rxt tail FILE`: 打印新追加的行(类似 tail -f)
//! - --filter PATTERN: 只输出匹配的行(正则)
//! - --interval MS: 轮询间隔(默认 500ms)
//! - --lines N: 先打印最后 N 行(类似 tail -n)
//! - --once: 打印最后 N 行后退出

use std::path::Path;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

pub fn run(path: &Path, filter: Option<&str>, interval_ms: u64, lines: usize, once: bool) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    let re = filter.and_then(|p| regex::Regex::new(p).ok());

    // Print last N lines first
    if lines > 0 {
        if let Ok(content) = fs::read_to_string(path) {
            let all_lines: Vec<&str> = content.lines().collect();
            let start = all_lines.len().saturating_sub(lines);
            for line in &all_lines[start..] {
                if re.as_ref().map_or(true, |r| r.is_match(line)) {
                    println!("{}", line);
                }
            }
        }
    }

    if once { return Ok(()); }

    let mut last_size: u64 = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let interval = Duration::from_millis(interval_ms);

    loop {
        std::thread::sleep(interval);
        let cur_size = match fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if cur_size > last_size {
            let mut f = match fs::File::open(path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if f.seek(SeekFrom::Start(last_size)).is_err() { continue; }
            let mut buf = String::new();
            if f.read_to_string(&mut buf).is_err() { continue; }
            for line in buf.lines() {
                if re.as_ref().map_or(true, |r| r.is_match(line)) {
                    println!("{}", line);
                }
            }
            last_size = cur_size;
        } else if cur_size < last_size {
            eprintln!("[file truncated or rotated]");
            last_size = 0;
        }
    }
}