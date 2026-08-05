//! publish — 一键发布 rxt
//!
//! 发布方全流程: 编译两平台 → 装本地 → 部署远程（可选）→ git push（可选）。
//!
//! 用法:
//!   rxt publish                    # 全流程
//!   rxt publish --no-deploy        # 只编译+装本地+git push
//!   rxt publish --no-push          # 编译+部署但不 git push
//!   rxt publish --message "..."    # 自定义 commit message
//!
//! 环境变量（写入 ~/.rxt/env 或 shell）:
//!   RXT_REPO                     源码路径（也可用 --repo）
//!   RXT_PUBLISH_LINUX_HOSTS      逗号分隔 Linux 目标别名（hosts.toml）
//!   RXT_PUBLISH_WINDOWS_HOSTS    逗号分隔 Windows 目标别名
//!   RXT_PUBLISH_GIT_REMOTES      逗号分隔 git remote（默认 origin）
//!
//! 未配置 *_HOSTS 时跳过远程部署（不绑定任何固定机器名）。

use std::path::PathBuf;
use std::process::Command;

pub fn run(
    repo: Option<&str>,
    no_deploy: bool,
    no_push: bool,
    message: Option<&str>,
) -> anyhow::Result<()> {
    let repo_path = find_repo(repo)?;
    println!("📦 仓库: {}", repo_path.display());

    let cargo_toml = std::fs::read_to_string(repo_path.join("Cargo.toml"))?;
    let version = cargo_toml
        .lines()
        .find_map(|l| {
            l.strip_prefix("version = ")
                .map(|s| s.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "?".into());
    println!("   版本: {}", version);

    let dirty = git_in(&repo_path, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        println!("⚠ 工作区有未提交改动 ({} 处)", dirty.lines().count());
    }

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
        _ => println!("⚠ Windows 交叉编译失败 (跳过 Windows 部署, 仅 Linux)"),
    }
    let has_windows = win_bin.exists();

    println!("\n📍 安装本地...");
    install_local(&linux_bin)?;

    let mut deployed: Vec<String> = Vec::new();
    if !no_deploy {
        deployed = deploy_remotes(&linux_bin, &win_bin, has_windows)?;
    }

    if !no_push {
        git_publish(&repo_path, message, &dirty)?;
    }

    println!("\n🎉 发布完成! rxt v{}", version);
    println!("   本地: 已安装");
    if !no_deploy {
        if deployed.is_empty() {
            println!(
                "   远程: 跳过（设 RXT_PUBLISH_LINUX_HOSTS / RXT_PUBLISH_WINDOWS_HOSTS）"
            );
        } else {
            for h in &deployed {
                println!("   远程: {} 已部署", h);
            }
        }
    }
    if !no_push {
        println!("   git: 已尝试推送");
    }
    Ok(())
}

fn install_local(linux_bin: &PathBuf) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let local_bin = home.join(".local/bin/rxt");

    let tmp = local_bin.with_extension("new");
    std::fs::copy(linux_bin, &tmp)?;
    let _ = std::fs::remove_file(&local_bin);
    std::fs::rename(&tmp, &local_bin)?;
    println!("  ✓ {}", local_bin.display());

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
        println!("  ⚠ /usr/local/bin 安装失败 (sudo? 跳过)");
    }
    Ok(())
}

fn csv_env(key: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 按环境变量部署；返回成功的目标别名列表
fn deploy_remotes(
    linux_bin: &PathBuf,
    win_bin: &PathBuf,
    has_windows: bool,
) -> anyhow::Result<Vec<String>> {
    let linux_hosts = csv_env("RXT_PUBLISH_LINUX_HOSTS");
    let win_hosts = csv_env("RXT_PUBLISH_WINDOWS_HOSTS");
    let mut ok = Vec::new();

    if linux_hosts.is_empty() && win_hosts.is_empty() {
        println!(
            "\n⏭ 未配置 RXT_PUBLISH_LINUX_HOSTS / RXT_PUBLISH_WINDOWS_HOSTS，跳过远程部署"
        );
        return Ok(ok);
    }

    for host in &linux_hosts {
        println!("\n🚀 部署 Linux → {}...", host);
        let status = Command::new(linux_bin)
            .args([
                "deploy",
                &linux_bin.display().to_string(),
                "-t",
                host,
            ])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("  ✓ {} 部署成功", host);
                ok.push(format!("{} (linux)", host));
            }
            _ => println!("  ⚠ {} 部署失败 (跳过)", host),
        }
    }

    if has_windows {
        for host in &win_hosts {
            println!("\n🚀 部署 Windows → {}...", host);
            match deploy_windows_host(host, win_bin) {
                Ok(()) => {
                    println!("  ✓ {} 部署成功", host);
                    ok.push(format!("{} (windows)", host));
                }
                Err(e) => println!("  ⚠ {} 部署失败: {}", host, e),
            }
        }
    } else if !win_hosts.is_empty() {
        println!("\n⚠ 无 Windows 产物，跳过: {}", win_hosts.join(", "));
    }

    Ok(ok)
}

/// Windows 目标: scp 到用户主目录 → rename → 验证
/// （部分环境 Defender 会破坏固定系统路径下的 exe，故用 $HOME）
fn deploy_windows_host(host_alias: &str, win_bin: &PathBuf) -> anyhow::Result<()> {
    let hosts = crate::hosts::HostsFile::load()?;
    let host = hosts.get_host(host_alias)?;
    let password = hosts.get_password(host).unwrap_or_default();

    let scp = format!(
        "sshpass -e scp -o StrictHostKeyChecking=no {} {}@{}:rxt-new.exe",
        win_bin.display(),
        host.user,
        host.host
    );
    let scp_result = Command::new("bash")
        .arg("-c")
        .arg(&scp)
        .env("SSHPASS", &password)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()?;
    if !scp_result.success() {
        anyhow::bail!("scp 到 {} 失败", host_alias);
    }
    println!("  ✓ scp 完成");

    let ps_rename = r#"$s="$env:USERPROFILE\rxt-new.exe"; $d="$env:USERPROFILE\rxt.exe"; Remove-Item $d -ErrorAction SilentlyContinue; Rename-Item $s $d; & $d --version"#;
    let local_rxt = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rxt"));
    let verify = Command::new(&local_rxt)
        .args(["exec", "--host", host_alias, ps_rename])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    match verify {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            println!(
                "  ✓ 验证: {}",
                ver.trim().lines().next().unwrap_or("")
            );
        }
        _ => println!("  ⚠ rename/验证失败 (可能需手动检查)"),
    }
    Ok(())
}

fn git_publish(repo: &PathBuf, message: Option<&str>, dirty: &str) -> anyhow::Result<()> {
    if dirty.trim().is_empty() && message.is_none() {
        println!("\n📝 无改动, 跳过 git commit");
        return Ok(());
    }
    println!("\n📝 git 提交推送...");

    let add = Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()?;
    if !add.success() {
        anyhow::bail!("git add 失败");
    }

    let msg = message.unwrap_or("release: rxt publish 自动提交");
    let commit = Command::new("git")
        .current_dir(repo)
        .args(["commit", "-m", msg])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;

    let remotes = csv_env("RXT_PUBLISH_GIT_REMOTES");
    let remotes = if remotes.is_empty() {
        vec!["origin".to_string()]
    } else {
        remotes
    };

    for remote in remotes {
        println!("  push {}...", remote);
        let push = Command::new("git")
            .current_dir(repo)
            .args(["push", &remote, "master"])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();
        match push {
            Ok(s) if s.success() => println!("  ✓ {} 推送成功", remote),
            _ => println!("  ⚠ {} 推送失败 (可手动 git push)", remote),
        }
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
    // 当前目录若是 rxt 仓库
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("Cargo.toml").exists() {
            if let Ok(toml) = std::fs::read_to_string(cwd.join("Cargo.toml")) {
                if toml.contains("name = \"rxt\"") {
                    return Ok(cwd);
                }
            }
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    for c in &[
        home.join("projects/rxt"),
        home.join("code/rxt"),
        home.join("src/rxt"),
        home.join("rxt"),
    ] {
        if c.join("Cargo.toml").exists() {
            return Ok(c.clone());
        }
    }
    anyhow::bail!("找不到 rxt 仓库, 用 --repo 指定或设 RXT_REPO")
}

fn git_in(repo: &PathBuf, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").current_dir(repo).args(args).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
