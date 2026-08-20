use std::fs;
use std::path::Path;

/// 依赖分析 — Cargo.toml 依赖信息
pub fn run(target: &str, tree: bool, json_output: bool, check: bool) -> anyhow::Result<()> {
    if tree {
        return dep_tree(target, json_output);
    }

    let path = if target.ends_with("Cargo.toml") {
        Path::new(target).to_path_buf()
    } else {
        // Try to find Cargo.toml
        let p = Path::new(target);
        if p.join("Cargo.toml").exists() {
            p.join("Cargo.toml")
        } else {
            p.to_path_buf()
        }
    };

    if !path.exists()
        && !path.to_string_lossy().contains('/')
        && !path.to_string_lossy().contains('\\')
    {
        // Maybe it's a crate name, try cargo search
        return search_crate(target, json_output);
    }

    let content = fs::read_to_string(&path)?;
    let parsed: toml::Value = content
        .parse()
        .map_err(|e| anyhow::anyhow!("parse error: {}", e))?;

    if json_output {
        let deps = extract_deps_json(&parsed);
        println!("{}", serde_json::to_string_pretty(&deps)?);
    } else {
        print_deps(&parsed, &path);
    }

    if check {
        dep_check(target)?;
    }

    Ok(())
}

fn print_deps(parsed: &toml::Value, path: &Path) {
    let name = parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("?");
    let version = parsed
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let edition = parsed
        .get("package")
        .and_then(|p| p.get("edition"))
        .and_then(|e| e.as_str())
        .unwrap_or("?");

    println!("Crate: {} v{} (edition {})", name, version, edition);
    println!("File: {}", path.display());
    println!();

    if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_table()) {
        println!("Dependencies:");
        let mut sorted: Vec<_> = deps.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (name, val) in &sorted {
            let info = dep_info(val);
            println!("  {} {}", name, info);
        }
    }

    if let Some(deps) = parsed.get("build-dependencies").and_then(|d| d.as_table()) {
        println!("\nBuild Dependencies:");
        for (name, val) in deps {
            println!("  {} {}", name, dep_info(val));
        }
    }

    if let Some(deps) = parsed.get("dev-dependencies").and_then(|d| d.as_table()) {
        println!("\nDev Dependencies:");
        for (name, val) in deps {
            println!("  {} {}", name, dep_info(val));
        }
    }

    if let Some(features) = parsed.get("features").and_then(|f| f.as_table()) {
        println!("\nFeatures:");
        let mut sorted: Vec<_> = features.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (name, val) in &sorted {
            if let Some(features_list) = val.as_array() {
                let enabled: Vec<String> = features_list
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !enabled.is_empty() {
                    println!("  {} = [{}]", name, enabled.join(", "));
                }
            }
        }
    }
}

fn dep_info(val: &toml::Value) -> String {
    if let Some(s) = val.as_str() {
        format!("\"{}\"", s)
    } else if let Some(table) = val.as_table() {
        let mut parts = Vec::new();
        if let Some(v) = table.get("version").and_then(|v| v.as_str()) {
            parts.push(format!("v{}", v));
        }
        if let Some(f) = table.get("features").and_then(|f| f.as_array()) {
            let feats: Vec<&str> = f.iter().filter_map(|v| v.as_str()).collect();
            if !feats.is_empty() {
                parts.push(format!("features=[{}]", feats.join(",")));
            }
        }
        if let Some(true) = table.get("optional").and_then(|o| o.as_bool()) {
            parts.push("optional".to_string());
        }
        if let Some(g) = table.get("git").and_then(|g| g.as_str()) {
            parts.push(format!("git={}", g));
        }
        parts.join(" ")
    } else {
        "?".to_string()
    }
}

fn extract_deps_json(parsed: &toml::Value) -> serde_json::Value {
    let name = parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str());
    let version = parsed
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str());

    let mut deps = Vec::new();
    if let Some(table) = parsed.get("dependencies").and_then(|d| d.as_table()) {
        for (name, val) in table {
            let info = if let Some(s) = val.as_str() {
                serde_json::json!({"version": s})
            } else {
                serde_json::json!({})
            };
            deps.push(serde_json::json!({"name": name, "info": info, "kind": "normal"}));
        }
    }

    serde_json::json!({
        "name": name,
        "version": version,
        "dependencies": deps
    })
}

fn dep_tree(target: &str, json_output: bool) -> anyhow::Result<()> {
    let output = std::process::Command::new("cargo")
        .args([
            "tree",
            "-p",
            target,
            if json_output { "--format=json" } else { "" },
        ])
        .args(if json_output {
            &[][..]
        } else {
            &["--prefix", "depth"][..]
        })
        .output()
        .map_err(|e| anyhow::anyhow!("cargo tree failed: {}", e))?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("{}", stderr);
    }
    Ok(())
}

fn search_crate(name: &str, json_output: bool) -> anyhow::Result<()> {
    let output = std::process::Command::new("cargo")
        .args(["search", name, "--limit", "5"])
        .output()
        .map_err(|e| anyhow::anyhow!("cargo search failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if json_output {
        let results: Vec<serde_json::Value> = stdout
            .lines()
            .map(|l| serde_json::json!({"info": l}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        println!("Search results for '{}':", name);
        println!("{}", stdout);
    }
    Ok(())
}

fn dep_check(target: &str) -> anyhow::Result<()> {
    let output = std::process::Command::new("cargo")
        .args(["update", "-p", target, "--dry-run"])
        .output();
    if let Ok(o) = output {
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if !stdout.trim().is_empty() {
                println!("Available updates:");
                println!("{}", stdout);
            } else {
                println!("All dependencies up to date");
            }
        }
    }
    Ok(())
}
