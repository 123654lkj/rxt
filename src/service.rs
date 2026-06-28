//! Windows 服务管理
//! 对标 PowerShell: Get/Start/Stop/Restart-Service
//!
//! 策略: 包装 sc.exe (Windows 自带,稳定),而非直接调 Win32 API。
//! 跨平台: 非 Windows 报错提示。
//!
//! 用法:
//!   rxt service                 # 列出全部服务
//!   rxt service --name "sql*"   # 按名称过滤(通配)
//!   rxt service --running       # 只看运行中的
//!   rxt service --start Spooler
//!   rxt service --stop Spooler
//!   rxt service --json

use std::process::Command;

pub fn run(name: Option<&str>, start: Option<&str>, stop: Option<&str>, running: bool, json: bool) -> anyhow::Result<()> {
    if cfg!(not(target_os = "windows")) {
        anyhow::bail!("service 命令仅支持 Windows");
    }

    // 启停操作优先
    if let Some(svc) = start {
        return control(svc, "start");
    }
    if let Some(svc) = stop {
        return control(svc, "stop");
    }

    // 列表
    let out = Command::new("sc").args(["query", "type=", "service", "state=", "all"])
        .output().map_err(|e| anyhow::anyhow!("sc.exe 调用失败: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let svcs = parse_services(&text);

    let mut filtered: Vec<&Service> = svcs.iter().collect();
    if let Some(pat) = name {
        filtered.retain(|s| crate::ps::glob_match_pub(&pat.to_lowercase(), &s.name.to_lowercase()));
    }
    if running {
        filtered.retain(|s| s.state.eq_ignore_ascii_case("running"));
    }

    if json {
        let arr: Vec<_> = filtered.iter().map(|s| serde_json::json!({
            "name": s.name, "display": s.display, "state": s.state, "pid": s.pid,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    println!("{:<32} {:<12} {:>7} {}", "NAME", "STATE", "PID", "DISPLAY");
    println!("{}", "-".repeat(90));
    for s in &filtered {
        let pid = if s.pid > 0 { s.pid.to_string() } else { "-".into() };
        println!("{:<32} {:<12} {:>7} {}", s.name, s.state, pid, s.display);
    }
    println!("\n共 {} 个服务", filtered.len());
    Ok(())
}

fn control(svc: &str, action: &str) -> anyhow::Result<()> {
    let out = Command::new("sc").args([action, svc]).output()
        .map_err(|e| anyhow::anyhow!("sc.exe 调用失败: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // sc 成功时 stderr 有 "[SC] ControlRequest 成功" 之类
    if out.status.success() || stderr.contains("成功") || stdout.contains("SUCCESS") {
        println!("✓ {} -> {}", svc, action);
        // 查一下新状态
        if let Ok(st) = query_state(svc) {
            println!("  当前状态: {}", st);
        }
        Ok(())
    } else {
        anyhow::bail!("{} {} 失败: {}", action, svc, stderr.trim());
    }
}

fn query_state(svc: &str) -> anyhow::Result<String> {
    let out = Command::new("sc").args(["query", svc]).output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("state") {
            if let Some(idx) = line.find(':') {
                let rest = line[idx+1..].trim();
                // "4 RUNNING (stop-pending, ...)" 取冒号后第一段
                return Ok(rest.split_whitespace().nth(1).unwrap_or(rest).to_string());
            }
        }
    }
    anyhow::bail!("无法获取 {} 状态", svc)
}

#[derive(Default)]
struct Service {
    name: String,
    display: String,
    state: String,
    pid: u32,
}

fn parse_services(text: &str) -> Vec<Service> {
    // sc query 输出格式:
    // SERVICE_NAME: name
    // DISPLAY_NAME: desc
    //         TYPE               : ...
    //         STATE              : 4 RUNNING (STOPPABLE...)
    //                             (pid) 或无
    let mut svcs = Vec::new();
    let mut cur = Service { name: String::new(), display: String::new(), state: String::new(), pid: 0 };
    let mut have = false;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("SERVICE_NAME:") {
            if have { svcs.push(std::mem::take(&mut cur)); }
            cur.name = rest.trim().to_string();
            have = true;
        } else if let Some(rest) = l.strip_prefix("DISPLAY_NAME:") {
            cur.display = rest.trim().to_string();
        } else if l.to_lowercase().contains("state") {
            // "STATE              : 4  RUNNING  (STOPPABLE...)"
            if let Some(idx) = l.find(':') {
                let after = l[idx+1..].trim();
                cur.state = after.split_whitespace().nth(1).unwrap_or(after).to_string();
            }
        } else if l.to_lowercase().contains("pid") {
            // 注意 sc query 默认不含 pid;sc queryex 才有。这里兼容
            if let Some(idx) = l.find(':') {
                let after = l[idx+1..].trim();
                if let Ok(pid) = after.split_whitespace().next().unwrap_or("0").parse::<u32>() {
                    cur.pid = pid;
                }
            }
        }
    }
    if have { svcs.push(cur); }
    svcs
}
