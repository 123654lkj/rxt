// rxt clean — 智能 workspace 清理
use std::path::{Path, PathBuf};

pub fn run(
    project_dir: Option<&str>,
    target: Option<&str>,
    profile: Option<&str>,
    dry_run: bool,
    all: bool,
) -> anyhow::Result<()> {
    let root = find_root(project_dir)?;
    let target_base = root.join("target");

    if !target_base.exists() {
        println!("  target/ not found");
        return Ok(());
    }

    if let Some(triple) = target {
        let dir = if let Some(prof) = profile {
            target_base.join(triple).join(prof)
        } else {
            target_base.join(triple)
        };
        return clean_dir(&dir, dry_run);
    }

    if let Some(prof) = profile {
        let mut cleaned = 0u64;
        if let Ok(entries) = std::fs::read_dir(&target_base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let prof_dir = p.join(prof);
                    if prof_dir.exists() {
                        cleaned += dir_size(&prof_dir);
                        if dry_run {
                            println!("  would delete: {}", prof_dir.display());
                        } else {
                            let _ = std::fs::remove_dir_all(&prof_dir);
                            println!("  deleted: {}", prof_dir.display());
                        }
                    }
                }
            }
        }
        let top_prof = target_base.join(prof);
        if top_prof.exists() {
            cleaned += dir_size(&top_prof);
            if dry_run {
                println!("  would delete: {}", top_prof.display());
            } else {
                let _ = std::fs::remove_dir_all(&top_prof);
                println!("  deleted: {}", top_prof.display());
            }
        }
        if dry_run {
            println!("  would free: {}", fmt_size(cleaned));
        } else {
            println!("  freed: {}", fmt_size(cleaned));
        }
        return Ok(());
    }

    if all {
        let size = dir_size(&target_base);
        if dry_run {
            println!("  would delete entire target/ ({}):", fmt_size(size));
            println!("    {}", target_base.display());
        } else {
            std::fs::remove_dir_all(&target_base)?;
            println!("  deleted entire target/ (freed {})", fmt_size(size));
        }
        return Ok(());
    }

    // 默认：清理所有 release profile 目录
    let mut cleaned = 0u64;
    if let Ok(entries) = std::fs::read_dir(&target_base) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let release_dir = p.join("release");
                if release_dir.exists() {
                    cleaned += dir_size(&release_dir);
                    if dry_run {
                        println!("  would delete: {}", release_dir.display());
                    } else {
                        let _ = std::fs::remove_dir_all(&release_dir);
                        println!("  deleted: {}", release_dir.display());
                    }
                }
            }
        }
    }
    let top_release = target_base.join("release");
    if top_release.exists() {
        cleaned += dir_size(&top_release);
        if dry_run {
            println!("  would delete: {}", top_release.display());
        } else {
            let _ = std::fs::remove_dir_all(&top_release);
            println!("  deleted: {}", top_release.display());
        }
    }

    if dry_run {
        println!("  would free: {}", fmt_size(cleaned));
    } else {
        println!("  freed: {}", fmt_size(cleaned));
    }

    Ok(())
}

fn find_root(dir: Option<&str>) -> anyhow::Result<PathBuf> {
    let start = dir.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap());
    let mut current = Some(start.as_path());
    while let Some(p) = current {
        if p.join("Cargo.toml").exists() {
            return Ok(p.to_path_buf());
        }
        current = p.parent();
    }
    anyhow::bail!("no Cargo.toml found")
}

fn clean_dir(dir: &Path, dry_run: bool) -> anyhow::Result<()> {
    if !dir.exists() {
        println!("  not found: {}", dir.display());
        return Ok(());
    }
    let size = dir_size(dir);
    if dry_run {
        println!("  would delete: {} ({})", dir.display(), fmt_size(size));
    } else {
        std::fs::remove_dir_all(dir)?;
        println!("  deleted: {} (freed {})", dir.display(), fmt_size(size));
    }
    Ok(())
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Ok(meta) = p.metadata() {
                    total += meta.len();
                }
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

fn fmt_size(bytes: u64) -> String {
    if bytes > 1048576 {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    } else if bytes > 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
