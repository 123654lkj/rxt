//! repeat — 轮询重试直到成功或超时
//!
//! 解决: 等服务启动/等文件出现/等端口通/等命令成功
//! 运维 & AI 的刚需——不用手写 while sleep 循环。
//!
//! 用法:
//!   rxt repeat "curl -s http://localhost:8080"           # 直到 curl 成功
//!   rxt repeat "curl http://localhost:8080" --timeout 60  # 60 秒超时
//!   rxt repeat --file output.log                          # 等文件出现
//!   rxt repeat --port 5432 --timeout 30                   # 等端口可连
//!   rxt repeat "make test" --tries 3                      # 最多试 3 次
//!   rxt repeat --ping 192.168.1.1                         # ping 通为止

use std::process::Command;
use std::time::{Duration, Instant};
use std::net::TcpStream;
use std::path::Path;

pub fn run(cmd: Option<&str>, file: Option<&str>, port: Option<&str>, ping: Option<&str>,
           timeout_secs: u64, interval_ms: u64, tries: usize) -> anyhow::Result<()> {

    // 用 shell 执行命令(支持管道/重定向)
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };

    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let interval = Duration::from_millis(interval_ms);
    let max_tries = if tries > 0 { Some(tries) } else { None };
    let mut attempt = 0usize;

    loop {
        attempt += 1;
        if let Some(mt) = max_tries { if attempt > mt { break; } }
        if start.elapsed() >= timeout { break; }

        let ok = if let Some(f) = file {
            Path::new(f).exists()
        } else if let Some(p) = port {
            let host_port = if p.contains(':') { p.to_string() } else { format!("127.0.0.1:{}", p) };
            TcpStream::connect_timeout(
                &host_port.parse().map_err(|e| anyhow::anyhow!("无效端口参数 {}: {}", p, e))?,
                Duration::from_secs(2),
            ).is_ok()
        } else if let Some(h) = ping {
            // 简单 ping: 尝试 TCP 连 80 端口或直接 ping 命令
            ping_check(h)
        } else if let Some(c) = cmd {
            let status = Command::new(shell).arg(flag).arg(c)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            status.map(|s| s.success()).unwrap_or(false)
        } else {
            anyhow::bail!("需要指定命令 / --file / --port / --ping 之一");
        };

        if ok {
            let elapsed = start.elapsed();
            println!("✓ 成功! 第 {} 次尝试, 用时 {:.1}s",
                attempt, elapsed.as_secs_f64());
            return Ok(());
        }

        let remaining = timeout.saturating_sub(start.elapsed());
        let wait = interval.min(remaining);
        eprint!("\r⏳ 第 {} 次失败, {:.0}s/{:.0}s, {}ms 后重试...",
            attempt, start.elapsed().as_secs_f64(), timeout.as_secs_f64(), wait.as_millis());
        std::thread::sleep(wait);
    }

    // 超时或用尽尝试
    eprintln!();
    if let Some(mt) = max_tries {
        anyhow::bail!("✗ 失败: 已用尽 {} 次尝试", mt);
    } else {
        anyhow::bail!("✗ 超时: {} 秒内未成功", timeout_secs);
    }
}

fn ping_check(host: &str) -> bool {
    // 优先用系统 ping
    let ping = if cfg!(windows) { "ping" } else { "ping" };
    let count_flag = if cfg!(windows) { "-n" } else { "-c" };
    let status = Command::new(ping).args([count_flag, "1", "-W", "2", host])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}
