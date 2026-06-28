//! bench — 性能基准(跑 N 次取统计 + 对比)
//!
//! 哪个快? 跑 N 次取 min/max/avg/p95, --vs 直接对比两个命令。
//!
//! 用法:
//!   rxt bench "rxt grep foo ." -n 10
//!   rxt bench "rxt find . --name *.py" "rg --files *.py" -n 5
//!   rxt bench "cmd A" --vs "cmd B" --warmup 2

use std::process::Command;
use std::time::{Duration, Instant};

pub fn run(cmds: &[String], runs: usize, warmup: usize, json: bool) -> anyhow::Result<()> {
    if cmds.is_empty() {
        anyhow::bail!("需要命令(用引号包裹)");
    }
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
    let mut results: Vec<(String, BenchResult)> = Vec::new();

    for cmd in cmds {
        // warmup
        for _ in 0..warmup {
            let _ = Command::new(shell).arg(flag).arg(cmd)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()).status();
        }
        // 跑 N 次
        let mut times: Vec<Duration> = Vec::with_capacity(runs);
        for i in 0..runs {
            let start = Instant::now();
            let status = Command::new(shell).arg(flag).arg(cmd)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()).status();
            let elapsed = start.elapsed();
            let ok = status.map(|s| s.success()).unwrap_or(false);
            eprint!("\r  [{}/{}] {:.3}s {}", i+1, runs, elapsed.as_secs_f64(), if ok {"✓"} else {"✗"});
            if !ok {
                eprintln!("\n⚠ 命令失败: {}", cmd);
            }
            times.push(elapsed);
        }
        eprintln!();
        results.push((cmd.clone(), compute(&times)));
    }

    if json {
        let arr: Vec<_> = results.iter().map(|(c, r)| serde_json::json!({
            "cmd": c, "runs": runs, "min_ms": r.min.as_secs_f64()*1000.0,
            "avg_ms": r.avg.as_secs_f64()*1000.0, "max_ms": r.max.as_secs_f64()*1000.0,
            "p95_ms": r.p95.as_secs_f64()*1000.0, "stdev_ms": r.stdev.as_secs_f64()*1000.0,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    // 表格输出
    println!("\n📊 基准结果 ({} 次运行, {} 次 warmup)", runs, warmup);
    println!("{:>10} {:>10} {:>10} {:>10} {:>10}  {}", "MIN", "AVG", "MAX", "P95", "STDEV", "CMD");
    println!("{}", "-".repeat(80));
    for (cmd, r) in &results {
        println!("{:>9.2}ms {:>9.2}ms {:>9.2}ms {:>9.2}ms {:>9.2}ms  {}",
            r.min.as_secs_f64()*1000.0, r.avg.as_secs_f64()*1000.0,
            r.max.as_secs_f64()*1000.0, r.p95.as_secs_f64()*1000.0, r.stdev.as_secs_f64()*1000.0,
            shorten(cmd, 40));
    }

    // 对比
    if results.len() >= 2 {
        let base = &results[0].1;
        println!("\n⚡ 对比 (以第 1 个为基准):");
        for (cmd, r) in results.iter().skip(1) {
            let speedup = base.avg.as_secs_f64() / r.avg.as_secs_f64();
            let faster = speedup > 1.0;
            println!("  {} {:.2}x {} (avg {:.2}ms vs {:.2}ms)",
                if faster {"🚀 快"} else {"🐢 慢"},
                if faster {speedup} else {1.0/speedup},
                shorten(cmd, 30),
                r.avg.as_secs_f64()*1000.0, base.avg.as_secs_f64()*1000.0);
        }
    }
    Ok(())
}

struct BenchResult {
    min: Duration,
    max: Duration,
    avg: Duration,
    p95: Duration,
    stdev: Duration,
}

fn compute(times: &[Duration]) -> BenchResult {
    let mut sorted: Vec<Duration> = times.to_vec();
    sorted.sort();
    let n = sorted.len();
    let sum: Duration = sorted.iter().sum();
    let avg = sum / n as u32;
    let min = sorted[0];
    let max = sorted[n-1];
    let p95_idx = ((n as f64) * 0.95).ceil() as usize;
    let p95 = sorted[p95_idx.saturating_sub(1).min(n-1)];
    let variance: f64 = sorted.iter().map(|t| {
        let diff = (t.as_secs_f64() - avg.as_secs_f64()).powi(2);
        diff
    }).sum::<f64>() / n as f64;
    let stdev = Duration::from_secs_f64(variance.sqrt());
    BenchResult { min, max, avg, p95, stdev }
}

fn fmt(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1.0 { format!("{:.2}µs", ms*1000.0) }
    else if ms < 1000.0 { format!("{:.1}ms", ms) }
    else { format!("{:.2}s", d.as_secs_f64()) }
}

fn shorten(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}...", &s[..max-3]) }
}
