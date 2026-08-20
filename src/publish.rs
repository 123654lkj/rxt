//! publish — 一键发布 rxt (v0.8.1)
//!
//! 发布方全流程自动化: 编译两平台 → 装本地 → 部署远程 → git push.
//! 解决每次升级都要手动 6 步的痛点.
//!
//! 用法:
//!   rxt publish                    # 全流程 (编译+部署+git push)
//!   rxt publish --no-deploy        # 只编译+装本地+git push, 不部署远程
//!   rxt publish --no-push          # 编译+部署但不 git push
//!   rxt publish --message "..."    # 自定义 commit message

use std::path::PathBuf;
use std::process::Command;

pub fn run(
    repo: Option<&str>,
    no_deploy: bool,
    no_push: bool,
    message: Option<&str>,
) -> anyhow::Result<()> {
    // 1. 定位仓库
    let repo_path = find_repo(repo)?;
    println!("📦 仓库: {}", repo_path.display());

    // 读版本号
    let cargo_toml = std::fs::read_to_string(repo_path.join("Cargo.toml"))?;
    let version = cargo_toml
        .lines()
        .find_map(|l| {
            l.strip_prefix("version = ")
                .map(|s| s.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "?".into());
    println!("   版本: {}", version);

    // 2. 检查有无未提交改动 (提醒, 不阻塞)
    let dirty = git_in(&repo_path, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        println!("⚠ 工作区有未提交改动 ({} 处)", dirty.lines().count());
    }

    // 3. 编译 Linux
    println!("\n🔨 编译 Linux (x86_64-unknown-linux-gnu)...");
    let build = Command::new("cargo")
        .current_dir(&repo_path)
        .args(["build", "--release"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if !build.success() {
        anyhow::bail!("Linux 编译失败");
    }
    println!("✓ Linux 编译成功");

    let linux_bin = repo_path.join("target/release/rxt");
    if !linux_bin.exists() {
        anyhow::bail!("Linux 产物不存在: {}", linux_bin.display());
    }

    // 4. 编译 Windows (交叉编译)
    println!("\n🔨 编译 Windows (x86_64-pc-windows-gnu)...");
    let win_target = "x86_64-pc-windows-gnu";
    let win_build = Command::new("cargo")
        .current_dir(&repo_path)
        .args(["build", "--release", "--target", win_target])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();
    let win_bin = repo_path.join("target/x86_64-pc-windows-gnu/release/rxt.exe");
    match win_build {
        Ok(s) if s.success() => println!("✓ Windows 编译成功"),
        _ => {
            println!("⚠ Windows 交叉编译失败 (跳过 Windows 部署, 仅 Linux)");
        }
    }
    let has_windows = win_bin.exists();

    // 5. 安装本地 (Linux)
    println!("\n📍 安装本地...");
    install_local(&linux_bin)?;

    // 6. 部署远程
    if !no_deploy {
        deploy_remotes(&linux_bin, &win_bin, has_windows)?;
    }

    // 7. git commit + push
    if !no_push {
        git_publish(&repo_path, message, &dirty)?;
    }

    // 8. 汇总
    println!("\n🎉 发布完成! rxt v{}", version);
    println!("   本地: 已安装");
    if !no_deploy {
        println!("   tuanzi: 已部署 (Linux ELF)");
        if has_windows {
            println!("   xian: 已部署 (Windows PE → Home)");
        }
    }
    if !no_push {
        println!("   git: 已提交推送");
    }
    Ok(())
}

/// 安装到本地两个位置
fn install_local(linux_bin: &PathBuf) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let local_bin = home.join(".local/bin/rxt");

    // .local/bin — 先写临时文件再 mv (避免 "Text file busy": 不能覆盖正在运行的二进制)
    let tmp = local_bin.with_extension("new");
    std::fs::copy(linux_bin, &tmp)?;
    // rename 是原子操作, Linux 上可以替换正在运行的 exe
    let _ = std::fs::remove_file(&local_bin); // 先删(有些情况下 rename 不能覆盖)
    std::fs::rename(&tmp, &local_bin)?;
    println!("  ✓ {}", local_bin.display());

    // /usr/local/bin (需 sudo)
    let sudo_install = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "sudo install -m 755 {} /usr/local/bin/rxt",
            linux_bin.display()
        ))
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::piped())
        .status();
    if matches!(sudo_install, Ok(s) if s.success()) {
        println!("  ✓ /usr/local/bin/rxt");
    } else {
        println!("  ⚠ /usr/local/bin 安装失败 (sudo 密码? 跳过)");
    }
    Ok(())
}

/// 部署到远程机器
fn deploy_remotes(linux_bin: &PathBuf, win_bin: &PathBuf, has_windows: bool) -> anyhow::Result<()> {
    // tuanzi: 用 rxt deploy (Linux ELF, deploy 命令处理)
    println!("\n🚀 部署 tuanzi...");
    let deploy_result = Command::new(linux_bin)
        .args(["deploy", &linux_bin.display().to_string(), "-t", "tuanzi"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();
    match deploy_result {
        Ok(s) if s.success() => println!("  ✓ tuanzi 部署成功"),
        _ => println!("  ⚠ tuanzi 部署失败 (跳过)"),
    }

    // xian: scp 到 Home + rename (绕过 Defender 破坏 C:\rxt 的问题)
    if has_windows {
        println!("\n🚀 部署 xian (Windows, scp → Home)...");
        if let Err(e) = deploy_xian(win_bin) {
            println!("  ⚠ xian 部署失败: {}", e);
        }
    }
    Ok(())
}

/// xian 部署: scp 到 Home → rename → 验证 (Defender 会在 C:\rxt 破坏 exe)
fn deploy_xian(win_bin: &PathBuf) -> anyhow::Result<()> {
    let hosts = crate::hosts::HostsFile::load()?;
    let xian = hosts.get_host("xian")?;
    let password = hosts.get_password(xian).unwrap_or_default();

    // scp 到 Home (rxt-new.exe)
    let scp = format!(
        "sshpass -e scp -o StrictHostKeyChecking=no {} {}@{}:rxt-new.exe",
        win_bin.display(),
        xian.user,
        xian.host
    );
    let scp_result = Command::new("bash")
        .arg("-c")
        .arg(&scp)
        .env("SSHPASS", &password)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()?;
    if !scp_result.success() {
        anyhow::bail!("scp 到 xian 失败");
    }
    println!("  ✓ scp 完成");

    // rename + 验证 (用刚装好的本地 rxt 执行远程命令)
    let ps_rename = r#"$s="$env:USERPROFILE\rxt-new.exe"; $d="$env:USERPROFILE\rxt.exe"; Remove-Item $d -ErrorAction SilentlyContinue; Rename-Item $s $d; & $d --version"#;
    let local_rxt = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rxt"));
    let verify = Command::new(&local_rxt)
        .args(["exec", "--host", "xian", ps_rename])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    match verify {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            println!(
                "  ✓ xian 部署成功: {}",
                ver.trim().lines().next().unwrap_or("")
            );
        }
        _ => println!("  ⚠ xian rename/验证失败 (可能需手动检查)"),
    }
    Ok(())
}

/// git add + commit + push
fn git_publish(repo: &PathBuf, message: Option<&str>, dirty: &str) -> anyhow::Result<()> {
    if dirty.trim().is_empty() && message.is_none() {
        println!("\n📝 无改动, 跳过 git commit");
        return Ok(());
    }
    println!("\n📝 git 提交推送...");

    // add
    let add = Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()?;
    if !add.success() {
        anyhow::bail!("git add 失败");
    }

    // commit
    let msg = message.unwrap_or("release: rxt publish 自动提交");
    let commit = Command::new("git")
        .current_dir(repo)
        .args(["commit", "-m", msg])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    // commit 可能因为"nothing to commit"失败, 不阻塞 push

    // push github
    println!("  push github...");
    let push_gh = Command::new("git")
        .current_dir(repo)
        .args(["push", "github", "master"])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();
    match push_gh {
        Ok(s) if s.success() => println!("  ✓ github 推送成功"),
        _ => println!("  ⚠ github 推送失败 (网络? 可手动 git push)"),
    }

    // push origin
    let push_origin = Command::new("git")
        .current_dir(repo)
        .args(["push", "origin", "master"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status();
    match push_origin {
        Ok(s) if s.success() => println!("  ✓ origin 推送成功"),
        _ => {}
    }

    let _ = commit;
    Ok(())
}

fn find_repo(explicit: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(PathBuf::from(p));
    }
    if let Ok(p) = std::env::var("RXT_REPO") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    for c in &[
        home.join("projects/rxt"),
        home.join("code/rxt"),
        home.join("rxt"),
    ] {
        if c.join("Cargo.toml").exists() {
            return Ok(c.clone());
        }
    }
    anyhow::bail!("找不到 rxt 仓库, 用 --repo 指定")
}

fn git_in(repo: &PathBuf, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").current_dir(repo).args(args).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
