//! notify — 跨平台桌面通知
//!
//! 解决: 长任务(编译/部署/训练)跑完弹通知,不再傻等盯着屏幕
//! 实现: 各平台原生通知机制
//!   - Windows: Toast 通知(msg.exe / PowerShell BurntToast / mshta)
//!   - Linux: notify-send
//!   - macOS: osascript
//!
//! 用法:
//!   rxt notify "编译完成"
//!   rxt notify "部署成功" --title "生产环境"
//!   rxt notify "测试失败" --level error
//!   make test; rxt notify "完成"   # 配合长命令

use std::process::Command;

pub fn run(message: &str, title: Option<&str>, level: &str) -> anyhow::Result<()> {
    let title = title.unwrap_or("rxt");
    let icon = match level {
        "error" | "err" => "❌",
        "warn" | "warning" => "⚠️",
        "success" | "ok" => "✅",
        _ => "ℹ️",
    };
    let full = format!("{} {}", icon, message);

    #[cfg(target_os = "windows")]
    {
        // 优先 PowerShell Toast(无依赖,Win10+ 自带)
        let ps = format!(
            "[reflection.assembly]::loadwithpartialname('System.Windows.Forms') | Out-Null; \
             $balloon = New-Object System.Windows.Forms.NotifyIcon; \
             $balloon.Icon = [System.Drawing.SystemIcons]::Information; \
             $balloon.BalloonTipTitle = '{}'; \
             $balloon.BalloonTipText = '{}'; \
             $balloon.Visible = $true; \
             $balloon.ShowBalloonTip(5000)",
            title.replace('\'', "''"), message.replace('\'', "''")
        );
        let _ = Command::new("powershell").args(["-NoProfile", "-Command", &ps])
            .spawn().map(|_| ());
        // 兜底: msg.exe(通知级别低但肯定有)
        // 不报错,因为通知失败不应中断主流程
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let urgency = match level { "error"|"err" => "critical", "warn"|"warning" => "normal", _ => "low" };
        if which("notify-send") {
            let _ = Command::new("notify-send").args(["-u", urgency, "-a", "rxt", title, message]).spawn();
        } else {
            // 兜底: 写到终端 bell
            eprint!("\x07");
        }
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!("display notification \"{}\" with title \"{}\"",
            message.replace('"', "\\\""), title.replace('"', "\\\""));
        let _ = Command::new("osascript").args(["-e", &script]).spawn();
    }

    println!("🔔 {} | {}", title, full);
    Ok(())
}

fn which(cmd: &str) -> bool {
    Command::new(if cfg!(windows) {"where"} else {"which"}).arg(cmd)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false)
}
