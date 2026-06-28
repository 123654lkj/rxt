// rxt build — 智能 Rust 构建
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default, serde::Deserialize)]
struct BuildConfig {
    target: Option<String>,
    profile: Option<String>,
    bin: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    workspace: bool,
}

pub fn run(
    project_dir: Option<&str>,
    target: Option<&str>,
    profile: Option<&str>,
    bin_name: Option<&str>,
    features: Vec<String>,
    workspace: bool,
    list_targets: bool,
    no_config: bool,
) -> anyhow::Result<()> {
    let root = find_project_root(project_dir)?;
    println!("  project: {}", root.display());

    let cfg = if !no_config {
        load_config(&root).unwrap_or_default()
    } else {
        BuildConfig::default()
    };

    let target = target
        .or(cfg.target.as_deref())
        .unwrap_or("x86_64-pc-windows-msvc");

    if list_targets {
        return list_installed_targets();
    }

    let profile = profile
        .or(cfg.profile.as_deref())
        .unwrap_or("release");

    let mut args = vec!["build".to_string()];
    args.push("--target".to_string());
    args.push(target.to_string());

    if profile == "release" {
        args.push("--release".to_string());
    }

    let bin = bin_name.or(cfg.bin.as_deref());
    if let Some(b) = bin {
        args.push("--bin".to_string());
        args.push(b.to_string());
    }

    let feat_list: Vec<&str> = if !features.is_empty() {
        features.iter().map(|s| s.as_str()).collect()
    } else {
        cfg.features.iter().map(|s| s.as_str()).collect()
    };

    if !feat_list.is_empty() {
        for f in &feat_list {
            args.push("--features".to_string());
            args.push(f.to_string());
        }
    }

    if workspace || cfg.workspace {
        args.push("--workspace".to_string());
    }

    println!("  cargo {} --target {}", if profile == "release" { "build --release" } else { "build" }, target);
    if let Some(b) = bin { println!("  binary: {}", b); }
    if !feat_list.is_empty() { println!("  features: {}", feat_list.join(", ")); }
    println!();

    let start = Instant::now();
    let status = std::process::Command::new("cargo")
        .args(&args)
        .current_dir(&root)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    let elapsed = start.elapsed();

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    println!();
    println!("  ✓ build finished in {:.1}s", elapsed.as_secs_f64());

    let target_dir = root.join("target").join(target).join(profile);
    if target_dir.exists() {
        show_binaries(&target_dir, bin);
    }

    Ok(())
}

fn find_project_root(dir: Option<&str>) -> anyhow::Result<PathBuf> {
    crate::common::find_project_root(dir)
}

fn load_config(root: &Path) -> Option<BuildConfig> {
    let path = root.join(".rxt.toml");
    if !path.exists() { return None; }
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()?
}

fn list_installed_targets() -> anyhow::Result<()> {
    let output = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    println!("{}", text);
    Ok(())
}

fn show_binaries(dir: &Path, filter: Option<&str>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut found = false;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let is_exe = p.extension().map(|e| e == "exe").unwrap_or(false);
                if !is_exe && !is_executable(&p) { continue; }
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                if let Some(f) = filter {
                    if !name.contains(f) && name != f && !name.starts_with(f) { continue; }
                }
                if let Ok(meta) = p.metadata() {
                    let size = meta.len();
                    let s = if size > 1048576 {
                        format!("{:.1} MB", size as f64 / 1048576.0)
                    } else if size > 1024 {
                        format!("{:.1} KB", size as f64 / 1024.0)
                    } else {
                        format!("{} B", size)
                    };
                    println!("  → {} ({})", p.display(), s);
                    found = true;
                }
            }
        }
        if !found { println!("  (no binaries found in {})", dir.display()); }
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
