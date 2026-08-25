//! upgrade — rxt 自我更新(自举封神)
//!
//! rxt upgrade 自动:
//!   1. 定位本地仓库(参数指定 / 自动探测 / 兜底 ~/.rxt-src)
//!   2. git pull 拉最新
//!   3. cargo build --release [--no-default-features 若本地无 gcc]
//!   4. 备份当前二进制
//!   5. 热替换到 current_exe()
//!
//! 用法:
//!   rxt upgrade                    # 自动探测仓库并升级
//!   rxt upgrade --repo PATH        # 指定仓库
//!   rxt upgrade --check            # 只看是否有更新,不真升级
//!   rxt upgrade --features "a,b"   # 指定 feature(默认自动判断)
//!   rxt upgrade --no-build         # 只 pull 不编译(用预编译产物)

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(
    repo: Option<&str>,
    check_only: bool,
    features: Option<&str>,
    no_build: bool,
) -> anyhow::Result<()> {
    // 1. 定位仓库
    let repo_path = find_repo(repo)?;
    println!("📦 仓库: {}", repo_path.display());

    if !repo_path.join(".git").exists() && !repo_path.join(".git").is_file() {
        anyhow::bail!("{} 不是 git 仓库", repo_path.display());
    }

    // 2. 拉取前记下当前 HEAD
    let old_head = git_in(&repo_path, &["rev-parse", "HEAD"]).ok();

    // 3. pull
    println!("⬇  git pull...");
    let pull = Command::new("git")
        .current_dir(&repo_path)
        .args(["pull", "--ff-only"])
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o StrictHostKeyChecking=accept-new -o BatchMode=yes",
        )
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if !pull.success() {
        anyhow::bail!("git pull 失败(可能有本地改动未提交,或网络问题)");
    }

    let new_head = git_in(&repo_path, &["rev-parse", "HEAD"]).ok();
    let updated = old_head.as_deref() != new_head.as_deref();

    if check_only {
        if updated {
            println!(
                "\n✨ 有更新: {}..{}",
                &old_head.map(|h| h[..8].to_string()).unwrap_or_default(),
                &new_head.map(|h| h[..8].to_string()).unwrap_or_default()
            );
            let log = git_in(&repo_path, &["log", "--oneline", "-5"]).unwrap_or_default();
            println!("最近提交:\n{}", log);
            println!("\n(仅检查模式,未编译。去掉 --check 真正升级)");
        } else {
            println!("\n✓ 已是最新");
        }
        return Ok(());
    }

    if !updated && !no_build {
        println!("\n✓ 已是最新版本,跳过编译");
        // 但仍可重新部署(如果二进制丢了)
        return Ok(());
    }

    if no_build {
        println!("⚠ --no-build: 跳过编译,直接尝试用现有产物部署");
    } else {
        // 4. 编译
        let feats = determine_features(&repo_path, features);
        let mut args = vec!["build", "--release", "--bin", "rxt", "--bin", "rxt-tools"];
        let no_default = feats
            .as_deref()
            .map(|f| {
                if f.is_empty() {
                    args.push("--no-default-features");
                    true
                } else {
                    args.push("--no-default-features");
                    args.push("--features");
                    args.push(f);
                    true
                }
            })
            .is_some();
        let _ = no_default;
        println!("🔨 cargo {} (在 {})", args.join(" "), repo_path.display());
        let build = Command::new("cargo")
            .current_dir(&repo_path)
            .args(&args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;
        if !build.success() {
            anyhow::bail!("编译失败");
        }
        println!("✓ 编译成功");
    }

    // 5. 定位产物
    let exe_name = if cfg!(windows) { "rxt.exe" } else { "rxt" };
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                repo_path.join(p)
            }
        })
        .unwrap_or_else(|| repo_path.join("target"));
    let built = target_root.join("release").join(exe_name);
    if !built.exists() {
        anyhow::bail!("编译产物不存在: {}", built.display());
    }

    // 6. 热替换当前二进制
    let cur = std::env::current_exe()?;
    println!("🔧 替换 {} -> {}", built.display(), cur.display());

    // 备份
    let bak = cur.with_extension(if cfg!(windows) { "exe.bak" } else { "bak" });
    let _ = std::fs::remove_file(&bak);
    std::fs::copy(&cur, &bak)?;
    println!("  已备份 -> {}", bak.display());

    // Windows 不能覆盖正在运行的 exe,先改名
    #[cfg(windows)]
    {
        let tmp = cur.with_extension("exe.old");
        let _ = std::fs::remove_file(&tmp);
        std::fs::rename(&cur, &tmp)?;
        if let Err(e) = std::fs::copy(&built, &cur) {
            restore_binary(&cur, &bak)?;
            anyhow::bail!("复制新版本失败，已恢复旧版本: {e}");
        }
        if let Err(e) = crate::sign::sign_path(&cur, false) {
            restore_binary(&cur, &bak).map_err(|restore| {
                anyhow::anyhow!("新版本签名失败（{e}），恢复旧版本也失败: {restore}")
            })?;
            anyhow::bail!("新版本签名失败，已恢复旧版本: {e}");
        }
        // 留 .old,下次启动后可清(或留给系统)
    }
    #[cfg(not(windows))]
    {
        if let Err(e) = std::fs::copy(&built, &cur) {
            restore_binary(&cur, &bak)?;
            anyhow::bail!("复制新版本失败，已恢复旧版本: {e}");
        }
    }

    // 7. 验证
    let verify = Command::new(&cur).arg("--version").output();
    let ver = match verify {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        Ok(out) => {
            restore_binary(&cur, &bak)?;
            anyhow::bail!(
                "新版本自检失败，已恢复旧版本: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            restore_binary(&cur, &bak)?;
            anyhow::bail!("新版本无法启动，已恢复旧版本: {e}");
        }
    };
    println!("\n🎉 升级完成! {}", ver.trim());
    if updated {
        println!(
            "   {} -> {}",
            &old_head.map(|h| h[..8].to_string()).unwrap_or_default(),
            &new_head.map(|h| h[..8].to_string()).unwrap_or_default()
        );
    }
    Ok(())
}

fn restore_binary(current: &Path, backup: &Path) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(current);
    std::fs::copy(backup, current)?;
    Ok(())
}

/// 探测本地仓库: 参数 > 环境变量 RXT_REPO > 常见位置
fn find_repo(explicit: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(PathBuf::from(p));
    }
    if let Ok(p) = std::env::var("RXT_REPO") {
        return Ok(PathBuf::from(p));
    }
    // 常见位置扫描
    let candidates = [r"G:\codex-AI-tools\ws\rxt", r"C:\codex-AI-tools\ws\rxt"];
    for c in &candidates {
        if Path::new(c).join("Cargo.toml").exists() {
            return Ok(PathBuf::from(c));
        }
    }
    // Linux/Mac
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let linux_cands = [
        home.join("projects/rxt"),
        home.join("code/rxt"),
        home.join("rxt"),
    ];
    for c in &linux_cands {
        if c.join("Cargo.toml").exists() {
            return Ok(c.clone());
        }
    }
    anyhow::bail!("找不到 rxt 仓库,请用 --repo 指定(或设 RXT_REPO 环境变量)")
}

/// 决定 feature: 用户指定 > 自动判断(gcc 不存在则关 default)
fn determine_features(repo: &Path, explicit: Option<&str>) -> Option<String> {
    if let Some(f) = explicit {
        return Some(f.to_string());
    }
    // 自动: 检查 gcc 是否可用
    let has_gcc = Command::new(if cfg!(windows) { "gcc" } else { "cc" })
        .arg("--version")
        .output()
        .is_ok()
        || Command::new("gcc").arg("--version").output().is_ok();
    if has_gcc {
        None // 用默认 feature
    } else {
        Some(String::new()) // 空 = --no-default-features
    }
}

fn git_in(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").current_dir(repo).args(args).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
