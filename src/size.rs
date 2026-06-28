// rxt size — 编译产物大小分析
use std::path::{Path, PathBuf};

pub fn run(
    project_dir: Option<&str>,
    target: Option<&str>,
    profile: Option<&str>,
    all: bool,
    human: bool,
    sort: bool,
) -> anyhow::Result<()> {
    let root = find_root(project_dir)?;
    let target = target.unwrap_or("x86_64-unknown-linux-musl");
    let profile = profile.unwrap_or("release");

    let mut targets: Vec<(String, PathBuf)> = Vec::new();

    if all {
        let target_base = root.join("target");
        if let Ok(entries) = std::fs::read_dir(&target_base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || name == "debug" || name == "release" || name == "doc" || name == "package" || name == "wasm" {
                        continue;
                    }
                    let bin_dir = p.join(profile);
                    if bin_dir.exists() {
                        targets.push((name.clone(), bin_dir));
                    }
                }
            }
        }
        for p in ["debug", "release"] {
            let dir = target_base.join(p);
            if dir.exists() {
                targets.push((p.to_string(), dir));
            }
        }
    } else {
        let dir = root.join("target").join(&target).join(profile);
        if !dir.exists() {
            anyhow::bail!("target directory not found: {}", dir.display());
        }
        targets.push((format!("{}/{}", target, profile), dir));
    }

    if targets.is_empty() {
        println!("  no build artifacts found");
        return Ok(());
    }

    for (label, dir) in &targets {
        let mut files: Vec<(PathBuf, u64)> = Vec::new();
        collect_binaries(dir, &mut files);

        if files.is_empty() { continue; }

        if sort {
            files.sort_by(|a, b| b.1.cmp(&a.1));
        }

        println!("  [{}]", label);
        let mut total = 0u64;
        for (path, size) in &files {
            total += size;
            let s = if human || size > &1024 {
                fmt_size(*size)
            } else {
                format!("{} B", size)
            };
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            println!("    {:>10}  {}", s, name);
        }
        if files.len() > 1 {
            println!("    {:>10}  ─────────────────", fmt_size(total));
            println!("    {:>10}  total", fmt_size(total));
        }
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

fn collect_binaries(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let is_exe = p.extension().map(|e| e == "exe").unwrap_or(false);
                if !is_exe && !is_executable(&p) { continue; }
                if let Ok(meta) = p.metadata() {
                    let len = meta.len();
                    if len < 1024 { continue; }
                    out.push((p, len));
                }
            }
        }
    }
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        false
    }
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
