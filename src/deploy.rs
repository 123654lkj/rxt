//! deploy — 一键部署二进制到远程机器
//! 用系统 scp 传输大文件(比 exec+base64 可靠), rxt exec 只做 kill/验证

use std::path::PathBuf;
use std::process::Command;
use base64::Engine;
use crate::hosts::{HostsFile, RemoteOs};

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
            Ok(rp) => { println!("✅ -> {}", rp); ok.push(host_name.clone()); }
            Err(e) => { println!("❌ {}", e); fail.push((host_name.clone(), e.to_string())); }
        }
    }

    println!("\n{}", "─".repeat(50));
    println!("✅ 成功: {} 台 ({})", ok.len(), ok.join(", "));
    if !fail.is_empty() {
        println!("❌ 失败: {} 台", fail.len());
        for (h, e) in &fail { println!("   {}: {}", h, e); }
    }
    if !fail.is_empty() { anyhow::bail!("{} 台部署失败", fail.len()); }
    Ok(())
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

    // 探测远端 OS (用 sshpass + 一条命令)
    let os_probe = format!("sshpass -e ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {}@{} 'uname -s 2>/dev/null || echo WIN'", config.user, config.host);
    let probe_out = Command::new("bash").arg("-c").arg(&os_probe).env("SSHPASS", &password).output()?;
    let is_windows = !String::from_utf8_lossy(&probe_out.stdout).to_lowercase().contains("linux");

    // v0.4.2: 交叉平台检查 - 本地二进制格式必须匹配目标 OS
    let local_bytes_head = std::fs::read(binary).map_err(|e| anyhow::anyhow!("读本地文件: {}", e))?;
    let is_local_pe = local_bytes_head.len() >= 2 && &local_bytes_head[..2] == b"MZ"; // Windows PE
    let is_local_elf = local_bytes_head.len() >= 4 && &local_bytes_head[..4] == b"\x7fELF"; // Linux ELF
    if is_windows && !is_local_pe {
        anyhow::bail!("⚠️  跳过: 本地是 {} 二进制, 目标 {} 是 Windows, 需要 PE/exe 格式",
            if is_local_elf { "Linux ELF" } else { "未知" }, host_name);
    }
    if !is_windows && !is_local_elf {
        anyhow::bail!("⚠️  跳过: 本地是 {} 二进制, 目标 {} 是 Linux, 需要 ELF 格式",
            if is_local_pe { "Windows PE" } else { "未知" }, host_name);
    }

    // 确定远程路径
    let rp = remote_path.map(|p| p.to_string()).unwrap_or_else(|| {
        if is_windows { "C:\\rxt\\rxt.exe".to_string() }
        else {
            if String::from_utf8_lossy(&probe_out.stdout).contains("linux") {
                "/usr/local/bin/rxt".to_string()
            } else {
                "/usr/local/bin/rxt".to_string()
            }
        }
    });

    // Windows: 先 kill 占用进程
    if is_windows {
        let exe_name = std::path::Path::new(&rp)
            .file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("rxt".into());
        let kill_cmd = format!(
            "sshpass -e ssh -o StrictHostKeyChecking=no {}@{} 'pwsh -NoProfile -Command \"Stop-Process -Name {} -Force -ErrorAction SilentlyContinue\"'",
            config.user, config.host, exe_name
        );
        let _ = Command::new("bash").arg("-c").arg(&kill_cmd).output();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    // SCP 传输
    if !is_windows {
        // Linux: scp 到 /tmp 再 sudo mv (解决 /usr/local/bin 权限)
        let tmp_remote = "/tmp/_rxt_deploy_tmp";
        let scp = format!("sshpass -e scp -o StrictHostKeyChecking=no {} {}@{}:{}",
            binary.display(), config.user, config.host, tmp_remote);
        let scp_result = Command::new("bash").arg("-c").arg(&scp).env("SSHPASS", &password).output()?;
        if !scp_result.status.success() {
            let err = String::from_utf8_lossy(&scp_result.stderr);
            anyhow::bail!("scp 失败: {}", err.trim());
        }
        // sudo mv + chmod
        let mv = format!("sshpass -e ssh -o StrictHostKeyChecking=no {}@{} 'echo {} | sudo -S mv {} {} && sudo chmod 755 {}'",
            config.user, config.host, password, tmp_remote, rp, rp);
        let mv_result = Command::new("bash").arg("-c").arg(&mv).env("SSHPASS", &password).output()?;
        if !mv_result.status.success() {
            // sudo 可能没装或没权限, 尝试直接 mv
            let mv2 = format!("sshpass -e ssh -o StrictHostKeyChecking=no {}@{} 'mv {} {} 2>/dev/null || cp {} {}'",
                config.user, config.host, tmp_remote, rp, tmp_remote, rp);
            let _ = Command::new("bash").arg("-c").arg(&mv2).output();
        }
    } else {
        // Windows: 直接 scp (已 kill 进程)
        let scp_target = format!("{}@{}:{}", config.user, config.host, rp.replace('\\', "/"));
        let scp = format!("sshpass -e scp -o StrictHostKeyChecking=no {} {}",
            binary.display(), scp_target);
        let scp_result = Command::new("bash").arg("-c").arg(&scp).env("SSHPASS", &password).output()?;
        if !scp_result.status.success() {
            let err = String::from_utf8_lossy(&scp_result.stderr);
            anyhow::bail!("scp 失败: {}", err.trim());
        }
    }

    // 验证字节大小
    let verify = if is_windows {
        // 用 EncodedCommand 避免 pwsh 引号转义地狱
        let ps_script = format!("(Get-Item '{}').Length", rp);
        let b64 = base64::engine::general_purpose::STANDARD.encode(ps_script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect::<Vec<u8>>());
        format!("sshpass -e ssh -o StrictHostKeyChecking=no {}@{} 'pwsh -NoProfile -EncodedCommand {}'",
            config.user, config.host, b64)
    } else {
        format!("sshpass -e ssh -o StrictHostKeyChecking=no {}@{} 'stat -c %s \"{}\"'",
            config.user, config.host, rp)
    };
    let v_result = Command::new("bash").arg("-c").arg(&verify).env("SSHPASS", &password).output()?;
    let actual: u64 = String::from_utf8_lossy(&v_result.stdout).trim().parse().unwrap_or(0);
    if actual != local_size {
        anyhow::bail!("字节不一致: 本地 {} / 远程 {}", local_size, actual);
    }

    Ok(rp)
}
