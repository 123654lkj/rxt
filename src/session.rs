//! session — 持久 shell 会话管理 v2
//!
//! 本地: 常驻 Child (piped stdin/stdout/stderr), cd/变量天然保持
//! 远程: 每次 exec 用独立 SSH channel, 通过 cwd 跟踪 + 环境文件保持状态

// 当 remote feature 关闭时,提供最小 stub 使代码能通过编译
// RemoteShell 的实际实现在 remote feature 开启时通过 ssh2 crate 提供
#[cfg(not(feature = "remote"))]
mod ssh2 {
    pub struct Session;
    pub struct Channel;
    impl Session {
        pub fn new() -> Result<Self, std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote feature disabled")) }
        pub fn set_tcp_stream(&self, _: impl std::io::Read + std::io::Write) {}
        pub fn set_timeout(&self, _: u32) {}
        pub fn handshake(&self) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote feature disabled")) }
        pub fn userauth_pubkey_file(&self, _: &str, _: Option<&str>, _: &std::path::Path, _: Option<&str>) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn userauth_password(&self, _: &str, _: &str) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn userauth_agent(&self, _: &str) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn authenticated(&self) -> bool { false }
        pub fn channel_session(&self) -> Result<Channel, std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
    }
    impl Channel {
        pub fn exec(&mut self, _: &str) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn read_to_string(&self, _: &mut String) -> Result<usize, std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn wait(&self) -> Result<(), std::io::Error> { Err(std::io::Error::new(std::io::ErrorKind::Other, "remote")) }
        pub fn exit_status(&self) -> Result<ExitStatus, std::io::Error> { Ok(ExitStatus) }
        pub fn close(&self) -> Result<(), std::io::Error> { Ok(()) }
    }
    pub struct ExitStatus;
    impl ExitStatus {
        pub fn code(&self) -> Option<i32> { Some(-1) }
    }
}
#[cfg(not(feature = "remote"))]
mod shellexpand {
    pub fn tilde(s: &str) -> std::borrow::Cow<str> { std::borrow::Cow::Borrowed(s) }
}

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::thread;

/// 全局会话注册表
static SESSIONS: Mutex<Option<HashMap<String, Shell>>> = Mutex::new(None);

/// 流式输出缓冲区
static STREAMS: Mutex<Option<HashMap<String, StreamState>>> = Mutex::new(None);

struct StreamState {
    lines: Vec<String>,
    done: bool,
    exit_code: Option<i32>,
}

pub struct ExecResult {
    pub stdout: String,
    pub exit_code: i32,
}

pub enum Shell {
    Local(LocalShell),
    Remote(RemoteShell),
}

pub struct LocalShell {
    child_stdin: std::process::ChildStdin,
    receiver: mpsc::Receiver<String>,
    child: Child,
    lang: String,
}

/// 远程持久会话: 独立 exec channel + cwd/环境跟踪
pub struct RemoteShell {
    host: String,
    session: ssh2::Session,
    lang: String,
    cwd: String,
}

impl Shell {
    pub fn lang(&self) -> &str {
        match self {
            Shell::Local(s) => &s.lang,
            Shell::Remote(s) => &s.lang,
        }
    }

    pub fn exec(&mut self, code: &str) -> anyhow::Result<ExecResult> {
        match self {
            Shell::Local(s) => s.exec(code),
            Shell::Remote(s) => s.exec(code),
        }
    }

    pub fn close(&mut self) {
        match self {
            Shell::Local(s) => s.close(),
            Shell::Remote(_) => {}
        }
    }
}

impl LocalShell {
    pub fn create(_id: &str, lang: &str) -> anyhow::Result<Self> {
        let (program, args) = resolve_shell(lang)?;
        let mut child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("启动 {} 失败: {} (路径: {})", lang, e, program))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("无法获取 stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("无法获取 stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow::anyhow!("无法获取 stderr"))?;

        let (tx, rx) = mpsc::channel::<String>();
        let tx2 = tx.clone();

        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line { Ok(l) => { if tx.send(l).is_err() { break; } } Err(_) => break }
            }
            let _ = tx.send("\0__RXT_EOF__".to_string());
        });

        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line { Ok(l) => { if tx2.send(format!("[stderr] {}", l)).is_err() { break; } } Err(_) => break }
            }
        });

        Ok(LocalShell { child_stdin: stdin, receiver: rx, child, lang: lang.to_string() })
    }

    pub fn exec(&mut self, code: &str) -> anyhow::Result<ExecResult> {
        let marker = format!("__RXT_END_{}__", uuid_marker());

        // pwsh: Set-Location/cd 路径的 \ 替换为 /
        let safe_code = if self.lang == "pwsh" || self.lang == "ps1" {
            let t = code.trim();
            if t.starts_with("Set-Location ") || t.starts_with("cd ") || t.starts_with("CD ") {
                code.replace("\\", "/")
            } else { code.to_string() }
        } else { code.to_string() };

        let full_input = if self.lang == "pwsh" || self.lang == "ps1" {
            format!("{}\nWrite-Host '{}'\nWrite-Host \"EXITCODE:$LASTEXITCODE\"\n", safe_code, marker)
        } else {
            format!("{}\necho '{}'\necho \"EXITCODE:$?\"\n", safe_code, marker)
        };

        self.child_stdin.write_all(full_input.as_bytes())
            .map_err(|e| anyhow::anyhow!("写入 stdin 失败: {} (会话可能已死)", e))?;
        self.child_stdin.flush()?;

        let mut output_lines = Vec::new();
        let mut exit_code = 0i32;

        while let Ok(line) = self.receiver.recv() {
            if line == "\0__RXT_EOF__" {
                return Ok(ExecResult { stdout: output_lines.join("\n"), exit_code: -1 });
            }
            if line.trim() == marker { break; }
            if let Some(rest) = line.trim().strip_prefix("EXITCODE:") {
                if let Ok(c) = rest.trim().parse::<i32>() { exit_code = c; }
                continue;
            }
            output_lines.push(strip_ansi(&line));
        }

        Ok(ExecResult { stdout: output_lines.join("\n"), exit_code })
    }

    pub fn close(&mut self) {
        let _ = self.child_stdin.write_all(b"exit\n");
        let _ = self.child_stdin.flush();
        let _ = self.child.kill();
    }
}

impl Drop for LocalShell {
    fn drop(&mut self) { let _ = self.child.kill(); }
}

impl RemoteShell {
    pub fn create(host: &str, _lang: &str) -> anyhow::Result<Self> {
        let hosts = crate::hosts::HostsFile::load()?;
        let config = hosts.get_host(host)?.clone();

        let timeout = std::time::Duration::from_secs(10);
        let addr_str = format!("{}:{}", config.host, config.port);
        let addrs: Vec<std::net::SocketAddr> = addr_str.to_socket_addrs()
            .map_err(|e| anyhow::anyhow!("DNS 解析失败 {} : {}", addr_str, e))?
            .collect();
        if addrs.is_empty() { anyhow::bail!("DNS 解析无结果: {}", addr_str); }

        let mut last_err = None;
        let mut tcp = None;
        for addr in &addrs {
            match TcpStream::connect_timeout(addr, timeout) {
                Ok(s) => { tcp = Some(s); break; }
                Err(e) => { last_err = Some(e); }
            }
        }
        let tcp = tcp.ok_or_else(|| {
            anyhow::anyhow!("TCP 连接失败 {} ({}s): {:?}", addr_str, timeout.as_secs(), last_err)
        })?;

        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp);
        session.set_timeout(30_000);
        session.handshake()
            .map_err(|e| anyhow::anyhow!("SSH 握手失败: {}", e))?;

        if let Some(key_path) = &config.key {
            let key = shellexpand::tilde(key_path).into_owned();
            session.userauth_pubkey_file(&config.user, None, std::path::Path::new(&key), None)?;
        } else if let Some(password) = hosts.get_password(&config) {
            session.userauth_password(&config.user, &password)?;
        } else {
            session.userauth_agent(&config.user)?;
        }

        if !session.authenticated() {
            anyhow::bail!("认证失败 {}@{}", config.user, config.host);
        }

        let cwd = ssh_exec(&session, "pwd")?.trim().to_string();

        Ok(RemoteShell {
            host: host.to_string(),
            session,
            lang: "sh".to_string(),
            cwd,
        })
    }

    pub fn exec(&mut self, code: &str) -> anyhow::Result<ExecResult> {
        let marker = format!("__RXT_END_{}__", uuid_marker());

        // 构造命令: cd 到保存的目录 → 执行代码 → 保存新 cwd → marker
        let wrapped = format!(
            "cd '{}' 2>/dev/null; {}; __rxt_ec=$?; pwd > /tmp/.rxt_cwd; echo '{}'; echo \"EXITCODE:$__rxt_ec\"",
            self.cwd, code, marker
        );

        let output = ssh_exec(&self.session, &wrapped)?;

        // 解析输出
        let mut output_lines = Vec::new();
        let mut exit_code = 0i32;

        for line in output.lines() {
            let line = line.trim_end_matches('\r');
            if line == marker { continue; }
            if let Some(rest) = line.strip_prefix("EXITCODE:") {
                if let Ok(c) = rest.trim().parse::<i32>() { exit_code = c; }
                continue;
            }
            output_lines.push(strip_ansi(line));
        }

        // 更新 cwd
        if let Ok(new_cwd) = ssh_exec(&self.session, "cat /tmp/.rxt_cwd 2>/dev/null") {
            let trimmed = new_cwd.trim();
            if !trimmed.is_empty() {
                self.cwd = trimmed.to_string();
            }
        }

        Ok(ExecResult {
            stdout: output_lines.join("\n").trim().to_string(),
            exit_code,
        })
    }
}

/// SSH 简单执行: 返回 stdout
fn ssh_exec(session: &ssh2::Session, cmd: &str) -> anyhow::Result<String> {
    let mut channel = session.channel_session()
        .map_err(|e| anyhow::anyhow!("SSH channel 失败: {}", e))?;
    channel.exec(cmd)
        .map_err(|e| anyhow::anyhow!("SSH exec 失败: {}", e))?;
    let mut output = String::new();
    channel.read_to_string(&mut output)?;
    let _ = channel.exit_status();
    Ok(output)
}

fn resolve_shell(lang: &str) -> anyhow::Result<(String, Vec<String>)> {
    match lang {
        "pwsh" | "ps1" | "powershell" => {
            let pwsh = find_pwsh()?;
            Ok((pwsh, vec!["-NoProfile".into(), "-NoLogo".into(), "-Command".into(), "-".into()]))
        }
        "sh" | "bash" => Ok(("sh".into(), vec![])),
        "python" | "py" => Ok(("python3".into(), vec!["-i".into()])),
        _ => anyhow::bail!("不支持的会话语言: {} (支持: pwsh, sh, py)", lang),
    }
}

fn find_pwsh() -> anyhow::Result<String> {
    if cfg!(windows) {
        // 系统自带 5.1 优先，避免 PATH 里 PS7 坏 shim / 未装 pwsh
        for cmd in &[
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            "pwsh",
            "pwsh.exe",
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            "powershell",
            "powershell.exe",
        ] {
            if std::process::Command::new(cmd).arg("-NoProfile").arg("-Command").arg("1")
                .stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
            { return Ok(cmd.to_string()); }
        }
        anyhow::bail!("未找到可用的 PowerShell（系统 5.1 或 PowerShell 7）")
    } else {
        Ok("pwsh".to_string())
    }
}

fn uuid_marker() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:08x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos())
}

/// 过滤 ANSI 转义码
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(c2) = chars.next() {
                    if c2.is_ascii_alphabetic() { break; }
                }
            } else {
                chars.next();
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ===== SessionManager API =====

pub fn init() {
    let mut g = SESSIONS.lock().unwrap();
    if g.is_none() { *g = Some(HashMap::new()); }
    let mut sg = STREAMS.lock().unwrap();
    if sg.is_none() { *sg = Some(HashMap::new()); }
}

pub fn create_session(id: &str, lang: &str, host: Option<&str>) -> anyhow::Result<()> {
    let mut g = SESSIONS.lock().unwrap();
    let sessions = g.get_or_insert_with(HashMap::new);
    if sessions.contains_key(id) {
        anyhow::bail!("会话已存在: {} (先 close 再创建)", id);
    }
    let shell = if let Some(h) = host {
        if h == "local" {
            Shell::Local(LocalShell::create(id, lang)?)
        } else {
            Shell::Remote(RemoteShell::create(h, lang)?)
        }
    } else {
        Shell::Local(LocalShell::create(id, lang)?)
    };
    sessions.insert(id.to_string(), shell);
    Ok(())
}

pub fn exec_session(id: &str, code: &str) -> anyhow::Result<ExecResult> {
    let mut g = SESSIONS.lock().unwrap();
    let sessions = g.get_or_insert_with(HashMap::new);
    let shell = sessions.get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("会话不存在: {}", id))?;
    shell.exec(code)
}

pub fn exec_stream(id: &str, code: &str) -> anyhow::Result<String> {
    let result = exec_session(id, code)?;
    let stream_id = format!("s_{}", uuid_marker());
    let mut g = STREAMS.lock().unwrap();
    let streams = g.get_or_insert_with(HashMap::new);
    streams.insert(stream_id.clone(), StreamState {
        lines: result.stdout.lines().map(|s| s.to_string()).collect(),
        done: true,
        exit_code: Some(result.exit_code),
    });
    Ok(stream_id)
}

pub fn poll_stream(stream_id: &str) -> anyhow::Result<serde_json::Value> {
    let mut g = STREAMS.lock().unwrap();
    let streams = g.get_or_insert_with(HashMap::new);
    let state = streams.get_mut(stream_id)
        .ok_or_else(|| anyhow::anyhow!("stream 不存在: {}", stream_id))?;
    let new_lines: Vec<String> = state.lines.clone();
    state.lines.clear();
    let done = state.done;
    let exit_code = state.exit_code;
    if done { streams.remove(stream_id); }
    Ok(serde_json::json!({
        "stream_id": stream_id,
        "lines": new_lines,
        "done": done,
        "exit_code": exit_code,
    }))
}

pub fn close_session(id: &str) -> anyhow::Result<()> {
    let mut g = SESSIONS.lock().unwrap();
    let sessions = g.get_or_insert_with(HashMap::new);
    if let Some(mut shell) = sessions.remove(id) {
        shell.close();
        Ok(())
    } else {
        anyhow::bail!("会话不存在: {}", id)
    }
}

pub fn list_sessions() -> Vec<serde_json::Value> {
    let g = SESSIONS.lock().unwrap();
    match g.as_ref() {
        Some(s) => s.iter().map(|(id, shell)| {
            let st = match shell {
                Shell::Local(_) => "local",
                Shell::Remote(r) => &r.host,
            };
            serde_json::json!({"id": id, "lang": shell.lang(), "type": st})
        }).collect(),
        None => vec![],
    }
}
