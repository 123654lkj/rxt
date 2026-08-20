//! version — 批量查询远程机器 rxt 版本 + 一致性检测
//!
//! rxt version                    # 本地版本
//! rxt version --host xian        # 查单台
//! rxt version --group all        # 批量查所有 + 检测不一致

use crate::hosts::HostsFile;
use crate::remote::RemoteChannel;

pub fn run_local() -> anyhow::Result<()> {
    println!("rxt {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

pub fn run_remote(target: &str, is_group: bool) -> anyhow::Result<()> {
    let hosts = HostsFile::load()?;

    let mut target_hosts: Vec<String> = Vec::new();
    if is_group {
        let members = hosts.get_group_members(target)?;
        target_hosts.extend(members);
    } else {
        target_hosts.push(target.to_string());
    }

    // 加上本机
    println!("  本机      rxt {}", env!("CARGO_PKG_VERSION"));

    let mut versions: Vec<(String, String)> = Vec::new();
    versions.push(("本机".to_string(), env!("CARGO_PKG_VERSION").to_string()));

    for host_name in &target_hosts {
        match RemoteChannel::connect(host_name) {
            Ok(mut remote) => {
                // v0.7.5: 复用 remote.probe_rxt_path() 跨平台探测, 消除重复的 PowerShell 逻辑
                let ver = match remote.probe_rxt_path() {
                    Some(path) => {
                        match remote.exec(&format!("{} --version", path)) {
                            Ok(out) => out.trim().lines().next().unwrap_or("?").to_string(),
                            Err(_) => "ERROR".to_string(),
                        }
                    }
                    None => "?".to_string(),  // 远端无 rxt
                };
                println!("  {:<10} {}", host_name, ver);
                versions.push((host_name.clone(), ver));
            }
            Err(e) => {
                println!("  {:<10} ❌ 连接失败: {}", host_name, e);
                versions.push((host_name.clone(), "UNREACHABLE".to_string()));
            }
        }
    }

    // 一致性检测
    let unique: std::collections::HashSet<&String> = versions.iter().map(|(_, v)| v).collect();
    println!("\n{}", "─".repeat(40));
    if unique.len() == 1 {
        println!("✅ 全部一致: {}", versions[0].1);
    } else {
        println!("⚠️  版本不一致! 发现 {} 个不同版本:", unique.len());
        // 按版本分组显示
        use std::collections::BTreeMap;
        let mut by_ver: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
        for (host, ver) in &versions {
            by_ver.entry(ver).or_default().push(host);
        }
        for (ver, hs) in &by_ver {
            println!("   {} ← {}", ver, hs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }
    }

    Ok(())
}
