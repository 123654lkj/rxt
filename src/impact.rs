//! impact — 改动爆炸半径分析 (v0.8.0)
//!
//! 给定改动的文件, 反向计算受影响的调用者链 (blast radius).
//! 灵感: codeseek/coderadius 的 blast radius + loop-engineering 的 impact 分析.
//!
//! rxt impact src/remote.rs             # 给定文件
//! rxt impact --diff                    # 自动取 git diff 改动的文件
//! rxt impact src/remote.rs --json

use serde_json::json;
use std::path::{Path, PathBuf};

pub fn run(files: &[PathBuf], use_diff: bool, root: &Path, json: bool) -> anyhow::Result<()> {
    // 确定改动的文件列表
    let changed: Vec<String> = if use_diff {
        // 从 git diff 获取改动文件
        let output = crate::git::git(&["diff", "--name-only", "HEAD"])?;
        output
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        files
            .iter()
            .map(|f| {
                // 转成相对 root 的路径
                f.strip_prefix(root)
                    .map(|r| r.display().to_string())
                    .unwrap_or_else(|_| f.display().to_string())
            })
            .collect()
    };

    if changed.is_empty() {
        if json {
            println!("{}", json!({"impacted": []}));
        } else {
            println!("没有指定改动文件 (--diff 未检测到改动, 或未传文件参数).");
        }
        return Ok(());
    }

    let cg = crate::callgraph::CallGraph::build(root)?;
    let impacted = cg.impact(&changed);

    if impacted.is_empty() {
        if json {
            println!("{}", json!({"impacted": [], "changed_files": changed}));
        } else {
            println!("改动的文件里没有发现符号, 或没有其他函数调用它们.");
        }
        return Ok(());
    }

    if json {
        let arr: Vec<_> = impacted.iter().map(|(dist, n)| json!({
            "distance": dist, "name": n.name, "kind": n.kind, "file": n.file, "line": n.line,
        })).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "changed_files": changed,
                "impacted_count": impacted.len(),
                "impacted": arr,
            }))?
        );
    } else {
        println!(
            "💥 爆炸半径 — 改动 {} 个文件, 影响 {} 个符号",
            changed.len(),
            impacted.len()
        );
        println!("   改动: {}", changed.join(", "));
        println!();
        let mut current_dist = 0;
        for (dist, node) in &impacted {
            if *dist != current_dist {
                current_dist = *dist;
                let label = match *dist {
                    0 => "📍 直接改动".to_string(),
                    1 => "⚠️  直接影响 (1 跳)".to_string(),
                    d => format!("🔄 间接影响 ({} 跳)", d),
                };
                println!("\n{}", label);
            }
            println!("  {}:{}  {} {}", node.file, node.line, node.kind, node.name);
        }
    }
    Ok(())
}
