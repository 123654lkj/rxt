use notify::{Event, RecursiveMode, Watcher};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub fn run(
    patterns: &[String],
    cmd: &str,
    path: Option<&Path>,
    debounce_ms: u64,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<()>();
    let pats: Vec<String> = patterns.to_vec();
    let watch_dir = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let watch_dir2 = watch_dir.clone();
    let cmd_owned = cmd.to_string();

    let tx2 = tx.clone();
    thread::spawn(move || {
        let (ev_tx, ev_rx) = mpsc::channel::<()>();

        let mut watcher: notify::RecommendedWatcher =
            match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    for ev_path in &event.paths {
                        let pname: String = ev_path.to_string_lossy().to_string();
                        let matched = pats.is_empty()
                            || pats.iter().any(|p| {
                                if p.starts_with("*.") {
                                    pname.ends_with(&p[1..])
                                } else {
                                    pname.contains(p.as_str())
                                }
                            });
                        if matched {
                            let _ = ev_tx.send(());
                            break;
                        }
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("watch error: {}", e);
                    return;
                }
            };

        if let Err(e) = watcher.watch(&watch_dir2, RecursiveMode::Recursive) {
            eprintln!("watch error: {}", e);
            return;
        }

        let mut last = Instant::now();
        let deb = Duration::from_millis(debounce_ms);
        while ev_rx.recv().is_ok() {
            let now = Instant::now();
            if now.duration_since(last) >= deb {
                let _ = tx2.send(());
                last = now;
            }
        }
    });

    println!(" Watching {} for '{}'", watch_dir.display(), cmd_owned);

    for _ in rx {
        println!("> {}", cmd_owned);
        let start = Instant::now();
        let mut command = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            command.arg("/C");
        } else {
            command.arg("-c");
        }
        command.arg(&cmd_owned);
        match command.spawn() {
            Ok(mut child) => {
                let _ = child.wait();
                let d = start.elapsed();
                println!("  ({}.{:03}s)", d.as_secs(), d.subsec_millis());
            }
            Err(e) => eprintln!("  error: {}", e),
        }
    }
    Ok(())
}
