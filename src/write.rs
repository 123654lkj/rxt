//! 智能文件写入 — 格式保持 + 跨平台兼容
//! 默认保持目标文件原有格式，新文件用平台默认

use std::path::Path;
use std::fs;
use std::io::{self, Read, Write};

use crate::signature::{FileSignature, apply_format};

/// 写入内容到文件（自动保持格式）
pub fn run(path: &Path, content: Option<&str>, append: bool, preserve_format: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    let data = match content {
        Some(s) => {
            if preserve_format && path.exists() && !append {
                // 读取原始文件指纹并应用格式
                let raw = fs::read(path)?;
                let sig = FileSignature::detect(&raw);
                let formatted = apply_format(s, &sig);
                formatted.into_bytes()
            } else {
                // 新文件或追加模式：直接写字节
                s.as_bytes().to_vec()
            }
        }
        None => {
            // Read from stdin
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

    if let Some(remote) = remote {
        remote.write_file(path, &data)?;
    } else {
        file.write_all(&data)?;
    }
    eprintln!("  wrote {} bytes -> {}", data.len(), path.display());
    Ok(())
}

/// Read content from a source file and write to the target path
pub fn run_file(path: &Path, source: &Path, append: bool) -> anyhow::Result<()> {
    let data = fs::read(source)?;
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
pub fn run_b64(path: &Path, b64_content: &str, append: bool) -> anyhow::Result<()> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(b64_content.trim())
        .map_err(|e| anyhow::anyhow!("base64 decode error: {}", e))?;

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
