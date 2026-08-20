//! dead — 死代码检测 (v0.8.0)
//!
//! 从入口点(main/pub/export/tests)做可达性分析, 找出不可达的函数.
//! 灵感: codebase-memory-mcp 的 dead_code 检测 + fable5 的 AUDIT.md.
//!
//! rxt dead          # 扫描项目, 列出不可达函数
//! rxt dead --json

use serde_json::json;
use std::collections::BTreeMap;

pub fn run(root: &std::path::Path, json: bool) -> anyhow::Result<()> {
    let cg = crate::callgraph::CallGraph::build(root)?;
    let (total_nodes, total_edges, total_entries) = cg.stats();

    let dead = cg.dead_code();

    if dead.is_empty() {
        if json {
            println!("{}", json!({"dead_count": 0, "total_symbols": total_nodes}));
        } else {
            println!(
                "💀 没有发现死代码 ({} 个符号全部可达, {} 个入口, {} 条边)",
                total_nodes, total_entries, total_edges
            );
        }
        return Ok(());
    }

    // 按文件分组
    let mut by_file: BTreeMap<String, Vec<&crate::callgraph::SymNode>> = BTreeMap::new();
    for node in &dead {
        by_file.entry(node.file.clone()).or_default().push(*node);
    }

    if json {
        let arr: Vec<_> = dead
            .iter()
            .map(|n| {
                json!({
                    "name": n.name, "kind": n.kind, "file": n.file, "line": n.line,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dead_count": dead.len(),
                "total_symbols": total_nodes,
                "dead": arr,
            }))?
        );
    } else {
        println!(
            "💀 死代码检测 — {} 个不可达函数 (共 {} 个符号, {} 条边)",
            dead.len(),
            total_nodes,
            total_edges
        );
        println!("   (从 main/pub/export/tests 出发, 不可达的函数)");
        println!();
        for (file, nodes) in &by_file {
            println!("── {} ──", file);
            for n in nodes {
                println!("  L{:4}   {} {}", n.line, n.kind, n.name);
            }
            println!();
        }
    }
    Ok(())
}
