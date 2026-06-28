use std::time::Instant;
use std::process::Command;

/// 命令执行计时
pub fn run(cmd: &str) -> anyhow::Result<()> {
    let start = Instant::now();

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let mut command = Command::new(shell);
    if cfg!(windows) {
        command.arg("/C");
    } else {
        command.arg("-c");
    }
    command.arg(cmd);
    let status = command.status()?;

    let elapsed = start.elapsed();
    let secs = elapsed.as_secs();
    let millis = elapsed.subsec_millis();
    let mins = secs / 60;
    let secs_remainder = secs % 60;

    if mins > 0 {
        eprintln!("  time: {}m {}.{:03}s (exit: {})", mins, secs_remainder, millis, status.code().unwrap_or(-1));
    } else {
        eprintln!("  time: {}.{:03}s (exit: {})", secs, millis, status.code().unwrap_or(-1));
    }

    Ok(())
}
