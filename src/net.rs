//! 网络 — TCP 连接 / 路由 / DNS / 端口检查
//! 对标 PowerShell: Get-NetTCPConnection / Get-NetRoute / Resolve-DnsName / Test-NetConnection
//!
//! 策略:
//! - DNS: 纯 Rust std::net::ToSocketAddrs
//! - TCP 连接/路由/端口: 包装 netstat / route (Windows) 或 /proc (Linux)
//!
//! 用法:
//!   rxt net --resolve example.com
//!   rxt net --conn established
//!   rxt net --conn listen
//!   rxt net --route
//!   rxt net --port 8080

use std::net::ToSocketAddrs;
use std::process::Command;

pub fn run(conn: Option<&str>, resolve: Option<&str>, route: bool, port: Option<&str>, json: bool) -> anyhow::Result<()> {
    if let Some(host) = resolve {
        return do_resolve(host, json);
    }
    if let Some(state) = conn {
        return do_conn(state, json);
    }
    if route {
        return do_route(json);
    }
    if let Some(p) = port {
        return do_port(p);
    }
    anyhow::bail!("需要指定 --conn/--resolve/--route/--port 之一")
}

fn do_resolve(host: &str, json: bool) -> anyhow::Result<()> {
    // 加端口触发 DNS(用 :80 占位)
    let addr = format!("{}:80", host.trim_end_matches(':'));
    let addrs: Vec<_> = addr.to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("DNS 解析失败 {}: {}", host, e))?
        .collect();
    let ips: Vec<String> = addrs.iter().map(|a| a.ip().to_string()).collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"host": host, "addresses": ips}))?);
    } else {
        println!("{} ->", host);
        for ip in &ips { println!("  {}", ip); }
    }
    Ok(())
}

fn do_conn(state_filter: &str, json: bool) -> anyhow::Result<()> {
    if cfg!(target_os = "windows") {
        // netstat -ano
        let out = Command::new("netstat").args(["-ano"]).output()
            .map_err(|e| anyhow::anyhow!("netstat 调用失败: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        parse_netstat(&text, state_filter, json)
    } else {
        // Linux: 解析 /proc/net/tcp
        let tcp = std::fs::read_to_string("/proc/net/tcp").unwrap_or_default();
        parse_proc_tcp(&tcp, state_filter, json)
    }
}

fn parse_netstat(text: &str, state_filter: &str, json: bool) -> anyhow::Result<()> {
    let mut conns: Vec<Conn> = Vec::new();
    let mut in_data = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("Proto") { in_data = true; continue; }
        if !in_data || l.is_empty() { continue; }
        let parts: Vec<&str> = l.split_whitespace().collect();
        if parts.len() < 5 { continue; }
        let proto = parts[0];
        if proto != "TCP" && proto != "TCPv6" { continue; }
        let local = parts.get(1).unwrap_or(&"");
        let remote = parts.get(2).unwrap_or(&"");
        let state = parts.get(3).unwrap_or(&"");
        let pid = parts.get(4).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        conns.push(Conn {
            proto: proto.to_string(),
            local: local.to_string(),
            remote: remote.to_string(),
            state: state.to_string(),
            pid,
        });
    }
    let filt = state_filter.to_lowercase();
    if !filt.is_empty() && filt != "all" {
        conns.retain(|c| c.state.to_lowercase().contains(&filt));
    }
    if json {
        let arr: Vec<_> = conns.iter().map(|c| serde_json::json!({
            "proto": c.proto, "local": c.local, "remote": c.remote, "state": c.state, "pid": c.pid,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!("{:<6} {:<24} {:<24} {:<14} {:>6}", "PROTO", "LOCAL", "REMOTE", "STATE", "PID");
        println!("{}", "-".repeat(80));
        for c in &conns {
            println!("{:<6} {:<24} {:<24} {:<14} {:>6}", c.proto, c.local, c.remote, c.state, c.pid);
        }
        println!("\n共 {} 条连接", conns.len());
    }
    Ok(())
}

fn parse_proc_tcp(text: &str, state_filter: &str, json: bool) -> anyhow::Result<()> {
    let state_map = |s: &str| -> &str {
        match s {
            "01" => "ESTABLISHED", "02" => "SYN_SENT", "03" => "SYN_RECV",
            "04" => "FIN_WAIT1", "05" => "FIN_WAIT2", "06" => "TIME_WAIT",
            "0A" => "LISTEN", "08" => "CLOSE_WAIT", _ => "UNKNOWN",
        }
    };
    let mut conns: Vec<Conn> = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 { continue; }
        let local = f[1].to_string();
        let remote = f[2].to_string();
        let st_code = f[3];
        let pid = f.get(7).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        conns.push(Conn {
            proto: "TCP".into(), local: hex_ip(&local),
            remote: hex_ip(&remote), state: state_map(st_code).to_string(), pid,
        });
    }
    let filt = state_filter.to_uppercase();
    if !filt.is_empty() && filt != "ALL" {
        conns.retain(|c| c.state.contains(&filt));
    }
    if json {
        let arr: Vec<_> = conns.iter().map(|c| serde_json::json!({
            "proto": c.proto, "local": c.local, "remote": c.remote, "state": c.state, "pid": c.pid,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        for c in &conns {
            println!("{:<6} {:<24} {:<24} {:<14} {:>6}", c.proto, c.local, c.remote, c.state, c.pid);
        }
        println!("\n共 {} 条", conns.len());
    }
    Ok(())
}

fn hex_ip(s: &str) -> String {
    // "0100007F:0050" -> "127.0.0.1:80"
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 { return s.to_string(); }
    let hex = parts[0];
    let port = u16::from_str_radix(parts[1], 16).unwrap_or(0);
    if hex.len() == 8 {
        let bytes = (0..4).map(|i| u8::from_str_radix(&hex[i*2..i*2+2], 16).unwrap_or(0)).collect::<Vec<_>>();
        format!("{}.{}.{}.{}:{}", bytes[3], bytes[2], bytes[1], bytes[0], port)
    } else { s.to_string() }
}

fn do_route(json: bool) -> anyhow::Result<()> {
    if cfg!(target_os = "windows") {
        let out = Command::new("route").args(["print", "-4"]).output()?;
        println!("{}", String::from_utf8_lossy(&out.stdout));
    } else {
        let out = Command::new("ip").args(["route"]).output()?;
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }
    let _ = json;
    Ok(())
}

fn do_port(port: &str) -> anyhow::Result<()> {
    let p: u16 = port.parse().map_err(|_| anyhow::anyhow!("无效端口: {}", port))?;
    // 检查本地是否监听
    if cfg!(target_os = "windows") {
        let out = Command::new("netstat").args(["-ano", "-p", "TCP"]).output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let listen: Vec<&str> = text.lines().filter(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            parts.len() >= 4 && parts[3].eq_ignore_ascii_case("LISTENING")
                && parts.iter().any(|f| f.ends_with(&format!(":{}", p)))
        }).collect();
        if listen.is_empty() {
            println!("端口 {} 无监听", p);
        } else {
            println!("端口 {} 正在监听 ({} 条)", p, listen.len());
            for l in listen { println!("  {}", l.trim()); }
        }
    } else {
        let out = Command::new("ss").args(["-tlnp"]).output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let hit: Vec<&str> = text.lines().filter(|l| l.contains(&format!(":{}", p))).collect();
        if hit.is_empty() { println!("端口 {} 无监听", p); }
        else { println!("端口 {} 监听中:", p); for l in hit { println!("  {}", l); } }
    }
    Ok(())
}

struct Conn {
    proto: String,
    local: String,
    remote: String,
    state: String,
    pid: u32,
}
