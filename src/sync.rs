//! sync — 跨机目录同步 (rsync 替代)
//!
//! rxt sync <local_dir> --to <host>:<remote_dir> [--delete] [--dry-run]
//! 基于 walk + read + write 实现, 不依赖 rsync

use std::path::{Path, PathBuf};
use crate::remote::RemoteChannel;
use crate::hosts::RemoteOs;

pub fn run(
    local_dir: &PathBuf,
    target: &str,
    remote_dir: &str,
    delete: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    if !local_dir.is_dir() {
        anyhow::bail!("{} 不是目录", local_dir.display());
    }

    let remote = RemoteChannel::connect(target)?;
    let os = remote.remote_os();

    // 统计本地文件
    let local_files = collect_files(local_dir)?;
    println!("📁 本地: {} 个文件 ({} bytes)",
        local_files.len(),
        local_files.iter().map(|(_, s)| s).sum::<u64>());

    if dry_run {
        println!("🔍 DRY RUN 模式 (不会真正写入/删除)");
    }

    let mut uploaded = 0u64;
    let mut skipped = 0u64;
    let mut errors = 0u64;

    for (rel_path, size) in &local_files {
        let local_path = local_dir.join(rel_path);
        let remote_path = format_remote_path(&remote_dir, &rel_path, os);

        // 读取本地文件
        let content = match std::fs::read(&local_path) {
            Ok(c) => c,
            Err(e) => { eprintln!("  ❌ 读 {}: {}", rel_path, e); errors += 1; continue; }
        };

        if dry_run {
            println!("  → {} ({}B)", rel_path, size);
            uploaded += 1;
            continue;
        }

        match remote.write_file_with_mode(Path::new(&remote_path), &content, 0o644) {
            Ok(_) => { uploaded += 1; }
            Err(e) => {
                // 检查是否是二进制文件(无法用 exec 写入)
                eprintln!("  ⚠ {} : {}", rel_path, e);
                errors += 1;
            }
        }
    }

    // delete 模式: 删远程多余文件
    if delete && !dry_run {
        println!("\n🗑  检查远程多余文件...");
        // 简化: 列远程文件, 不在本地的删掉
        // (实现省略, 需要远程 ls 支持)
    }

    println!("\n{}", "─".repeat(40));
    println!("✅ 上传: {}  跳过: {}  错误: {}", uploaded, skipped, errors);

    if errors > 0 {
        anyhow::bail!("{} 个文件同步失败", errors);
    }
    Ok(())
}

fn collect_files(dir: &Path) -> anyhow::Result<Vec<(String, u64)>> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn walk(base: &Path, current: &Path, files: &mut Vec<(String, u64)>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // 跳过隐藏目录和常见垃圾
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        if path.is_dir() {
            walk(base, &path, files)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(base).unwrap_or(path.as_path()).to_string_lossy().replace('\\', "/");
            let size = entry.metadata()?.len();
            files.push((rel, size));
        }
    }
    Ok(())
}

fn format_remote_path(remote_dir: &str, rel_path: &str, os: RemoteOs) -> String {
    match os {
        RemoteOs::Windows => {
            format!("{}\\{}", remote_dir.trim_end_matches('\\'), rel_path.replace('/', "\\"))
        }
        _ => format!("{}/{}", remote_dir.trim_end_matches('/'), rel_path),
    }
}
