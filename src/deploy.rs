//! deploy — 一键部署二进制到远程机器
//! v0.8.7: Windows 优先原生 OpenSSH（ssh/scp），不再依赖 bash+sshpass；
//!         Linux 仍可用 sshpass，也可走本机 OpenSSH 密钥/ssh config。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use base64::Engine;
use crate::hosts::HostsFile;

pub fn run(
    binary: &PathBuf,
    targets: &[String],
    is_group: bool,
    remote_path: Option<&str>,
) -> anyhow::Result<()> {
    let local_size = std::fs::metadata(binary)
        .map_err(|e| anyhow::anyhow!("本地文件不存在: {} ({})", binary.display(), e))?
        .len();
    println!("📦 部署: {} ({} bytes)", binary.display(), local_size);

    let hosts = HostsFile::load()?;
    let mut target_hosts: Vec<String> = Vec::new();
    for t in targets {
        if is_group {
            let members = hosts.get_group_members(t)?;
            target_hosts.extend(members);
        } else {
            target_hosts.push(t.clone());
        }
    }
    target_hosts.dedup();
    println!("🎯 目标: {} 台机器 {:?}", target_hosts.len(), target_hosts);

    let mut ok = Vec::new();
    let mut fail = Vec::new();

    for host_name in &target_hosts {
        print!("\n  [{}] ", host_name);
        match deploy_to_host(binary, local_size, host_name, remote_path, &hosts) {
            Ok(rp) => {
                println!("✅ -> {}", rp);
                ok.push(host_name.clone());
            }
            Err(e) => {
                println!("❌ {}", e);
                fail.push((host_name.clone(), e.to_string()));
            }
        }
    }

    println!("\n{}", "─".repeat(50));
    println!("✅ 成功: {} 台 ({})", ok.len(), ok.join(", "));
    if !fail.is_empty() {
        println!("❌ 失败: {} 台", fail.len());
        for (h, e) in &fail {
            println!("   {}: {}", h, e);
        }
        anyhow::bail!("{} 台部署失败", fail.len());
    }
    Ok(())
}

/// 是否更像用「主机别名」走 OpenSSH config（Win/本机 ssh huhu 可用）
fn prefer_ssh_alias() -> bool {
    // Windows 上 OpenSSH 常见；有 ssh 且无 bash 时必须走原生
    which("ssh").is_some() && (cfg!(windows) || which("sshpass").is_none())
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(cmd);
        if p.is_file() {
            return Some(p);
        }
        #[cfg(windows)]
        {
            let p_exe = dir.join(format!("{}.exe", cmd));
            if p_exe.is_file() {
                return Some(p_exe);
            }
        }
    }
    None
}

fn run_cmd(mut c: Command) -> anyhow::Result<Output> {
    c.output()
        .map_err(|e| anyhow::anyhow!("执行失败: {} — 请确认已装 OpenSSH Client", e))
}

fn ssh_base_args() -> Vec<String> {
    vec![
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "BatchMode=yes".into(),
    ]
}

/// 原生 OpenSSH：优先主机别名（读 ~/.ssh/config），否则 user@host
fn ssh_target(host_alias: &str, user: &str, host: &str, use_alias: bool) -> String {
    if use_alias {
        host_alias.to_string()
    } else {
        format!("{}@{}", user, host)
    }
}

fn deploy_to_host(
    binary: &PathBuf,
    local_size: u64,
    host_name: &str,
    remote_path: Option<&str>,
    hosts: &HostsFile,
) -> anyhow::Result<String> {
    let config = hosts.get_host(host_name)?.clone();
    let password = hosts.get_password(&config).unwrap_or_default();
    let use_native = prefer_ssh_alias();
    let target = ssh_target(host_name, &config.user, &config.host, use_native);

    // 探测远端 OS
    let uname = if use_native {
        let mut c = Command::new("ssh");
        for a in ssh_base_args() {
            c.arg(a);
        }
        c.arg(&target).arg("uname -s 2>/dev/null || echo WIN");
        let out = run_cmd(c)?;
        String::from_utf8_lossy(&out.stdout).to_string()
    } else {
        let os_probe = format!(
            "sshpass -e ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {}@{} 'uname -s 2>/dev/null || echo WIN'",
            config.user, config.host
        );
        let probe_out = Command::new("bash")
            .arg("-c")
            .arg(&os_probe)
            .env("SSHPASS", &password)
            .output()?;
        String::from_utf8_lossy(&probe_out.stdout).to_string()
    };
    let is_windows = !uname.to_lowercase().contains("linux");

    // 交叉平台检查
    let local_bytes_head =
        std::fs::read(binary).map_err(|e| anyhow::anyhow!("读本地文件: {}", e))?;
    let is_local_pe = local_bytes_head.len() >= 2 && &local_bytes_head[..2] == b"MZ";
    let is_local_elf = local_bytes_head.len() >= 4 && &local_bytes_head[..4] == b"\x7fELF";
    if is_windows && !is_local_pe {
        anyhow::bail!(
            "跳过: 本地是 {} 二进制, 目标 {} 是 Windows, 需要 PE/exe",
            if is_local_elf { "Linux ELF" } else { "未知" },
            host_name
        );
    }
    if !is_windows && !is_local_elf {
        anyhow::bail!(
            "跳过: 本地是 {} 二进制, 目标 {} 是 Linux, 需要 ELF",
            if is_local_pe { "Windows PE" } else { "未知" },
            host_name
        );
    }

    let rp = remote_path.map(|p| p.to_string()).unwrap_or_else(|| {
        if is_windows {
            r"C:\rxt\rxt.exe".to_string()
        } else {
            "/usr/local/bin/rxt".to_string()
        }
    });

    if is_windows {
        let exe_name = Path::new(&rp)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "rxt".into());
        let kill = format!(
            r#"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -Command "Stop-Process -Name {} -Force -ErrorAction SilentlyContinue""#,
            exe_name
        );
        let _ = ssh_run(use_native, &target, &config, &password, &kill);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    if !is_windows {
        let tmp_remote = "/tmp/_rxt_deploy_tmp";
        scp_to(
            use_native,
            binary,
            &target,
            &config,
            &password,
            tmp_remote,
        )?;
        // 优先 sudo 装系统路径，失败则装到用户目录并尽量 cp
        let install = format!(
            "chmod +x {tmp} && (sudo mv {tmp} {rp} 2>/dev/null || sudo cp {tmp} {rp} 2>/dev/null || (mkdir -p \"$HOME/.local/bin\" && cp {tmp} \"$HOME/.local/bin/rxt\" && chmod 755 \"$HOME/.local/bin/rxt\"; cp {tmp} {rp} 2>/dev/null || true)) && (sudo chmod 755 {rp} 2>/dev/null || chmod 755 {rp} 2>/dev/null || true); test -x {rp} || test -x \"$HOME/.local/bin/rxt\"",
            tmp = tmp_remote,
            rp = rp
        );
        let out = ssh_run(use_native, &target, &config, &password, &install)?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("远端安装失败: {}", err.trim());
        }
    } else {
        let scp_target_path = rp.replace('\\', "/");
        scp_to(
            use_native,
            binary,
            &target,
            &config,
            &password,
            &scp_target_path,
        )?;
    }

    // 验证大小
    let verify_cmd = if is_windows {
        let ps_script = format!("(Get-Item '{}').Length", rp);
        let b64 = base64::engine::general_purpose::STANDARD.encode(
            ps_script
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
        format!(r#"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -EncodedCommand {}"#, b64)
    } else {
        format!(
            "stat -c %s '{}' 2>/dev/null || stat -c %s \"$HOME/.local/bin/rxt\"",
            rp
        )
    };
    let v_out = ssh_run(use_native, &target, &config, &password, &verify_cmd)?;
    let actual: u64 = String::from_utf8_lossy(&v_out.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    if actual != local_size {
        anyhow::bail!("字节不一致: 本地 {} / 远程 {}", local_size, actual);
    }

    Ok(rp)
}

fn scp_to(
    use_native: bool,
    local: &Path,
    target: &str,
    config: &crate::hosts::HostConfig,
    password: &str,
    remote_path: &str,
) -> anyhow::Result<()> {
    if use_native {
        let dest = format!("{}:{}", target, remote_path);
        let mut c = Command::new("scp");
        for a in ssh_base_args() {
            c.arg(a);
        }
        c.arg(local).arg(&dest);
        let out = run_cmd(c)?;
        if !out.status.success() {
            anyhow::bail!(
                "scp 失败: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    } else {
        let scp = format!(
            "sshpass -e scp -o StrictHostKeyChecking=no {} {}@{}:{}",
            local.display(),
            config.user,
            config.host,
            remote_path
        );
        let out = Command::new("bash")
            .arg("-c")
            .arg(&scp)
            .env("SSHPASS", password)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "scp 失败: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }
}

fn ssh_run(
    use_native: bool,
    target: &str,
    config: &crate::hosts::HostConfig,
    password: &str,
    remote_cmd: &str,
) -> anyhow::Result<Output> {
    if use_native {
        let mut c = Command::new("ssh");
        for a in ssh_base_args() {
            c.arg(a);
        }
        c.arg(target).arg(remote_cmd);
        run_cmd(c)
    } else {
        let cmd = format!(
            "sshpass -e ssh -o StrictHostKeyChecking=no {}@{} {}",
            config.user,
            config.host,
            shell_single_quote(remote_cmd)
        );
        Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .env("SSHPASS", password)
            .output()
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
