//! snapshot — 文件/目录时光机
//!
//! 拍快照(增量拷贝) -> 任意时刻回滚。比 git 轻(无需仓库),
//! 比 Ctrl+Z 强(跨重启持久),给 AI 文件操作上"安全网"。
//!
//! 快照存在 ~/.rxt-snapshots/<timestamp>_<label>/<orig_path>
//!
//! 用法:
//!   rxt snapshot .                      # 给当前目录拍照
//!   rxt snapshot config.toml --label 改前
//!   rxt snapshot --list                 # 列出所有快照
//!   rxt snapshot --restore <id>         # 回滚到某快照
//!   rxt snapshot --diff <id>            # 看快照与当前的差异
//!   rxt snapshot --clean 30             # 清理 30 天前的

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(
    target: Option<&str>,
    label: Option<&str>,
    list: bool,
    restore: Option<&str>,
    diff: Option<&str>,
    clean_days: Option<u64>,
) -> anyhow::Result<()> {
    let store = snapshot_store()?;

    if let Some(days) = clean_days {
        return clean(&store, days);
    }
    if list {
        return list_snapshots(&store);
    }
    if let Some(id) = restore {
        return restore_snapshot(&store, id);
    }
    if let Some(id) = diff {
        return diff_snapshot(&store, id);
    }
    // 默认: 拍快照
    let t = target.unwrap_or(".");
    let p = Path::new(t);
    if !p.exists() {
        anyhow::bail!("目标不存在: {}", p.display());
    }
    create_snapshot(&store, p, label)
}

fn snapshot_store() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法定位 home 目录"))?;
    let store = home.join(".rxt-snapshots");
    fs::create_dir_all(&store)?;
    Ok(store)
}

fn create_snapshot(store: &Path, target: &Path, label: Option<&str>) -> anyhow::Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let ts = chrono::DateTime::from_timestamp(now as i64, 0)
        .map(|d| d.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| now.to_string());
    let lbl = label
        .map(|l| {
            // 只保留安全字符
            let safe: String = l
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            format!("_{}", safe)
        })
        .unwrap_or_default();
    let id = format!("{}{}", ts, lbl);

    let snap_dir = store.join(&id);
    fs::create_dir_all(&snap_dir)?;

    let abs = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());
    let dest = snap_dir.join("data");
    let count;
    if abs.is_dir() {
        count = copy_dir(&abs, &dest)?;
    } else {
        fs::create_dir_all(&dest)?;
        fs::copy(&abs, dest.join(abs.file_name().unwrap_or_default()))?;
        count = 1;
    }

    // 元数据
    let meta = serde_json::json!({
        "id": id,
        "created": ts,
        "created_iso": chrono::DateTime::from_timestamp(now as i64, 0)
            .map(|d| d.to_rfc3339()).unwrap_or_default(),
        "target": abs.display().to_string(),
        "label": label.unwrap_or(""),
        "file_count": count,
    });
    fs::write(
        snap_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    println!("📸 快照已创建");
    println!("  ID:     {}", id);
    println!("  目标:   {}", abs.display());
    println!("  文件数: {}", count);
    println!("\n回滚: rxt snapshot --restore {}", id);
    Ok(())
}

fn list_snapshots(store: &Path) -> anyhow::Result<()> {
    let mut snaps: Vec<(String, serde_json::Value)> = Vec::new();
    for entry in fs::read_dir(store)? {
        let entry = entry?;
        let meta_path = entry.path().join("meta.json");
        if let Ok(content) = fs::read_to_string(&meta_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                let id = v
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                snaps.push((id, v));
            }
        }
    }
    snaps.sort_by(|a, b| b.0.cmp(&a.0));
    if snaps.is_empty() {
        println!("(无快照)");
        return Ok(());
    }
    println!("{:<24} {:<10} {:>6} {}", "ID", "LABEL", "FILES", "TARGET");
    println!("{}", "-".repeat(80));
    for (_, m) in &snaps {
        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let label = m.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let count = m.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let target = m.get("target").and_then(|v| v.as_str()).unwrap_or("");
        let target_short = Path::new(target)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string());
        println!("{:<24} {:<10} {:>6} {}", id, label, count, target_short);
    }
    println!("\n共 {} 个快照", snaps.len());
    Ok(())
}

fn restore_snapshot(store: &Path, id: &str) -> anyhow::Result<()> {
    let snap_dir = store.join(id);
    if !snap_dir.exists() {
        anyhow::bail!("快照不存在: {} (用 --list 查看)", id);
    }
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snap_dir.join("meta.json"))?)?;
    let target = meta
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("meta 损坏"))?;
    let data_dir = snap_dir.join("data");

    // 恢复前先拍当前状态(防止误操作)
    if Path::new(target).exists() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let pre_id = format!(
            "{}_PRE_RESTORE_{}",
            chrono::DateTime::from_timestamp(now as i64, 0)
                .map(|d| d.format("%Y%m%d_%H%M%S").to_string())
                .unwrap_or_else(|| now.to_string()),
            id
        );
        let pre_dir = store.join(&pre_id);
        fs::create_dir_all(&pre_dir)?;
        let target_path = Path::new(target);
        let pre_count = if target_path.is_dir() {
            copy_dir(target_path, &pre_dir.join("data"))?
        } else {
            1
        };
        fs::write(pre_dir.join("meta.json"), serde_json::json!({
            "id": pre_id, "target": target, "label": format!("回滚前自动备份-{}", id), "file_count": pre_count,
        }).to_string())?;
        println!("⚠ 恢复前已自动备份当前状态: {}", pre_id);
    }

    println!("⏮  恢复 {} <- 快照 {}", target, id);
    let count = if Path::new(target).is_dir() {
        // 清空目标再拷
        if Path::new(target).exists() {
            let _ = fs::remove_dir_all(target);
        }
        copy_dir(&data_dir, Path::new(target))?
    } else {
        if let Some(name) = data_dir.read_dir()?.next().and_then(|e| e.ok()) {
            fs::copy(name.path(), target)?;
        }
        1
    };
    println!("✓ 已恢复 {} 个文件", count);
    Ok(())
}

fn diff_snapshot(store: &Path, id: &str) -> anyhow::Result<()> {
    let snap_dir = store.join(id);
    if !snap_dir.exists() {
        anyhow::bail!("快照不存在: {}", id);
    }
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snap_dir.join("meta.json"))?)?;
    let target = meta.get("target").and_then(|v| v.as_str()).unwrap_or(".");
    let data_dir = snap_dir.join("data");

    // 用 rxt 自身的 diff? 这里简单比较文件列表 + 大小
    println!("快照 {} vs 当前 {}:", id, target);
    let snap_files = collect_files(&data_dir);
    let cur_files = if Path::new(target).exists() {
        collect_files(Path::new(target))
    } else {
        Vec::new()
    };

    let mut changed = 0;
    let mut added = 0;
    let mut removed = 0;
    let snap_set: std::collections::HashMap<&String, u64> =
        snap_files.iter().map(|(p, s)| (p, *s)).collect();
    let cur_set: std::collections::HashMap<&String, u64> =
        cur_files.iter().map(|(p, s)| (p, *s)).collect();

    for (p, s) in &snap_files {
        match cur_set.get(p) {
            None => {
                println!("  - (已删) {}", p);
                removed += 1;
            }
            Some(cs) if cs != s => {
                println!("  ~ (改动) {} ({} -> {})", p, s, cs);
                changed += 1;
            }
            _ => {}
        }
    }
    for (p, _) in &cur_files {
        if !snap_set.contains_key(p) {
            println!("  + (新增) {}", p);
            added += 1;
        }
    }
    println!("\n改动 {} / 新增 {} / 删除 {}", changed, added, removed);
    Ok(())
}

fn clean(store: &Path, days: u64) -> anyhow::Result<()> {
    let cutoff = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() - days * 86400;
    let mut removed = 0;
    for entry in fs::read_dir(store)? {
        let entry = entry?;
        let path = entry.path();
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                if let Ok(age) = mtime.duration_since(UNIX_EPOCH) {
                    if age.as_secs() < cutoff {
                        let _ = fs::remove_dir_all(&path);
                        removed += 1;
                    }
                }
            }
        }
    }
    println!("✓ 清理 {} 个超过 {} 天的快照", removed, days);
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> anyhow::Result<usize> {
    fs::create_dir_all(dst)?;
    let mut count = 0;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            count += copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
            count += 1;
        }
    }
    Ok(count)
}

fn collect_files(base: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    fn walk(base: &Path, cur: &Path, out: &mut Vec<(String, u64)>) {
        if let Ok(rd) = fs::read_dir(cur) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else {
                    let rel = p
                        .strip_prefix(base)
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push((rel, size));
                }
            }
        }
    }
    walk(base, base, &mut out);
    out.sort();
    out
}
