//! watch-run — 文件变化时自动重跑命令
//!
//! 替代 nodemon/entr,零依赖(notify 已有)。开发循环神器:
//! 改代码 → 自动编译/测试/重启。
//!
//! 用法:
//!   rxt watch-run "cargo build" --ext rs
//!   rxt watch-run "python test.py" --ext py
//!   rxt watch-run "rxt snapshot ." --debounce 2000
//!   rxt watch-run "make" src/ tests/ --ext "c,h"

use notify::{Config as NotifyConfig, Event, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

pub fn run(
    cmd: &str,
    paths: &[String],
    exts: &str,
    debounce_ms: u64,
    run_on_start: bool,
) -> anyhow::Result<()> {
    let watch_paths: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.iter().map(PathBuf::from).collect()
    };

    let ext_filter: Vec<String> = exts
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().trim_start_matches('.').to_lowercase())
        .collect();

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        NotifyConfig::default(),
    )?;

    for p in &watch_paths {
        watcher.watch(p, RecursiveMode::Recursive)?;
    }
    println!(
        "👀 watching: {} (exts: [{}], debounce: {}ms)",
        watch_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        if ext_filter.is_empty() {
            "all".into()
        } else {
            ext_filter.join(",")
        },
        debounce_ms
    );
    println!("▶ cmd: {}\n", cmd);

    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let run_cmd = || {
        let start = Instant::now();
        let status = Command::new(shell)
            .arg(flag)
            .arg(cmd)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status();
        let elapsed = start.elapsed();
        match status {
            Ok(s) => {
                let mark = if s.success() {
                    "✓".to_string()
                } else {
                    format!("✗ exit {}", s.code().unwrap_or(-1))
                };
                println!(
                    "\n⏱ {:.2}s {} (waiting for changes...)\n",
                    elapsed.as_secs_f64(),
                    mark
                );
            }
            Err(e) => eprintln!("\n✗ 启动失败: {}\n", e),
        }
    };

    if run_on_start {
        run_cmd();
    }

    let debounce = Duration::from_millis(debounce_ms);
    let mut last_fire = Instant::now() - debounce;

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(event)) => {
                // 扩展名过滤
                let relevant = if ext_filter.is_empty() {
                    true
                } else {
                    event.paths.iter().any(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| ext_filter.contains(&e.to_lowercase()))
                            .unwrap_or(false)
                    })
                };
                if !relevant {
                    continue;
                }

                // debounce
                if last_fire.elapsed() < debounce {
                    continue;
                }
                last_fire = Instant::now();

                let t = chrono::Local::now().format("%H:%M:%S");
                println!("🔁 [{}] 变化检测, 重跑...", t);
                run_cmd();
            }
            Ok(Err(e)) => eprintln!("watch 错误: {}", e),
            Err(_) => {} // timeout, 继续
        }
    }
}
