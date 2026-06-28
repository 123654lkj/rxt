//! serve — 极简 HTTP 文件服务器(手机扫码秒访问)
//!
//! 纯 std::net 手写,无新依赖。自动探测局域网 IP + 调 qr 命令显示二维码。
//! 跨平台(Win/Linux/Mac)。
//!
//! 用法:
//!   rxt serve                       # 当前目录, 端口 8000
//!   rxt serve --port 9000
//!   rxt serve /path/to/dir
//!   rxt serve --no-qr               # 不显示二维码

use std::path::{Path, PathBuf};
use std::net::{TcpListener, TcpStream, IpAddr};
use std::io::{Read, Write, BufRead, BufReader};
use std::fs;

pub fn run(dir: Option<&str>, port: u16, no_qr: bool) -> anyhow::Result<()> {
    let root = dir.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let root = root.canonicalize().unwrap_or(root);
    if !root.is_dir() {
        anyhow::bail!("{} 不是目录", root.display());
    }

    // 绑定 0.0.0.0:port
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;

    // 探测局域网 IP
    let lan_ip = detect_lan_ip().unwrap_or_else(|| "127.0.0.1".parse().unwrap());
    let url = format!("http://{}:{}", lan_ip, port);

    println!("🚀 rxt serve — 文件服务器已启动");
    println!("   目录: {}", root.display());
    println!("   本机: http://127.0.0.1:{}", port);
    println!("   局域网: {}", url);
    println!("   (Ctrl+C 停止)\n");

    if !no_qr {
        // 显示二维码(调 qr 模块)
        let _ = crate::qr::run(&url, false, true);
        println!();
    }

    println!("📝 访问日志:");
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        if let Err(e) = handle(&stream, &root) {
            eprintln!("  [{}] 处理失败: {}", peer, e);
        }
    }
    Ok(())
}

fn handle(stream: &TcpStream, root: &Path) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.by_ref().read_line(&mut request_line)?;
    // 跳过 headers
    loop {
        let mut h = String::new();
        reader.by_ref().read_line(&mut h)?;
        if h.trim().is_empty() { break; }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let (method, raw_path) = (parts.first().copied().unwrap_or("GET"), parts.get(1).copied().unwrap_or("/"));
    let decoded = urlencoding::decode(raw_path.split('?').next().unwrap_or("/"))
        .map(|s| s.to_string()).unwrap_or_else(|_| raw_path.to_string());
    let decoded = decoded.strip_prefix('/').unwrap_or(&decoded);
    let decoded = decoded.trim_start_matches('/');

    let fs_path = root.join(decoded);

    // 安全: 防目录穿越
    let safe = fs_path.canonicalize().unwrap_or_else(|_| fs_path.clone());
    if !safe.starts_with(root) {
        return respond(stream, 403, "Forbidden", "403 Forbidden", "text/plain");
    }

    let peer = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();

    if safe.is_dir() {
        // 目录列表
        let entries = fs::read_dir(&safe)?;
        let mut html = format!("<!DOCTYPE html><html><head><meta charset=utf-8><title>{}</title><style>body{{font-family:system-ui;max-width:900px;margin:2rem auto;padding:0 1rem}}h1{{font-size:1.3rem}}a{{display:block;padding:.4rem .2rem;text-decoration:none;color:#0366d6;border-bottom:1px solid #eee}}a:hover{{background:#f6f8fa}}a.dir{{font-weight:600}}.meta{{color:#888;font-size:.85rem}}</style></head><body><h1>📁 /{}</h1>",
            safe.display(), decoded);
        // 返回上级
        if !decoded.is_empty() {
            html.push_str("<a class=dir href=\"../\">📁 ../</a>");
        }
        let mut dirs: Vec<_> = Vec::new();
        let mut files: Vec<_> = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                dirs.push(name);
            } else if !name.starts_with('.') {
                files.push((name, e.metadata().ok()));
            }
        }
        dirs.sort(); files.sort_by(|a,b| a.0.cmp(&b.0));
        let entry_count = dirs.len() + files.len();
        for d in dirs {
            html.push_str(&format!("<a class=dir href=\"{}/\">📁 {}/</a>", html_escape(&d), html_escape(&d)));
        }
        for (name, meta) in files {
            let size = meta.as_ref().map(|m| fmt_size(m.len())).unwrap_or_default();
            html.push_str(&format!("<a href=\"{}\">📄 {} <span class=meta>{}</span></a>", html_escape(&name), html_escape(&name), size));
        }
        html.push_str("</body></html>");
        println!("  [{}] GET /{} -> 目录 ({} 项)", peer, decoded, entry_count);
        return respond(stream, 200, "OK", &html, "text/html; charset=utf-8");
    }

    if safe.is_file() {
        let data = fs::read(&safe)?;
        let mime = guess_mime(&safe);
        println!("  [{}] GET /{} -> {} ({} {})", peer, decoded, safe.display(), fmt_size(data.len() as u64), mime);
        // 大文件流式更佳,这里简单一次性返回
        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", mime, data.len());
        stream.try_clone()?.write_all(response.as_bytes())?;
        stream.try_clone()?.write_all(&data)?;
        return Ok(());
    }

    println!("  [{}] GET /{} -> 404", peer, decoded);
    respond(stream, 404, "Not Found", "404 Not Found", "text/plain")
}

fn respond(stream: &TcpStream, code: u16, status: &str, body: &str, mime: &str) -> anyhow::Result<()> {
    let resp = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        code, status, mime, body.len(), body);
    stream.try_clone()?.write_all(resp.as_bytes())?;
    Ok(())
}

fn detect_lan_ip() -> Option<IpAddr> {
    // 用 UDP connect 探测本机出口 IP(不真发包)
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("mp4") => "video/mp4",
        Some("mp3") => "audio/mpeg",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn fmt_size(b: u64) -> String {
    const MB: u64 = 1024*1024; const KB: u64 = 1024; const GB: u64 = MB*1024;
    if b >= GB { format!("{:.1}G", b as f64/GB as f64) }
    else if b >= MB { format!("{:.1}M", b as f64/MB as f64) }
    else if b >= KB { format!("{:.0}K", b as f64/KB as f64) }
    else { format!("{}B", b) }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
