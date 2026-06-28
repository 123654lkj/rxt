//! clip — 跨平台系统剪贴板读写
//!
//! 解决: AI 长输出直接进剪贴板 / 用户复制内容给 AI 读取
//! 实现: 调用各平台原生剪贴板工具(无新依赖)
//!   - Windows: PowerShell Get-SetClipboard
//!   - Linux: xclip / xsel / wl-copy (Wayland)
//!   - macOS: pbcopy / pbpaste
//!
//! 用法:
//!   rxt clip read                    # 读剪贴板到 stdout
//!   rxt clip write "内容"             # 写入剪贴板
//!   rxt clip write --file data.txt   # 文件内容写入剪贴板
//!   rxt clip clear                   # 清空
//!   echo "hi" | rxt clip write       # 管道写入

use std::process::{Command, Stdio};
use std::io::{Read, Write};

pub fn run(action: &str, content: Option<&str>, file: Option<&str>) -> anyhow::Result<()> {
    match action {
        "read" | "get" => read(),
        "write" | "set" => {
            let data = if let Some(f) = file {
                std::fs::read_to_string(f)?
            } else if let Some(c) = content {
                c.to_string()
            } else {
                // 从 stdin 读
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };
            write(&data)
        }
        "clear" | "clr" => write(""),
        other => anyhow::bail!("未知操作 '{}',可选: read/write/clear", other),
    }
}

fn read() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // PowerShell: Get-Clipboard 返回字符串,转 UTF-8 字节写 stdout
        let ps = "$c=Get-Clipboard -Raw; if($c){$b=[System.Text.Encoding]::UTF8.GetBytes($c); [Console]::OpenStandardOutput().Write($b,0,$b.Length)}";
        let out = Command::new("powershell").args(["-NoProfile", "-Command", ps]).output()
            .map_err(|e| anyhow::anyhow!("剪贴板读取失败: {}", e))?;
        if !out.status.success() {
            anyhow::bail!("剪贴板读取失败: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        // PowerShell 在 UTF-8 输出开头会加 BOM, 去掉
        let mut bytes = out.stdout;
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            bytes.drain(..3);
        }
        std::io::stdout().write_all(&bytes)?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let (cmd, args) = read_cmd()?;
        let out = Command::new(cmd).args(&args).output()
            .map_err(|e| anyhow::anyhow!("剪贴板读取失败({} 未安装?): {}", cmd, e))?;
        if !out.status.success() {
            anyhow::bail!("剪贴板读取失败: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        std::io::stdout().write_all(&out.stdout)?;
        Ok(())
    }
}

fn write(data: &str) -> anyhow::Result<()> {
    let len = data.chars().count();
    #[cfg(target_os = "windows")]
    {
        // 写临时 UTF-8 文件, PowerShell Get-Content -Encoding UTF8 读后 SetClipboard
        // 这是绕开 stdin 管道编码地狱最可靠的方式
        let tmp = std::env::temp_dir().join(format!("rxt_clip_{}.txt", std::process::id()));
        std::fs::write(&tmp, data)?;
        let ps = format!("$c=Get-Content -Path '{}' -Raw -Encoding UTF8; Set-Clipboard -Value $c", tmp.display());
        let out = Command::new("powershell").args(["-NoProfile", "-Command", &ps]).output()
            .map_err(|e| anyhow::anyhow!("剪贴板写入失败: {}", e))?;
        let _ = std::fs::remove_file(&tmp);
        if !out.status.success() {
            anyhow::bail!("剪贴板写入失败: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        println!("✓ 已写入剪贴板 ({} 字符)", len);
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = len;
        let (cmd, args) = write_cmd()?;
        let mut child = Command::new(cmd).args(&args)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().map_err(|e| anyhow::anyhow!("剪贴板写入失败({} 未安装?): {}", cmd, e))?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(data.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("剪贴板写入失败");
        }
        println!("✓ 已写入剪贴板 ({} 字符)", data.chars().count());
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_cmd() -> anyhow::Result<(&'static str, Vec<&'static str>)> {
    // X11: xclip; Wayland: wl-paste
    if std::env::var("WAYLAND_DISPLAY").is_ok() && which("wl-paste") {
        Ok(("wl-paste", vec![]))
    } else if which("xclip") {
        Ok(("xclip", vec!["-selection", "clipboard", "-o"]))
    } else if which("xsel") {
        Ok(("xsel", vec!["--clipboard", "--output"]))
    } else {
        anyhow::bail!("需要 xclip / xsel / wl-paste (任一)")
    }
}
#[cfg(all(unix, not(target_os = "macos")))]
fn write_cmd() -> anyhow::Result<(&'static str, Vec<&'static str>)> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() && which("wl-copy") {
        Ok(("wl-copy", vec![]))
    } else if which("xclip") {
        Ok(("xclip", vec!["-selection", "clipboard"]))
    } else if which("xsel") {
        Ok(("xsel", vec!["--clipboard", "--input"]))
    } else {
        anyhow::bail!("需要 xclip / xsel / wl-copy (任一)")
    }
}

#[cfg(target_os = "macos")]
fn read_cmd() -> anyhow::Result<(&'static str, Vec<&'static str>)> { Ok(("pbpaste", vec![])) }
#[cfg(target_os = "macos")]
fn write_cmd() -> anyhow::Result<(&'static str, Vec<&'static str>)> { Ok(("pbcopy", vec![])) }

/// 检查命令是否存在
fn which(cmd: &str) -> bool {
    Command::new(if cfg!(windows) {"where"} else {"which"})
        .arg(cmd).stdout(Stdio::null()).stderr(Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false)
}
