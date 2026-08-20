//! trace — 多跳调用链追踪 (v0.8.0)
//!
//! 给定一个符号, 输出 N 跳调用链 (refs 是单跳, trace 是传递闭包).
//! 灵感: codeseek trace 的跨文件调用链 + graphcode 的调用图.
//!
//! rxt trace connect_async              # 默认向下 3 跳 (它调用了谁)
//! rxt trace connect_async --depth 5
//! rxt trace connect_async --up         # 向上 (谁调用了它)
//! rxt trace connect_async --json

use serde_json::json;
use std::path::Path;

pub fn run(
    symbol: &str,
    root: &Path,
    depth: usize,
    upward: bool,
    json: bool,
) -> anyhow::Result<()> {
    let cg = crate::callgraph::CallGraph::build(root)?;

    // 确认符号存在
    if cg.find_node(symbol).is_none() {
        if json {
            println!(
                "{}",
                json!({"error": format!("symbol '{}' not found", symbol)})
            );
        } else {
            println!("找不到符号 '{}'.", symbol);
        }
        return Ok(());
    }

    let direction = if upward {
        "向上 (谁调用了它)"
    } else {
        "向下 (它调用了谁)"
    };
    let trace_result = cg.trace(symbol, depth, !upward);

    if trace_result.is_empty() {
        if json {
            println!("{}", json!({"symbol": symbol, "trace": []}));
        } else {
            println!(" '{}' {} {} 跳内没有找到调用链.", symbol, direction, depth);
        }
        return Ok(());
    }

    if json {
        let arr: Vec<_> = trace_result
            .iter()
            .map(|(d, n)| {
                json!({
                    "depth": d, "name": n.name, "kind": n.kind, "file": n.file, "line": n.line,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "symbol": symbol, "direction": direction, "max_depth": depth, "trace": arr,
            }))?
        );
    } else {
        let arrow = if upward { "←" } else { "→" };
        println!("🔗 trace '{}' {} {} 跳", symbol, direction, depth);
        println!();
        for (d, node) in &trace_result {
            let indent = "  ".repeat(*d);
            let prefix = if *d == 0 { "" } else { &format!("{} ", arrow) };
            println!(
                "{}{}{} ({} {}:{})",
                indent, prefix, node.name, node.kind, node.file, node.line
            );
        }
    }
    Ok(())
}
