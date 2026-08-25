//! recipe — 命令宏录制/重放/列表
//!
//! 把重复的命令序列变成一个词。recipe 存 ~/.rxt-recipes/<name>.sh,
//! 支持位置参数 $1 $2 ...(运行时替换)。
//!
//! 用法:
//!   rxt recipe add backup "rxt snapshot . --label daily; rxt git push"
//!   rxt recipe add deploy "cargo build --release && cp target/release/rxt.exe /c/rxt/"
//!   rxt recipe list                    # 列出所有
//!   rxt backup                         # 0.9.4：未知子命令回退到 recipe
//!   rxt recipe run backup              # 执行（带横幅）
//!   rxt recipe run deploy --extra "v2" # 传参
//!   rxt recipe show backup             # 看内容
//!   rxt recipe rm backup               # 删除
//!   rxt recipe run backup --dry-run    # 只看会执行什么

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

thread_local! {
    static RECIPES_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn run(
    action: &str,
    name: Option<&str>,
    content: Option<&str>,
    args: &[String],
    dry_run: bool,
    json: bool,
) -> anyhow::Result<()> {
    let store = recipe_store()?;
    match action {
        "add" | "create" | "set" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要 recipe 名"))?;
            let c = content.ok_or_else(|| anyhow::anyhow!("需要 recipe 内容(用引号包裹)"))?;
            add(&store, n, c)
        }
        "list" | "ls" => list(&store, json),
        "run" | "exec" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要 recipe 名"))?;
            // clap 把 NAME 后第一个位置参数吃进 content；run 时并入 $1..$n
            // 用法: rxt recipe run explore /path/to/dir
            let mut run_args: Vec<String> = Vec::new();
            if let Some(c) = content {
                if !c.is_empty() {
                    run_args.push(c.to_string());
                }
            }
            run_args.extend_from_slice(args);
            run_recipe(&store, n, &run_args, dry_run, false)
        }
        "show" | "cat" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要 recipe 名"))?;
            show(&store, n)
        }
        "rm" | "del" | "delete" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要 recipe 名"))?;
            remove(&store, n)
        }
        "edit" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要 recipe 名"))?;
            edit(&store, n)
        }
        other => anyhow::bail!("未知操作 '{}',可选: add/list/run/show/rm/edit", other),
    }
}

fn recipe_store_path() -> PathBuf {
    if let Some(p) = RECIPES_DIR_OVERRIDE.with(|s| s.borrow().clone()) {
        return p;
    }
    if let Ok(p) = std::env::var("RXT_RECIPES_DIR") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt-recipes")
}

fn recipe_store() -> anyhow::Result<PathBuf> {
    let store = recipe_store_path();
    fs::create_dir_all(&store)?;
    Ok(store)
}

fn recipe_path(store: &Path, name: &str) -> PathBuf {
    let safe = sanitize(name);
    let preferred_ext = if cfg!(windows) { "cmd" } else { "sh" };
    let preferred = store.join(format!("{safe}.{preferred_ext}"));
    if preferred.is_file() {
        return preferred;
    }
    let alt_ext = if cfg!(windows) { "sh" } else { "cmd" };
    let alt = store.join(format!("{safe}.{alt_ext}"));
    if alt.is_file() {
        return alt;
    }
    preferred
}

/// 不创建目录。没有 recipe 时返回 None。
pub fn resolve_path(name: &str) -> Option<PathBuf> {
    if sanitize(name).is_empty() {
        return None;
    }
    let p = recipe_path(&recipe_store_path(), name);
    p.is_file().then_some(p)
}

pub fn list_entries() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let store = recipe_store_path();
    let Ok(rd) = fs::read_dir(&store) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            if !stem.is_empty() {
                out.push((stem.to_string(), p));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 未知子命令回退：找到 recipe 则执行，返回 Ok(true)。
pub fn try_run_as_command(name: &str, args: &[String]) -> anyhow::Result<bool> {
    if name.is_empty() || resolve_path(name).is_none() {
        return Ok(false);
    }
    let store = recipe_store()?;
    run_recipe(&store, name, args, false, true)?;
    Ok(true)
}

#[cfg(test)]
pub(crate) fn with_recipes_dir<F, R>(dir: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RECIPES_DIR_OVERRIDE.with(|s| *s.borrow_mut() = None);
        }
    }
    let _g = Guard;
    let _ = fs::create_dir_all(dir);
    RECIPES_DIR_OVERRIDE.with(|s| *s.borrow_mut() = Some(dir.to_path_buf()));
    f()
}

fn add(store: &Path, name: &str, content: &str) -> anyhow::Result<()> {
    let safe = sanitize(name);
    if safe.is_empty() {
        anyhow::bail!("recipe 名不能为空");
    }
    let path = recipe_path(store, name);
    fs::write(&path, content)?;
    // 计算行数和命令数
    let lines = content.lines().filter(|l| !l.trim().is_empty()).count();
    println!(
        "✓ recipe '{}' 已保存 ({} 行, {})",
        name,
        lines,
        path.display()
    );
    println!("运行: rxt {name}");
    println!("  或: rxt recipe run {name}");
    Ok(())
}

fn list(store: &Path, json: bool) -> anyhow::Result<()> {
    let mut recipes: Vec<(String, String, usize)> = Vec::new(); // (name, preview, lines)
    if let Ok(rd) = fs::read_dir(store) {
        for entry in rd.flatten() {
            let p = entry.path();
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if let Ok(content) = fs::read_to_string(&p) {
                    let preview: String = content
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(50)
                        .collect();
                    let lines = content.lines().filter(|l| !l.trim().is_empty()).count();
                    recipes.push((stem.to_string(), preview, lines));
                }
            }
        }
    }
    recipes.sort_by(|a, b| a.0.cmp(&b.0));
    if json {
        let arr: Vec<_> = recipes
            .iter()
            .map(|(n, p, l)| serde_json::json!({"name": n, "lines": l, "preview": p}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
        return Ok(());
    }
    if recipes.is_empty() {
        println!("(无 recipe。用 rxt recipe add <名> \"命令\" 创建)");
        return Ok(());
    }
    println!("{:<20} {:>5} {}", "NAME", "LINES", "PREVIEW");
    println!("{}", "-".repeat(80));
    for (n, p, l) in &recipes {
        println!("{:<20} {:>5} {}", n, l, p);
    }
    println!("\n共 {} 个 recipe", recipes.len());
    Ok(())
}

fn run_recipe(
    store: &Path,
    name: &str,
    args: &[String],
    dry_run: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let path = recipe_path(store, name);
    if !path.exists() {
        anyhow::bail!("recipe '{}' 不存在 (用 rxt recipe list 查看)", name);
    }
    let mut content = fs::read_to_string(&path)?;
    // 替换位置参数 $1 $2 ...
    for (i, arg) in args.iter().enumerate() {
        content = content.replace(&format!("${}", i + 1), arg);
    }

    if dry_run {
        println!("(dry-run, 不执行)\n{}", content);
        return Ok(());
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (shell, flag) = if ext == "cmd" || ext == "bat" {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    if !quiet {
        println!("▶ 执行 recipe '{}':", name);
    }
    let status = Command::new(shell)
        .arg(flag)
        .arg(&content)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;
    if status.success() {
        if !quiet {
            println!("\n✓ recipe '{}' 完成", name);
        }
    } else {
        anyhow::bail!(
            "recipe '{}' 失败 (exit {})",
            name,
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn show(store: &Path, name: &str) -> anyhow::Result<()> {
    let path = recipe_path(store, name);
    if !path.exists() {
        anyhow::bail!("recipe '{}' 不存在", name);
    }
    let content = fs::read_to_string(&path)?;
    println!("# recipe: {}\n{}", name, content);
    Ok(())
}

fn remove(store: &Path, name: &str) -> anyhow::Result<()> {
    let path = recipe_path(store, name);
    if !path.exists() {
        anyhow::bail!("recipe '{}' 不存在", name);
    }
    fs::remove_file(&path)?;
    println!("✓ 已删除 recipe '{}'", name);
    Ok(())
}

fn edit(store: &Path, name: &str) -> anyhow::Result<()> {
    let path = recipe_path(store, name);
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
        if cfg!(windows) {
            "notepad".into()
        } else {
            "nano".into()
        }
    });
    let status = Command::new(&editor).arg(&path).status()?;
    if !status.success() {
        anyhow::bail!("编辑器退出码 {}", status.code().unwrap_or(-1));
    }
    println!("✓ recipe '{}' 已编辑", name);
    Ok(())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_run_missing_is_false() {
        let dir = std::env::temp_dir().join(format!("rxt-recipe-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        with_recipes_dir(&dir, || {
            assert!(!try_run_as_command("nope", &[]).unwrap());
            assert!(resolve_path("nope").is_none());
        });
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_then_try_run() {
        let dir = std::env::temp_dir().join(format!("rxt-recipe-add-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        with_recipes_dir(&dir, || {
            run("add", Some("t1"), Some("echo t1-ok"), &[], false, false).unwrap();
            assert!(resolve_path("t1").is_some());
            assert!(try_run_as_command("t1", &[]).unwrap());
            let names: Vec<_> = list_entries().into_iter().map(|(n, _)| n).collect();
            assert!(names.iter().any(|n| n == "t1"));
        });
        let _ = fs::remove_dir_all(&dir);
    }
}
