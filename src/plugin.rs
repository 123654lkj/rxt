//! 插件系统 — 让用户扩展 rxt 功能
//!
//! 插件格式:
//!   - Python 脚本 (.py)
//!   - 可执行文件 (.exe/.sh)
//!   - 配置文件 (manifest.json)
//!
//! 插件目录: ~/.rxt/plugins/
//!
//! 用法:
//!   rxt plugin list              # 列出已安装插件
//!   rxt plugin install <path>    # 安装插件
//!   rxt plugin remove <name>     # 卸载插件
//!   rxt plugin run <name> [args] # 运行插件
//!   rxt plugin info <name>       # 查看插件信息

use std::path::{Path, PathBuf};
use std::fs;

/// 插件信息
#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub command: String,
    pub args: Vec<String>,
    pub path: PathBuf,
}

/// 获取插件目录
fn plugin_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
        .join("plugins")
}

/// 确保插件目录存在
fn ensure_plugin_dir() -> anyhow::Result<PathBuf> {
    let dir = plugin_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// 扫描已安装的插件
fn scan_plugins() -> anyhow::Result<Vec<Plugin>> {
    let dir = plugin_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(Some(plugin)) = load_plugin(&path) {
                plugins.push(plugin);
            }
        }
    }
    Ok(plugins)
}

/// 从目录加载插件
fn load_plugin(dir: &Path) -> anyhow::Result<Option<Plugin>> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        // 尝试从目录名和可执行文件推断
        return load_plugin_from_dir(dir);
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&content)?;

    let name = manifest["name"].as_str().unwrap_or("unknown").to_string();
    let version = manifest["version"].as_str().unwrap_or("0.1.0").to_string();
    let description = manifest["description"].as_str().unwrap_or("").to_string();
    let author = manifest["author"].as_str().unwrap_or("unknown").to_string();

    // 查找可执行文件
    let command = find_executable(dir)?;

    Ok(Some(Plugin {
        name,
        version,
        description,
        author,
        command,
        args: Vec::new(),
        path: dir.to_path_buf(),
    }))
}

/// 从目录推断插件信息
fn load_plugin_from_dir(dir: &Path) -> anyhow::Result<Option<Plugin>> {
    let name = dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let command = find_executable(dir)?;

    Ok(Some(Plugin {
        name: name.clone(),
        version: "0.1.0".to_string(),
        description: format!("Plugin: {}", name),
        author: "unknown".to_string(),
        command,
        args: Vec::new(),
        path: dir.to_path_buf(),
    }))
}

/// 查找可执行文件
fn find_executable(dir: &Path) -> anyhow::Result<String> {
    let extensions = if cfg!(windows) {
        vec![".exe", ".bat", ".cmd", ".ps1", ".py"]
    } else {
        vec!["", ".sh", ".py", ".rb", ".pl"]
    };

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            for ext in &extensions {
                if name.ends_with(ext) {
                    return Ok(path.to_string_lossy().to_string());
                }
            }
        }
    }

    anyhow::bail!("No executable found in plugin directory")
}

/// 运行插件
fn run_plugin(plugin: &Plugin, args: &[String]) -> anyhow::Result<()> {
    use std::process::Command;

    let mut cmd = if cfg!(windows) && plugin.command.ends_with(".py") {
        let mut c = Command::new("python");
        c.arg(&plugin.command);
        c
    } else {
        Command::new(&plugin.command)
    };

    for arg in args {
        cmd.arg(arg);
    }

    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

/// 插件管理命令
pub fn run(action: &str, name: Option<&str>, path: Option<&str>, args: &[String]) -> anyhow::Result<()> {
    match action {
        "list" => list_plugins(),
        "install" => {
            let p = path.ok_or_else(|| anyhow::anyhow!("需要指定插件路径"))?;
            install_plugin(p)
        }
        "remove" | "rm" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要指定插件名"))?;
            remove_plugin(n)
        }
        "run" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要指定插件名"))?;
            let plugins = scan_plugins()?;
            let plugin = plugins.iter().find(|p| p.name == n)
                .ok_or_else(|| anyhow::anyhow!("插件 '{}' 不存在", n))?;
            run_plugin(plugin, args)
        }
        "info" => {
            let n = name.ok_or_else(|| anyhow::anyhow!("需要指定插件名"))?;
            show_plugin_info(n)
        }
        _ => {
            anyhow::bail!("未知操作 '{}', 可选: list/install/remove/run/info", action)
        }
    }
}

/// 列出已安装插件
fn list_plugins() -> anyhow::Result<()> {
    let plugins = scan_plugins()?;

    if plugins.is_empty() {
        println!("没有已安装的插件");
        println!();
        println!("安装插件: rxt plugin install <路径>");
        return Ok(());
    }

    println!("已安装插件 ({}):", plugins.len());
    println!();
    for plugin in &plugins {
        println!("  {} v{}", plugin.name, plugin.version);
        println!("    {}", plugin.description);
        println!("    路径: {}", plugin.path.display());
        println!();
    }
    Ok(())
}

/// 安装插件
fn install_plugin(source: &str) -> anyhow::Result<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        anyhow::bail!("路径 '{}' 不存在", source);
    }

    let plugin_dir = ensure_plugin_dir()?;

    let name = source_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plugin");

    let target = plugin_dir.join(name);
    if target.exists() {
        anyhow::bail!("插件 '{}' 已存在", name);
    }

    // 复制文件
    if source_path.is_dir() {
        copy_dir_all(source_path, &target)?;
    } else {
        fs::copy(source_path, &target.join(source_path.file_name().unwrap()))?;
    }

    println!("✅ 插件 '{}' 已安装", name);
    println!("   路径: {}", target.display());
    Ok(())
}

/// 卸载插件
fn remove_plugin(name: &str) -> anyhow::Result<()> {
    let plugin_dir = plugin_dir();
    let target = plugin_dir.join(name);

    if !target.exists() {
        anyhow::bail!("插件 '{}' 不存在", name);
    }

    fs::remove_dir_all(&target)?;
    println!("✅ 插件 '{}' 已卸载", name);
    Ok(())
}

/// 查看插件信息
fn show_plugin_info(name: &str) -> anyhow::Result<()> {
    let plugins = scan_plugins()?;
    let plugin = plugins.iter().find(|p| p.name == name)
        .ok_or_else(|| anyhow::anyhow!("插件 '{}' 不存在", name))?;

    println!("插件: {}", plugin.name);
    println!("版本: {}", plugin.version);
    println!("描述: {}", plugin.description);
    println!("作者: {}", plugin.author);
    println!("路径: {}", plugin.path.display());
    println!("命令: {}", plugin.command);
    Ok(())
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
