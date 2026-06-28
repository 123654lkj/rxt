//! dup — 按内容哈希找重复文件
//!
//! 解决: 清理重复照片/下载/文档。按 SHA256 精确比对,不靠文件名。
//! 三阶段优化: 大小过滤 → 首尾采样哈希 → 全文哈希(只对真正可疑的)
//!
//! 用法:
//!   rxt dup /path/to/dir              # 找重复
//!   rxt dup . --min-size 1M           # 只看大于 1MB 的
//!   rxt dup . --json                  # 结构化输出
//!   rxt dup . --delete                # 交互式删除(保留每组第一个)
//!   rxt dup . --ext jpg,png           # 只看指定扩展名

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fs;
use sha2::{Sha256, Digest};

pub fn run(dir: &str, min_size: &str, exts: Option<&str>, delete: bool, json: bool) -> anyhow::Result<()> {
    let root = Path::new(dir);
    if !root.is_dir() {
        anyhow::bail!("{} 不是目录", dir);
    }
    let min_bytes = parse_size(min_size)?;

    // 扩展名过滤
    let ext_filter: Option<Vec<String>> = exts.map(|s| {
        s.split(',').map(|e| e.trim().trim_start_matches('.').to_lowercase()).collect()
    });

    println!("🔍 扫描 {} ...", root.display());
    let start = std::time::Instant::now();

    // 阶段1: 收集所有文件 + 按大小分组
    let mut files: Vec<PathBuf> = Vec::new();
    let mut total_scanned = 0u64;
    let mut total_size = 0u64;
    walk(root, &mut |p| {
        total_scanned += 1;
        if let Ok(meta) = fs::metadata(p) {
            if meta.len() < min_bytes { return; }
            if meta.is_file() {
                if let Some(ref exts) = ext_filter {
                    let ext = p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
                    if !exts.contains(&ext) { return; }
                }
                total_size += meta.len();
                files.push(p.to_path_buf());
            }
        }
    });

    // 按大小分组,只对大小相同(>=2个)的算哈希
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for f in files {
        if let Ok(meta) = fs::metadata(&f) {
            by_size.entry(meta.len()).or_default().push(f);
        }
    }
    let size_candidates: Vec<&Vec<PathBuf>> = by_size.values().filter(|v| v.len() >= 2).collect();

    println!("   扫描 {} 个文件 ({}), {} 组大小相同需进一步比对",
        total_scanned, fmt_size(total_size), size_candidates.iter().map(|v| v.len()).sum::<usize>());

    // 阶段2: 首尾采样哈希(快速过滤)
    let mut by_sample: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for group in &size_candidates {
        for f in *group {
            if let Ok(h) = sample_hash(f) {
                by_sample.entry(h).or_default().push(f.clone());
            }
        }
    }
    let sample_candidates: Vec<&Vec<PathBuf>> = by_sample.values().filter(|v| v.len() >= 2).collect();

    // 阶段3: 全文哈希(只对真正可疑的)
    let mut duplicates: Vec<Vec<PathBuf>> = Vec::new();
    for group in &sample_candidates {
        let mut by_full: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in *group {
            if let Ok(h) = full_hash(f) {
                by_full.entry(h).or_default().push(f.clone());
            }
        }
        for v in by_full.into_values() {
            if v.len() >= 2 {
                let mut sorted = v;
                sorted.sort();
                duplicates.push(sorted);
            }
        }
    }

    let elapsed = start.elapsed();
    // 统计可节省空间
    let mut waste = 0u64;
    for group in &duplicates {
        if let Ok(meta) = fs::metadata(&group[0]) {
            waste += meta.len() * (group.len() as u64 - 1);
        }
    }

    if json {
        let arr: Vec<_> = duplicates.iter().map(|g| {
            let size = fs::metadata(&g[0]).map(|m| m.len()).unwrap_or(0);
            serde_json::json!({
                "size": size,
                "files": g.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "duplicate_groups": duplicates.len(),
            "duplicate_files": duplicates.iter().map(|g| g.len()).sum::<usize>(),
            "wasted_bytes": waste,
            "groups": arr,
        }))?);
        return Ok(());
    }

    println!("\n📋 发现 {} 组重复 ({} 个文件), 浪费 {}, 用时 {:.1}s",
        duplicates.len(),
        duplicates.iter().map(|g| g.len()).sum::<usize>(),
        fmt_size(waste),
        elapsed.as_secs_f64());

    if duplicates.is_empty() {
        println!("🎉 没有重复文件!");
        return Ok(());
    }

    for (i, group) in duplicates.iter().enumerate() {
        let size = fs::metadata(&group[0]).map(|m| m.len()).unwrap_or(0);
        println!("\n[组 #{}] {} 个文件, 各 {}:", i + 1, group.len(), fmt_size(size));
        for (j, f) in group.iter().enumerate() {
            let mark = if j == 0 { "✓ 保留" } else { "  删除" };
            println!("  {} {}", mark, f.display());
        }
    }

    if delete {
        println!("\n⚠ --delete 模式: 删除每组除第一个外的所有文件");
        let mut freed = 0u64;
        for group in &duplicates {
            for f in group.iter().skip(1) {
                if let Ok(meta) = fs::metadata(f) {
                    freed += meta.len();
                }
                match fs::remove_file(f) {
                    Ok(_) => println!("  🗑 删除 {}", f.display()),
                    Err(e) => eprintln!("  ✗ 删除失败 {}: {}", f.display(), e),
                }
            }
        }
        println!("\n✓ 释放 {}", fmt_size(freed));
    } else {
        println!("\n加 --delete 删除重复(保留每组第一个)");
    }
    Ok(())
}

/// 首尾 + 中间各取 4KB 采样哈希(快速判断,大文件优化)
fn sample_hash(path: &Path) -> anyhow::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path)?;
    let total = fs::metadata(path)?.len();
    let mut hasher = Sha256::new();

    // 头 4KB
    let mut buf = vec![0u8; 4096];
    let n = f.read(&mut buf)?;
    hasher.update(&buf[..n]);

    if total > 12288 {
        // 中间 4KB
        f.seek(SeekFrom::Start(total / 2))?;
        let n = f.read(&mut buf)?;
        hasher.update(&buf[..n]);
        // 尾 4KB
        f.seek(SeekFrom::Start(total.saturating_sub(4096)))?;
        let n = f.read(&mut buf)?;
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn full_hash(path: &Path) -> anyhow::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn walk(p: &Path, cb: &mut dyn FnMut(&Path)) {
    if let Ok(rd) = fs::read_dir(p) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 跳过隐藏目录和常见大目录
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || matches!(&*name_str, "node_modules"|"target"|"__pycache__"|".git") {
                    continue;
                }
                walk(&path, cb);
            } else {
                cb(&path);
            }
        }
    }
}

fn parse_size(s: &str) -> anyhow::Result<u64> {
    if s.is_empty() { return Ok(0); }
    let s = s.trim();
    let (num, unit) = s.find(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| (&s[..i], &s[i..]))
        .unwrap_or((s, ""));
    let n: f64 = num.parse().map_err(|_| anyhow::anyhow!("无效大小: {}", s))?;
    let mult = match unit.trim().to_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => anyhow::bail!("未知大小单位: {}", unit),
    };
    Ok((n * mult) as u64)
}

fn fmt_size(b: u64) -> String {
    const MB: u64 = 1024*1024; const KB: u64 = 1024; const GB: u64 = MB*1024;
    if b >= GB { format!("{:.2}G", b as f64/GB as f64) }
    else if b >= MB { format!("{:.1}M", b as f64/MB as f64) }
    else if b >= KB { format!("{:.1}K", b as f64/KB as f64) }
    else { format!("{}B", b) }
}
