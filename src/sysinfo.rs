//! 系统信息 — OS / CPU / 内存 / 磁盘 / 网络
//!
//! 跨平台(Win/Linux/macOS)。用 sysinfo crate,一次实现三平台通吃。
//! 对标 PowerShell: Get-CimInstance Win32_OperatingSystem / Get-Process / Get-Counter
//!
//! 用法:
//!   rxt sysinfo              # 全部信息(人类可读)
//!   rxt sysinfo --json       # 结构化 JSON
//!   rxt sysinfo cpu          # 仅 CPU
//!   rxt sysinfo mem          # 仅内存
//!   rxt sysinfo disk         # 仅磁盘
//!   rxt sysinfo os           # 仅 OS/主机
//!   rxt sysinfo net          # 仅网络接口

use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};

pub fn run(section: &str, json_output: bool) -> anyhow::Result<()> {
    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    // CPU 使用率需要两轮采样间隔才有意义
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    if json_output {
        let out = build_json(&sys, section)?;
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    match section {
        "all" | "" => {
            print_os(&sys);
            println!();
            print_cpu(&sys);
            println!();
            print_mem(&sys);
            println!();
            print_disk();
            println!();
            print_net();
        }
        "os" => print_os(&sys),
        "cpu" => print_cpu(&sys),
        "mem" => print_mem(&sys),
        "disk" => print_disk(),
        "net" => print_net(),
        other => anyhow::bail!(
            "unknown section '{}', valid: all os cpu mem disk net",
            other
        ),
    }
    Ok(())
}

fn print_os(sys: &System) {
    println!("== OS / 主机 ==");
    println!(
        "  系统:   {} {}",
        System::name().unwrap_or_default(),
        System::long_os_version().unwrap_or_default()
    );
    println!("  主机名: {}", System::host_name().unwrap_or_default());
    println!("  架构:   {}", std::env::consts::ARCH);
    println!("  运行:   {}", format_uptime(System::uptime()));
}

fn print_cpu(sys: &System) {
    println!("== CPU ==");
    let cpus = sys.cpus();
    if let Some(c0) = cpus.first() {
        println!("  型号:   {}", c0.brand());
        println!("  频率:   {} MHz", c0.frequency());
    }
    println!("  核心数: {}", cpus.len());
    let total_usage: f32 =
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len().max(1) as f32;
    println!("  总使用: {:.1}%", total_usage);
    if cpus.len() > 1 {
        let per: Vec<String> = cpus
            .iter()
            .take(16)
            .map(|c| format!("{:.0}", c.cpu_usage()))
            .collect();
        println!("  各核:   {}%", per.join("/"));
    }
}

fn print_mem(sys: &System) {
    println!("== 内存 ==");
    println!("  总内存: {}", format_bytes(sys.total_memory()));
    println!(
        "  已用:   {} ({:.0}%)",
        format_bytes(sys.used_memory()),
        pct(sys.used_memory(), sys.total_memory())
    );
    if sys.total_swap() > 0 {
        println!(
            "  交换:   {}/{} ({:.0}%)",
            format_bytes(sys.used_swap()),
            format_bytes(sys.total_swap()),
            pct(sys.used_swap(), sys.total_swap())
        );
    }
}

fn print_disk() {
    println!("== 磁盘 ==");
    let disks = Disks::new_with_refreshed_list();
    let mut shown = 0;
    for d in disks.list() {
        let name = d.name().to_string_lossy();
        let mp = d.mount_point().display();
        let total = d.total_space();
        let used = total - d.available_space();
        println!(
            "  {:<14} {:>10} / {:>10} ({:>5.0}%)  {}",
            &name.to_string().chars().take(14).collect::<String>(),
            format_bytes(used),
            format_bytes(total),
            pct(used, total),
            mp
        );
        shown += 1;
    }
    if shown == 0 {
        println!("  (无)");
    }
}

fn print_net() {
    println!("== 网络接口 ==");
    let nets = Networks::new_with_refreshed_list();
    let mut shown = 0;
    for (name, data) in nets.list() {
        println!(
            "  {:<16} ↓{}/s  ↑{}/s",
            name,
            format_bytes(data.received()),
            format_bytes(data.transmitted())
        );
        shown += 1;
    }
    if shown == 0 {
        println!("  (无)");
    }
}

fn build_json(sys: &System, section: &str) -> anyhow::Result<serde_json::Value> {
    let cpus = sys.cpus();
    let cpu_arr: Vec<serde_json::Value> = cpus
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name(),
                "brand": c.brand(),
                "usage_pct": (c.cpu_usage() * 10.0).round() / 10.0,
                "freq_mhz": c.frequency(),
            })
        })
        .collect();

    let os_obj = serde_json::json!({
        "name": System::name(),
        "long_version": System::long_os_version(),
        "host_name": System::host_name(),
        "arch": std::env::consts::ARCH,
        "uptime_sec": System::uptime(),
    });
    let cpu_obj = serde_json::json!({
        "cores": cpus.len(),
        "model": cpus.first().map(|c| c.brand()).unwrap_or(""),
        "freq_mhz": cpus.first().map(|c| c.frequency()).unwrap_or(0),
        "total_usage_pct": (cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len().max(1) as f32 * 10.0).round() / 10.0,
        "per_core": cpu_arr,
    });
    let mem_obj = serde_json::json!({
        "total_bytes": sys.total_memory(),
        "used_bytes": sys.used_memory(),
        "used_pct": (pct(sys.used_memory(), sys.total_memory()) * 10.0).round() / 10.0,
        "swap_total_bytes": sys.total_swap(),
        "swap_used_bytes": sys.used_swap(),
    });
    let disk_arr: Vec<serde_json::Value> = Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|d| {
            let total = d.total_space();
            let used = total - d.available_space();
            serde_json::json!({
                "name": d.name().to_string_lossy(),
                "mount_point": d.mount_point().display().to_string(),
                "total_bytes": total,
                "used_bytes": used,
                "available_bytes": d.available_space(),
                "used_pct": (pct(used, total) * 10.0).round() / 10.0,
            })
        })
        .collect();
    let net_arr: Vec<serde_json::Value> = Networks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|(n, d)| {
            serde_json::json!({
                "name": n, "rx_bytes": d.received(), "tx_bytes": d.transmitted(),
            })
        })
        .collect();

    Ok(match section {
        "all" | "" => serde_json::json!({
            "os": os_obj, "cpu": cpu_obj, "memory": mem_obj, "disks": disk_arr, "networks": net_arr,
        }),
        "os" => os_obj,
        "cpu" => cpu_obj,
        "mem" => mem_obj,
        "disk" => serde_json::json!({"disks": disk_arr}),
        "net" => serde_json::json!({"networks": net_arr}),
        other => anyhow::bail!("unknown section '{}'", other),
    })
}

fn format_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;
    if b >= TB {
        format!("{:.1}T", b as f64 / TB as f64)
    } else if b >= GB {
        format!("{:.1}G", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.0}M", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.0}K", b as f64 / KB as f64)
    } else {
        format!("{}B", b)
    }
}

fn format_uptime(sec: u64) -> String {
    let d = sec / 86400;
    let h = (sec % 86400) / 3600;
    let m = (sec % 3600) / 60;
    if d > 0 {
        format!("{}d{}h{}m", d, h, m)
    } else if h > 0 {
        format!("{}h{}m", h, m)
    } else {
        format!("{}m", m)
    }
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}
