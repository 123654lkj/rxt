//! 进程管理 — 列表 / 查杀 / 排序
//! 对标 PowerShell: Get-Process / Stop-Process
//! 跨平台: sysinfo crate 提供

use sysinfo::{System, ProcessRefreshKind, Users, ProcessesToUpdate};

pub fn run(name_filter: Option<&str>, kill: Option<&str>, top: usize, sort: &str, tree: bool, json: bool) -> anyhow::Result<()> {
    let mut sys = System::new_all();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    // kill 模式
    if let Some(target) = kill {
        return do_kill(&sys, target);
    }

    let mut procs: Vec<&sysinfo::Process> = sys.processes().values().collect();

    // 名称过滤(支持 * 通配)
    if let Some(pat) = name_filter {
        procs.retain(|p| {
            let pname = p.name().to_string_lossy().to_lowercase();
            glob_match(&pat.to_lowercase(), &pname)
        });
    }

    // 排序
    match sort {
        "cpu" => procs.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal)),
        "pid" => procs.sort_by_key(|p| p.pid().as_u32()),
        "name" => procs.sort_by_key(|p| p.name().to_string_lossy().to_lowercase()),
        _ => procs.sort_by(|a, b| b.memory().cmp(&a.memory())),
    }

    // top 限制
    if top > 0 && procs.len() > top { procs.truncate(top); }

    let users = Users::new_with_refreshed_list();

    if json {
        let arr: Vec<serde_json::Value> = procs.iter().map(|p| {
            serde_json::json!({
                "pid": p.pid().as_u32(),
                "name": p.name().to_string_lossy(),
                "cpu_pct": (p.cpu_usage() * 10.0).round() / 10.0,
                "mem_bytes": p.memory(),
                "user": user_of(&users, p),
                "cmd": p.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect::<Vec<_>>().join(" "),
                "start_sec": p.start_time(),
            })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    // 人类可读
    if tree {
        print_tree(&sys);
        return Ok(());
    }

    println!("{:>7} {:>6} {:>10} {:<10} {:<8} {}",
             "PID", "CPU%", "MEM", "USER", "ELAPSED", "NAME");
    println!("{}", "-".repeat(70));
    let now = System::uptime();
    let boot = System::boot_time();
    let now_epoch = boot.saturating_add(now);
    for p in &procs {
        let user = user_of(&users, p);
        // start_time 是绝对时间戳(UNIX epoch), elapsed = 当前epoch - 启动epoch
        let elapsed = format_elapsed(now_epoch.saturating_sub(p.start_time()));
        println!("{:>7} {:>5.1}% {:>10} {:<10} {:<8} {}",
                 p.pid().as_u32(), p.cpu_usage(), fmt_bytes(p.memory()),
                 user, elapsed, p.name().to_string_lossy());
    }
    println!("\n共 {} 个进程{}", procs.len(), if name_filter.is_some() { " (已过滤)" } else { "" });
    Ok(())
}

fn do_kill(sys: &System, target: &str) -> anyhow::Result<()> {
    // 数字 = PID,否则按名称
    let mut killed = 0;
    if let Ok(pid) = target.parse::<u32>() {
        if let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) {
            let name = p.name().to_string_lossy().to_string();
            if p.kill() {
                println!("已终止 PID {} ({})", pid, name);
                killed += 1;
            } else {
                anyhow::bail!("终止失败 PID {} (权限不足?)", pid);
            }
        } else {
            anyhow::bail!("找不到 PID {}", pid);
        }
    } else {
        // 按名称
        for (pid, p) in sys.processes() {
            if p.name().to_string_lossy().eq_ignore_ascii_case(target) {
                if p.kill() {
                    println!("已终止 PID {} ({})", pid.as_u32(), target);
                    killed += 1;
                }
            }
        }
        if killed == 0 { anyhow::bail!("找不到进程: {}", target); }
    }
    println!("共终止 {} 个进程", killed);
    Ok(())
}

fn print_tree(sys: &System) {
    // 找根进程(ppid=0 或不存在)
    let mut roots: Vec<_> = sys.processes().values()
        .filter(|p| {
            let ppid = p.parent();
            ppid.is_none() || sys.process(ppid.unwrap()).is_none()
        })
        .collect();
    roots.sort_by_key(|p| p.pid().as_u32());
    for root in roots {
        print_subtree(sys, root, 0);
    }
}

fn print_subtree(sys: &System, p: &sysinfo::Process, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{} ({} {})",
             indent, p.name().to_string_lossy(), p.pid().as_u32(), fmt_bytes(p.memory()));
    let mut children: Vec<_> = sys.processes().values()
        .filter(|c| c.parent() == Some(p.pid()))
        .collect();
    children.sort_by_key(|c| c.pid().as_u32());
    for child in children {
        print_subtree(sys, child, depth + 1);
    }
}

fn user_of(users: &Users, p: &sysinfo::Process) -> String {
    if let Some(uid) = p.user_id() {
        if let Some(u) = users.get_user_by_id(uid) {
            return u.name().to_string();
        }
    }
    "-".to_string()
}

fn fmt_bytes(b: u64) -> String {
    const MB: u64 = 1024 * 1024; const GB: u64 = MB * 1024;
    if b >= GB { format!("{:.1}G", b as f64 / GB as f64) }
    else if b >= MB { format!("{:.0}M", b as f64 / MB as f64) }
    else { format!("{}K", b / 1024) }
}

fn format_elapsed(sec: u64) -> String {
    let d = sec / 86400; let h = (sec % 86400) / 3600; let m = (sec % 3600) / 60;
    if d > 0 { format!("{}d{}h", d, h) }
    else if h > 0 { format!("{}h{}m", h, m) }
    else { format!("{}m", m) }
}

/// 简易 glob: * 匹配任意, ? 匹配单字符
pub fn glob_match_pub(pat: &str, s: &str) -> bool {
    glob_match(pat, s)
}
fn glob_match(pat: &str, s: &str) -> bool {
    let pb: Vec<char> = pat.chars().collect();
    let sb: Vec<char> = s.chars().collect();
    glob_rec(&pb, 0, &sb, 0)
}
fn glob_rec(p: &[char], pi: usize, s: &[char], si: usize) -> bool {
    if pi == p.len() { return si == s.len(); }
    match p[pi] {
        '*' => {
            // * 匹配 0 到剩余全部
            for k in si..=s.len() {
                if glob_rec(p, pi + 1, s, k) { return true; }
            }
            false
        }
        '?' => si < s.len() && glob_rec(p, pi + 1, s, si + 1),
        c => si < s.len() && s[si] == c && glob_rec(p, pi + 1, s, si + 1),
    }
}
