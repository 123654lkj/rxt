//! 目录列表 — 类似 ls,适合 LLM 快速读取目录概览
//!
//! 输出列: type size mtime name
//! - --json: 结构化 [{type, size, mtime, name}]
//! - --all 含隐藏
//! - --sort name|size|mtime
//! - --depth N (默认 1)

use std::path::Path;
use std::fs;
use std::time::SystemTime;

pub fn run(dir: &Path, json_output: bool, all: bool, sort_by: Option<&str>, depth: Option<usize>, max_results: Option<usize>, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    // 远程模式: 在远端执行等效命令
    if let Some(rc) = remote {
        return run_remote(dir, depth, rc);
    }

    let d = if dir.exists() { dir } else { Path::new(".") };
    let mut entries: Vec<Entry> = Vec::new();
    let max_d = depth.unwrap_or(1);
    walk(d, 0, max_d, all, &mut entries);
    if let Some(m) = max_results { entries.truncate(m); }

    match sort_by.unwrap_or("name") {
        "size" => entries.sort_by_key(|e| e.size),
        "mtime" => entries.sort_by_key(|e| std::cmp::Reverse(e.mtime)),
        _ => entries.sort_by(|a, b| a.name.cmp(&b.name)),
    }

    if json_output {
        let v: Vec<_> = entries.iter().map(|e| serde_json::json!({
            "type": e.kind,
            "size": e.size,
            "mtime": format_mtime(e.mtime),
            "name": e.rel_path,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "dir": d.display().to_string(),
            "count": entries.len(),
            "entries": v,
        }))?);
    } else {
        println!("Directory: {} ({} entries)", d.display(), entries.len());
        println!("{:<6} {:>10}  {:<19}  {}", "TYPE", "SIZE", "MTIME", "NAME");
        for e in &entries {
            let mtime_str = format_mtime(e.mtime);
            println!("{:<6} {:>10}  {:<19}  {}", e.kind, e.size, mtime_str, e.rel_path);
        }
    }
    Ok(())
}

struct Entry { kind: String, size: u64, mtime: SystemTime, name: String, rel_path: String }

fn walk(dir: &Path, depth: usize, max_depth: usize, all: bool, entries: &mut Vec<Entry>) {
    if depth > max_depth { return; }
    let read = match fs::read_dir(dir) { Ok(r) => r, Err(_) => return };
    for entry in read.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !all && name.starts_with('.') { continue; }
        let meta = match entry.metadata() { Ok(m) => m, Err(_) => continue };
        let size = if meta.is_dir() { compute_dir_size(&p) } else { meta.len() };
        let kind = if meta.is_dir() { "dir" } else if meta.is_symlink() { "link" } else { "file" }.to_string();
        let rel = p.display().to_string();
        entries.push(Entry { kind, size, mtime: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), name: name.clone(), rel_path: rel });
        if meta.is_dir() && depth + 1 <= max_depth {
            walk(&p, depth + 1, max_depth, all, entries);
        }
    }
}

fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += compute_dir_size(&entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

fn format_mtime(t: SystemTime) -> String {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    // YYYY-MM-DD HH:MM:SS
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let min = (time_of_day / 60) % 60;
    let sec = time_of_day % 60;
    // Civil date from days since 1970-01-01
    let (y, m, d) = civil_from_days(days_since_epoch);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hour, min, sec)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    // Howard Hinnant's date algorithm
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365*yoe + yoe/4 - yoe/100);
    let mp = (5*doy + 2) / 153;
    let d = (doy - (153*mp + 2)/5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// 远程 ls: Windows 用 Get-ChildItem, Linux 用 find+stat
fn run_remote(dir: &Path, depth: Option<usize>, rc: &crate::remote::RemoteChannel) -> anyhow::Result<()> {
    let path_str = dir.to_string_lossy();
    let max_d = depth.unwrap_or(1);

    if rc.is_windows() {
        // OpenSSH 默认 shell 是 cmd，必须显式走系统 PowerShell 5.1
        let ps = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
        let win_path = path_str.replace('/', "\\");
        let cmd = format!(
            r#"{} -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -Path '{}' -Recurse -Depth {} -Force | Select-Object Mode, Length, LastWriteTime, Name | Format-Table -AutoSize""#,
            ps, win_path, max_d
        );
        let out = rc.exec(&cmd)?;
        println!("{}", out.trim_end());
    } else {
        // Linux: 用 ls -la 输出
        let cmd = format!("ls -la --time-style=long-iso '{}'", path_str);
        let out = rc.exec(&cmd)?;
        if out.trim().is_empty() {
            println!("Directory: {} (0 entries)", path_str);
            return Ok(());
        }
        println!("{}", out.trim_end());
    }
    Ok(())
}