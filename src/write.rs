//! 智能文件写入 — 格式保持 + 跨平台兼容
//! 默认保持目标文件原有格式，新文件用平台默认

use std::path::Path;
use std::fs;
use std::io::{self, Read, Write};

use crate::signature::{FileSignature, apply_format};

/// 写入内容到文件（自动保持格式）
/// v0.4.3: 远程时不碰本地 fs (之前无论是否 remote 都先 fs::create_dir_all + 本地 create, 导致远程路径出错)
pub fn run(path: &Path, content: Option<&str>, append: bool, preserve_format: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    // 远程: 直接走 remote.write_file (格式保持对远程文件意义不大, 且 read 远程代价高, 简化处理)
    if let Some(remote_channel) = remote {
        let data = match content {
            Some(s) => s.as_bytes().to_vec(),
            None => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                buf.into_bytes()
            }
        };
        if append {
            let existing = remote_channel.read_file(path).unwrap_or_default();
            let mut combined = Vec::with_capacity(existing.len() + data.len());
            combined.extend_from_slice(&existing);
            combined.extend_from_slice(&data);
            remote_channel.write_file(path, &combined)?;
        } else {
            remote_channel.write_file(path, &data)?;
        }
        eprintln!("  wrote {} bytes (remote) -> {}", data.len(), path.display());
        return Ok(());
    }

    // 本地: 原有逻辑 (含格式保持)
    let data = match content {
        Some(s) => {
            if preserve_format && path.exists() && !append {
                let raw = fs::read(path)?;
                let sig = FileSignature::detect(&raw);
                let formatted = apply_format(s, &sig);
                formatted.into_bytes()
            } else {
                s.as_bytes().to_vec()
            }
        }
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            if preserve_format && path.exists() && !append {
                let raw = fs::read(path)?;
                let sig = FileSignature::detect(&raw);
                let formatted = apply_format(&buf, &sig);
                formatted.into_bytes()
            } else {
                buf.into_bytes()
            }
        }
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = if append {
        fs::OpenOptions::new().append(true).create(true).open(path)?
    } else {
        fs::File::create(path)?
    };
    file.write_all(&data)?;
    eprintln!("  wrote {} bytes -> {}", data.len(), path.display());
    Ok(())
}

/// Read content from a source file and write to the target path
/// v0.4.3: 支持 remote —— 有 remote_channel 时走远程写 (修复 write --host --file 写本地的 bug)
pub fn run_file(path: &Path, source: &Path, append: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    let data = fs::read(source)?;
    if let Some(remote_channel) = remote {
        return write_remote(path, &data, append, remote_channel);
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let mut file = if append {
        fs::OpenOptions::new().append(true).create(true).open(path)?
    } else {
        fs::File::create(path)?
    };

    file.write_all(&data)?;
    eprintln!("  wrote {} bytes (from {}) -> {}", data.len(), source.display(), path.display());
    Ok(())
}

/// Decode base64 content and write to the target path
/// v0.4.3: 支持 remote —— 有 remote_channel 时走远程写 (修复 write --host --b64 写本地的 bug)
pub fn run_b64(path: &Path, b64_content: &str, append: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(b64_content.trim())
        .map_err(|e| anyhow::anyhow!("base64 decode error: {}", e))?;

    if let Some(remote_channel) = remote {
        return write_remote(path, &data, append, remote_channel);
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let mut file = if append {
        fs::OpenOptions::new().append(true).create(true).open(path)?
    } else {
        fs::File::create(path)?
    };

    file.write_all(&data)?;
    eprintln!("  wrote {} bytes (base64 decoded) -> {}", data.len(), path.display());
    Ok(())
}

/// 远程写入 (append 时先读旧内容合并)
fn write_remote(path: &Path, data: &[u8], append: bool, remote: &crate::remote::RemoteChannel) -> anyhow::Result<()> {
    if append {
        let existing = remote.read_file(path).unwrap_or_default();
        let mut combined = Vec::with_capacity(existing.len() + data.len());
        combined.extend_from_slice(&existing);
        combined.extend_from_slice(data);
        remote.write_file(path, &combined)?;
    } else {
        remote.write_file(path, data)?;
    }
    eprintln!("  wrote {} bytes (remote) -> {}", data.len(), path.display());
    Ok(())
}
/// 写入原始字节(不经过 base64)
pub fn run_bytes(path: &Path, data: &[u8], append: bool, preserve_format: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    if let Some(remote_channel) = remote {
        return run_bytes_remote(path, data, append, remote_channel);
    }
    run_bytes_local(path, data, append, preserve_format)
}

fn run_bytes_local(path: &Path, data: &[u8], append: bool, preserve_format: bool) -> anyhow::Result<()> {
    let final_data = if preserve_format && path.exists() && !append {
        let raw = fs::read(path)?;
        let sig = FileSignature::detect(&raw);
        let text_str = String::from_utf8_lossy(data);
        apply_format(&text_str, &sig).into_bytes()
    } else {
        data.to_vec()
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = if append {
        fs::OpenOptions::new().append(true).create(true).open(path)?
    } else {
        fs::File::create(path)?
    };
    file.write_all(&final_data)?;
    eprintln!("  wrote {} bytes -> {}", final_data.len(), path.display());
    Ok(())
}

fn run_bytes_remote(path: &Path, data: &[u8], append: bool, remote: &crate::remote::RemoteChannel) -> anyhow::Result<()> {
    if append {
        let existing = remote.read_file(path).unwrap_or_default();
        let mut combined = Vec::with_capacity(existing.len() + data.len());
        combined.extend_from_slice(&existing);
        combined.extend_from_slice(data);
        remote.write_file(&path, &combined)?;
    } else {
        remote.write_file(&path, data)?;
    }
    eprintln!("  wrote {} bytes (remote) -> {}", data.len(), path.display());
    Ok(())
}
