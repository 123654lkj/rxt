//! channel_update — 从自建更新频道拉预编译二进制
//!
//! 频道根目录需提供:
//!   manifest.json
//!   rxt-x86_64-unknown-linux-gnu
//!   rxt-x86_64-pc-windows-msvc.exe  (可选)
//!
//! 用法:
//!   rxt update              # 检查并安装
//!   rxt update --check      # 只检查
//!   rxt update --force      # 忽略节流强制检查
//!
//! 环境变量:
//!   RXT_UPDATE_URL       频道根 URL（必填，否则 update 报错、启动自动检查跳过）
//!   RXT_UPDATE_AUTO=0    关闭启动自动检查
//!   RXT_UPDATE_CHECK_HOURS  自动检查间隔小时（默认 6）
//!
//! 例: 写入 ~/.rxt/env
//!   RXT_UPDATE_URL=http://192.168.1.10:26780

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, serde::Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: std::collections::HashMap<String, Asset>,
}

#[derive(Debug, serde::Deserialize)]
struct Asset {
    file: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
}

fn update_url_opt() -> Option<String> {
    std::env::var("RXT_UPDATE_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
}

fn update_url() -> anyhow::Result<String> {
    update_url_opt().ok_or_else(|| {
        anyhow::anyhow!(
            "未设置 RXT_UPDATE_URL。\n\
             例: export RXT_UPDATE_URL=http://your-server:26780\n\
             或写入 ~/.rxt/env 后重试"
        )
    })
}

fn platform_key() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

fn stamp_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".rxt").join("update-check.stamp"))
}

fn hours_since_stamp() -> f64 {
    let Some(p) = stamp_path() else {
        return 9999.0;
    };
    let Ok(meta) = fs::metadata(&p) else {
        return 9999.0;
    };
    let Ok(mtime) = meta.modified() else {
        return 9999.0;
    };
    let Ok(elapsed) = SystemTime::now().duration_since(mtime) else {
        return 9999.0;
    };
    elapsed.as_secs_f64() / 3600.0
}

fn touch_stamp() {
    if let Some(p) = stamp_path() {
        let _ = fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = fs::write(&p, format!("{}\n", ts));
    }
}

fn fetch_url(url: &str) -> anyhow::Result<Vec<u8>> {
    #[cfg(feature = "net")]
    {
        let resp = ureq::get(url)
            .call()
            .map_err(|e| anyhow::anyhow!("GET {}: {}", url, e))?;
        let mut buf = Vec::new();
        resp.into_parts().1.into_reader().read_to_end(&mut buf)?;
        return Ok(buf);
    }
    #[cfg(not(feature = "net"))]
    {
        let _ = url;
        anyhow::bail!("channel update 需要 net feature（ureq）")
    }
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn is_newer(remote: &str, local: &str) -> bool {
    let r = parse_version(remote);
    let l = parse_version(local);
    for i in 0..r.len().max(l.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv != lv {
            return rv > lv;
        }
    }
    false
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn fetch_manifest() -> anyhow::Result<Manifest> {
    let url = format!("{}/manifest.json", update_url()?);
    let bytes = fetch_url(&url)?;
    let text = String::from_utf8_lossy(&bytes);
    let m: Manifest = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!(
            "manifest 解析失败: {} body={}",
            e,
            text.chars().take(200).collect::<String>()
        )
    })?;
    Ok(m)
}

/// 启动钩子：节流后检查；有更新则安装
pub fn auto_check_on_start() {
    if std::env::var("RXT_UPDATE_AUTO")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
    {
        return;
    }
    // 未配置频道则静默跳过（开源默认不绑任何内网地址）
    if update_url_opt().is_none() {
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("update") {
        return;
    }
    let hours: f64 = std::env::var("RXT_UPDATE_CHECK_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6.0);
    if hours_since_stamp() < hours {
        return;
    }
    // 静默检查，仅在真正更新时 eprintln
    match check_silent() {
        Ok(UpdateResult::UpToDate { .. }) => touch_stamp(),
        Ok(UpdateResult::Available { local, remote }) => {
            match install_from_channel() {
                Ok(_) => {
                    eprintln!("✨ rxt 已自动更新 {} → {}（下次命令用新二进制）", local, remote);
                    touch_stamp();
                }
                Err(e) => eprintln!("⚠ rxt 自动更新失败: {}（可手动 rxt update）", e),
            }
        }
        Ok(UpdateResult::Updated { .. }) => touch_stamp(),
        Err(_) => {}
    }
}

fn check_silent() -> anyhow::Result<UpdateResult> {
    let local = env!("CARGO_PKG_VERSION").to_string();
    let manifest = fetch_manifest()?;
    let remote = manifest.version.clone();
    let key = platform_key();
    if key == "unknown" {
        anyhow::bail!("unsupported");
    }
    if manifest.assets.get(key).is_none() {
        anyhow::bail!("no asset");
    }
    if !is_newer(&remote, &local) {
        return Ok(UpdateResult::UpToDate { local, remote });
    }
    Ok(UpdateResult::Available { local, remote })
}

fn install_from_channel() -> anyhow::Result<()> {
    let _ = run(false, false)?;
    Ok(())
}

#[derive(Debug)]
pub enum UpdateResult {
    UpToDate { local: String, remote: String },
    Available { local: String, remote: String },
    Updated { local: String, remote: String },
}

pub fn run(check_only: bool, _force: bool) -> anyhow::Result<UpdateResult> {
    let local = env!("CARGO_PKG_VERSION").to_string();
    let manifest = fetch_manifest()?;
    let remote = manifest.version.clone();
    let key = platform_key();
    if key == "unknown" {
        anyhow::bail!("当前平台不支持频道更新");
    }
    let asset = manifest.assets.get(key).ok_or_else(|| {
        anyhow::anyhow!(
            "manifest 无平台 {}: 已有 {:?}",
            key,
            manifest.assets.keys().collect::<Vec<_>>()
        )
    })?;

    if !is_newer(&remote, &local) {
        println!(
            "✓ 已是最新 rxt {} (频道 {} 远程 {})",
            local,
            update_url()?,
            remote
        );
        return Ok(UpdateResult::UpToDate { local, remote });
    }

    println!("✨ 发现新版本: 本地 {} → 远程 {}", local, remote);
    if let Some(t) = &manifest.published_at {
        println!("   发布时间: {}", t);
    }
    println!("   平台: {}  文件: {}", key, asset.file);

    if check_only {
        println!("(仅检查，运行 rxt update 安装)");
        return Ok(UpdateResult::Available { local, remote });
    }

    let url = format!("{}/{}", update_url()?, asset.file.trim_start_matches('/'));
    println!("⬇  下载 {} ...", url);
    let bytes = fetch_url(&url)?;
    if let Some(sz) = asset.size {
        if bytes.len() as u64 != sz {
            anyhow::bail!("大小不符: 期望 {} 实际 {}", sz, bytes.len());
        }
    }
    let dig = sha256_hex(&bytes);
    if !dig.eq_ignore_ascii_case(asset.sha256.trim()) {
        anyhow::bail!("sha256 不符: 期望 {} 实际 {}", asset.sha256, dig);
    }
    println!("✓ 校验通过 ({:.1} MB)", bytes.len() as f64 / 1_048_576.0);

    let cur = std::env::current_exe()?;
    install_binary(&cur, &bytes)?;

    let ver = Command::new(&cur)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("🎉 已更新: {}", ver.trim());
    touch_stamp();
    Ok(UpdateResult::Updated { local, remote })
}

fn install_binary(cur: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = cur.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(
        "{}.new",
        cur.file_name().and_then(|s| s.to_str()).unwrap_or("rxt")
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp, perms)?;
        let bak = cur.with_extension("bak");
        let _ = fs::remove_file(&bak);
        let _ = fs::copy(cur, &bak);
        fs::rename(&tmp, cur)?;
    }
    #[cfg(windows)]
    {
        let old = cur.with_extension("exe.old");
        let _ = fs::remove_file(&old);
        if cur.exists() {
            let _ = fs::rename(cur, &old);
        }
        fs::copy(&tmp, cur)?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}
