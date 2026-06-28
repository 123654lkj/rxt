//! trash — 安全删除(进回收站)+ 恢复 + 清理
//!
//! 终结 rm 误删血泪史。删除的文件进专用回收站 ~/.rxt-trash/,
//! 带 meta 记录原始路径, --restore 秒撤销。
//!
//! 用法:
//!   rxt trash file.txt              # 删到回收站
//!   rxt trash dir/                  # 目录也行
//!   rxt trash --list                # 看回收站
//!   rxt trash --restore <id>        # 恢复
//!   rxt trash --restore <id> --to . # 恢复到指定位置
//!   rxt trash --clean 30            # 清理 30 天前的
//!   rxt trash --purge               # 清空回收站

use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(paths: &[String], list: bool, restore: Option<&str>, restore_to: Option<&str>, clean_days: Option<u64>, purge: bool, json: bool) -> anyhow::Result<()> {
    let store = trash_store()?;

    if purge {
        return do_purge(&store);
    }
    if let Some(days) = clean_days {
        return do_clean(&store, days);
    }
    if list {
        return do_list(&store, json);
    }
    if let Some(id) = restore {
        return do_restore(&store, id, restore_to);
    }
    // 默认: 删除
    if paths.is_empty() {
        anyhow::bail!("需要文件路径,或 --list/--restore/--clean/--purge");
    }
    let mut count = 0;
    for p in paths {
        let path = Path::new(p);
        if !path.exists() {
            eprintln!("⚠ 跳过(不存在): {}", p);
            continue;
        }
        trash_one(&store, path)?;
        count += 1;
    }
    println!("🗑 已删除 {} 项到回收站 (rxt trash --list 查看, --restore 恢复)", count);
    Ok(())
}

fn trash_store() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法定位 home"))?;
    let store = home.join(".rxt-trash");
    fs::create_dir_all(&store)?;
    Ok(store)
}

fn trash_one(store: &Path, target: &Path) -> anyhow::Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let abs = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    let name = abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "unnamed".into());

    // 回收站条目: <store>/<timestamp>_<rand>/<name>
    let id = format!("{}_{:x}", now, rand_seed());
    let entry = store.join(&id);
    fs::create_dir_all(&entry)?;

    // 移动文件/目录
    let dest = entry.join(&name);
    // 跨设备移动: 先试 rename, 失败则 copy+remove
    if fs::rename(&abs, &dest).is_err() {
        if abs.is_dir() {
            copy_dir(&abs, &dest)?;
            let _ = fs::remove_dir_all(&abs);
        } else {
            fs::copy(&abs, &dest)?;
            let _ = fs::remove_file(&abs);
        }
    }

    // meta
    let meta = serde_json::json!({
        "id": id,
        "name": name,
        "orig_path": abs.display().to_string(),
        "trashed_at": now,
        "is_dir": abs.is_dir(),
    });
    fs::write(entry.join("meta.json"), meta.to_string())?;
    println!("  🗑 {} -> 回收站 [{}]", name, id);
    Ok(())
}

fn do_list(store: &Path, json: bool) -> anyhow::Result<()> {
    let mut items: Vec<(String, serde_json::Value, u128)> = Vec::new();
    for entry in fs::read_dir(store)? {
        let entry = entry?;
        let meta_path = entry.path().join("meta.json");
        if let Ok(content) = fs::read_to_string(&meta_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let ts = v.get("trashed_at").and_then(|v| v.as_u64()).unwrap_or(0) as u128;
                items.push((id, v, ts));
            }
        }
    }
    items.sort_by(|a, b| b.2.cmp(&a.2));

    if json {
        let arr: Vec<_> = items.iter().map(|(_, v, _)| v.clone()).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }

    if items.is_empty() {
        println!("回收站为空");
        return Ok(());
    }
    println!("{:<24} {:<20} {:<8} {}", "ID", "NAME", "TYPE", "ORIG_PATH");
    println!("{}", "-".repeat(90));
    for (_, m, ts) in &items {
        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let is_dir = m.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        let orig = m.get("orig_path").and_then(|v| v.as_str()).unwrap_or("");
        let time_str = chrono::DateTime::from_timestamp_millis(*ts as i64)
            .map(|d| d.format("%m-%d %H:%M").to_string()).unwrap_or_default();
        let typ = if is_dir { "DIR" } else { "FILE" };
        println!("{:<24} {:<20} {:<8} {} [{}]", id, &name[..name.len().min(20)], typ, orig, time_str);
    }
    println!("\n共 {} 项", items.len());
    Ok(())
}

fn do_restore(store: &Path, id: &str, to: Option<&str>) -> anyhow::Result<()> {
    let entry = store.join(id);
    if !entry.exists() {
        anyhow::bail!("回收站无此 ID: {} (用 --list 查看)", id);
    }
    let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(entry.join("meta.json"))?)?;
    let name = meta.get("name").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("meta 损坏"))?;
    let orig = meta.get("orig_path").and_then(|v| v.as_str()).unwrap_or(name);
    let dest_dir = to.map(PathBuf::from).unwrap_or_else(|| Path::new(orig).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")));
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(name);

    // 恢复(移动)
    let src = entry.join(name);
    if fs::rename(&src, &dest).is_err() {
        if src.is_dir() { copy_dir(&src, &dest)?; let _ = fs::remove_dir_all(&src); }
        else { fs::copy(&src, &dest)?; let _ = fs::remove_file(&src); }
    }
    // 清理空条目
    let _ = fs::remove_dir_all(&entry);
    println!("♻ 已恢复 {} -> {}", name, dest.display());
    Ok(())
}

fn do_clean(store: &Path, days: u64) -> anyhow::Result<()> {
    let cutoff = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() - (days as u128) * 86400 * 1000;
    let mut removed = 0;
    for entry in fs::read_dir(store)? {
        let entry = entry?;
        let meta_path = entry.path().join("meta.json");
        if let Ok(content) = fs::read_to_string(&meta_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                let ts = v.get("trashed_at").and_then(|v| v.as_u64()).unwrap_or(0) as u128;
                if ts < cutoff {
                    let _ = fs::remove_dir_all(entry.path());
                    removed += 1;
                }
            }
        }
    }
    println!("✓ 清理 {} 项(超过 {} 天)", removed, days);
    Ok(())
}

fn do_purge(store: &Path) -> anyhow::Result<()> {
    let mut count = 0;
    for entry in fs::read_dir(store)? {
        if entry?.path().is_dir() { count += 1; }
    }
    // 清空但保留 store 目录
    for entry in fs::read_dir(store)? {
        let _ = fs::remove_dir_all(entry?.path());
    }
    println!("✓ 已清空回收站 ({} 项)", count);
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let f = entry.path();
        let t = dst.join(entry.file_name());
        if f.is_dir() { copy_dir(&f, &t)?; }
        else { fs::copy(&f, &t)?; }
    }
    Ok(())
}

/// 简易种子(用时间 + pid)
fn rand_seed() -> u64 {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    let pid = std::process::id() as u64;
    t.wrapping_mul(pid).wrapping_add(0x9E3779B97F4A7C15)
}
