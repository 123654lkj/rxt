//! 归档解压 — zip / tar / tar.gz / tgz / 3mf
//!
//! 自动识别扩展名,解压到目标目录(默认同名子目录)
//! - --list 只列出内容不解压
//! - --json 输出条目列表(便于 AI 解析)

use std::fs::{self, File};
use std::io::{self, Read, Write, BufReader};
use std::path::{Path, PathBuf};

pub fn run(archive: &Path, target: Option<&Path>, list_only: bool, json_output: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    if !archive.exists() {
        anyhow::bail!("archive not found: {}", archive.display());
    }
    let archive_name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("archive");
    let target_dir = match target {
        Some(t) => t.to_path_buf(),
        None => {
            let stem = archive.file_stem().and_then(|n| n.to_str()).unwrap_or("archive");
            // For .tar.gz, stem is "foo.tar" — strip further
            let stem = if stem.ends_with(".tar") { &stem[..stem.len()-4] } else { stem };
            archive.with_file_name(stem)
        }
    };

    // Detect format by extension
    let lower_name = archive_name.to_lowercase();
    let is_tar_gz = lower_name.ends_with(".tar.gz") || lower_name.ends_with(".tgz");
    let is_tar_xz = lower_name.ends_with(".tar.xz") || lower_name.ends_with(".txz");
    let is_tar = lower_name.ends_with(".tar");
    let is_zip = lower_name.ends_with(".zip") || lower_name.ends_with(".3mf") || lower_name.ends_with(".cbz") || lower_name.ends_with(".epub");
    let is_gz = lower_name.ends_with(".gz") && !is_tar_gz;

    if list_only {
        return list_contents(archive, is_zip, is_tar, is_tar_gz, is_tar_xz, is_gz, json_output, strip_prefix);
    }

    fs::create_dir_all(&target_dir)?;
    println!("Extracting {} -> {}", archive.display(), target_dir.display());

    let count: usize;
    if is_zip {
        count = extract_zip(archive, &target_dir, strip_prefix)?;
    } else if is_tar_gz || is_tar_xz {
        count = extract_tar_gz(archive, &target_dir, is_tar_xz, strip_prefix)?;
    } else if is_tar {
        count = extract_tar(archive, &target_dir, strip_prefix)?;
    } else if is_gz {
        count = extract_gz(archive, &target_dir)?;
    } else {
        anyhow::bail!("unsupported archive format: {} (supported: .zip .tar .tar.gz .tgz .tar.xz .txz .3mf)", archive_name);
    }
    println!("Done: {} files extracted", count);
    Ok(())
}

fn list_contents(archive: &Path, is_zip: bool, is_tar: bool, is_tar_gz: bool, is_tar_xz: bool, is_gz: bool, json_output: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    if is_zip {
        list_zip(archive, json_output, strip_prefix)
    } else if is_tar_gz || is_tar_xz {
        list_tar_gz(archive, is_tar_xz, json_output, strip_prefix)
    } else if is_tar {
        list_tar(archive, json_output, strip_prefix)
    } else if is_gz {
        println!("{} (gzipped binary, single file)", archive.display());
        Ok(())
    } else {
        anyhow::bail!("unsupported archive format");
    }
}

// ─────────────────────────────────────────
// ZIP (.zip, .3mf, .cbz, .epub)
// ─────────────────────────────────────────

fn extract_zip(archive: &Path, target: &Path, strip_prefix: Option<usize>) -> anyhow::Result<usize> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
    let total = zip.len();
    for i in 0..total {
        let mut entry = zip.by_index(i)?;
        let raw_name = entry.name().to_string();
        let out_path = apply_strip(target, &raw_name, strip_prefix)?;
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
            let mut out = File::create(&out_path)?;
            io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(total)
}

fn list_zip(archive: &Path, json: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
    if json {
        let entries: Vec<_> = (0..zip.len()).map(|i| {
            let e = zip.by_index(i).ok();
            match e {
                Some(e) => {
                    let raw = e.name().to_string();
                    let stripped = strip_prefix.map(|n| apply_strip(Path::new(""), &raw, Some(n)).ok().map(|p| p.display().to_string()).unwrap_or(raw.clone())).unwrap_or(raw.clone());
                    serde_json::json!({
                        "name": raw,
                        "stripped": stripped,
                        "size": e.size(),
                        "compressed": e.compressed_size(),
                        "dir": e.is_dir(),
                    })
                }
                None => serde_json::json!({"error": "index out of range"})
            }
        }).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"archive": archive.display().to_string(), "count": zip.len(), "entries": entries}))?);
    } else {
        println!("Archive: {} ({} entries)", archive.display(), zip.len());
        for i in 0..zip.len() {
            let e = zip.by_index(i)?;
            let marker = if e.is_dir() { "/" } else { "" };
            println!("  {:>10} {}{}", e.size(), e.name(), marker);
        }
    }
    Ok(())
}

// ─────────────────────────────────────────
// TAR / TAR.GZ / TAR.XZ
// ─────────────────────────────────────────

fn extract_tar(archive: &Path, target: &Path, strip_prefix: Option<usize>) -> anyhow::Result<usize> {
    let file = File::open(archive)?;
    let mut tar = tar::Archive::new(BufReader::new(file));
    let total = extract_tar_entries(&mut tar, target, strip_prefix)?;
    Ok(total)
}

fn extract_tar_gz(archive: &Path, target: &Path, is_xz: bool, strip_prefix: Option<usize>) -> anyhow::Result<usize> {
    let file = File::open(archive)?;
    let decoder: Box<dyn Read> = if is_xz {
        Box::new(xz2::read::XzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(flate2::read::GzDecoder::new(BufReader::new(file)))
    };
    let mut tar = tar::Archive::new(decoder);
    let total = extract_tar_entries(&mut tar, target, strip_prefix)?;
    Ok(total)
}

fn extract_tar_entries<R: Read>(tar: &mut tar::Archive<R>, target: &Path, strip_prefix: Option<usize>) -> anyhow::Result<usize> {
    let mut count = 0;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.display().to_string();
        let out_path = apply_strip(target, &raw_path, strip_prefix)?;
        if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
        entry.unpack(&out_path)?;
        count += 1;
    }
    Ok(count)
}

fn list_tar(archive: &Path, json: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    let file = File::open(archive)?;
    let mut tar = tar::Archive::new(BufReader::new(file));
    print_tar_entries(&mut tar, archive, json, strip_prefix)
}

fn list_tar_gz(archive: &Path, is_xz: bool, json: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    let file = File::open(archive)?;
    let decoder: Box<dyn Read> = if is_xz {
        Box::new(xz2::read::XzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(flate2::read::GzDecoder::new(BufReader::new(file)))
    };
    let mut tar = tar::Archive::new(decoder);
    print_tar_entries(&mut tar, archive, json, strip_prefix)
}

fn print_tar_entries<R: Read>(tar: &mut tar::Archive<R>, archive: &Path, json: bool, strip_prefix: Option<usize>) -> anyhow::Result<()> {
    let mut entries = Vec::new();
    for entry in tar.entries()? {
        let entry = entry?;
        let raw_path = entry.path()?.display().to_string();
        let size = entry.header().size()?;
        let is_dir = entry.header().entry_type().is_dir();
        entries.push((raw_path, size, is_dir));
    }
    if json {
        let json_entries: Vec<_> = entries.iter().map(|(n, s, d)| {
            let stripped = strip_prefix.map(|p| apply_strip(Path::new(""), n, Some(p)).ok().map(|x| x.display().to_string()).unwrap_or(n.clone())).unwrap_or_else(|| n.clone());
            serde_json::json!({"name": n, "stripped": stripped, "size": s, "dir": d})
        }).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"archive": archive.display().to_string(), "count": entries.len(), "entries": json_entries}))?);
    } else {
        println!("Archive: {} ({} entries)", archive.display(), entries.len());
        for (n, s, d) in &entries {
            let marker = if *d { "/" } else { "" };
            println!("  {:>10} {}{}", s, n, marker);
        }
    }
    Ok(())
}

// ─────────────────────────────────────────
// Plain GZ (single file)
// ─────────────────────────────────────────

fn extract_gz(archive: &Path, target: &Path) -> anyhow::Result<usize> {
    let file = File::open(archive)?;
    let mut decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    let out_name = archive.file_stem().and_then(|n| n.to_str()).unwrap_or("output");
    let out_path = target.join(out_name);
    if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
    let mut out = File::create(&out_path)?;
    io::copy(&mut decoder, &mut out)?;
    Ok(1)
}

fn apply_strip(base: &Path, raw: &str, strip: Option<usize>) -> anyhow::Result<PathBuf> {
    let p = Path::new(raw);
    let stripped = if let Some(n) = strip {
        let mut comps = p.components();
        for _ in 0..n { comps.next(); }
        comps.as_path()
    } else { p };
    if stripped.as_os_str().is_empty() {
        return Ok(base.join(raw));
    }
    // Sanitize: refuse absolute paths or ".."
    let s = stripped.to_string_lossy();
    if s.starts_with('/') || s.contains("..") {
        return Ok(base.join(raw));  // fallback to raw
    }
    Ok(base.join(stripped))
}