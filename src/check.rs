// rxt check — Rust 代码质量检查组合
use std::path::PathBuf;
use std::process::Command;

pub fn run(
    project_dir: Option<&str>,
    clippy: bool,
    fmt: bool,
    fix: bool,
) -> anyhow::Result<()> {
    let root = find_root(project_dir)?;
    println!("  project: {}", root.display());
    println!();

    let mut all_ok = true;

    println!("── cargo check ──");
    if !run_cargo(&root, &["check"]) { all_ok = false; }
    println!();

    if clippy {
        println!("── cargo clippy ──");
        if !run_cargo(&root, &["clippy", "--", "-D", "warnings"]) { all_ok = false; }
        println!();
    }

    if fmt || fix {
        if fix {
            println!("── cargo fmt (fix) ──");
            if !run_cargo(&root, &["fmt"]) { all_ok = false; }
        } else {
            println!("── cargo fmt --check ──");
            if !run_cargo(&root, &["fmt", "--check"]) { all_ok = false; }
        }
        println!();
    }

    if all_ok {
        println!("  ✓ all checks passed");
    } else {
        eprintln!("  ✗ some checks failed");
        std::process::exit(1);
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

fn run_cargo(root: &std::path::Path, args: &[&str]) -> bool {
    Command::new("cargo")
        .args(args)
        .current_dir(root)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
