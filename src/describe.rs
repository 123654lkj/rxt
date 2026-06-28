//! 自描述协议 — `rxt --describe` 输出所有子命令 schema
//!
//! v0.4.0: 改成从 clap 反射自动生成, 加命令无需手写.
//! 之前是 36 个手写 commands.push, 极易漂移.

use serde_json::{json, Value};
use clap::Command;
use clap::CommandFactory;

pub fn run() -> anyhow::Result<()> {
    let schema = build_schema();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}

/// 从 clap 反射生成完整 schema
fn build_schema() -> Value {
    let mut cmd = crate::Cli::command();
    // 让 clap 把所有子命令都展开
    cmd.build();

    let commands: Vec<Value> = cmd.get_subcommands().map(|sub| {
        let name = sub.get_name().to_string();
        let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
        let args: Vec<Value> = sub.get_arguments().map(|a| {
            let mut obj = json!({
                "name": a.get_id().to_string(),
                "required": a.is_required_set(),
            });
            // long flag (--foo)
            if let Some(l) = a.get_long() {
                obj["long"] = json!(l.to_string());
            }
            // short flag (-f)
            if let Some(s) = a.get_short() {
                obj["short"] = json!(s.to_string());
            } else {
                obj["short"] = Value::Null;
            }
            // 帮助文本
            if let Some(h) = a.get_help() {
                obj["help"] = json!(h.to_string());
            }
            // 类型(从 value type 推断)
            obj["type"] = json!(infer_type(&a.get_id().to_string(), a.get_long().is_some()));
            // 默认值
            if let Some(d) = a.get_default_values().first() {
                obj["default"] = json!(d.to_string_lossy().to_string());
            }
            obj
        }).collect();

        let mut entry = json!({
            "name": name,
            "about": about,
            "args": args,
        });

        // 特例: jq 保留 syntax_hints(硬编码, clap 无法反射)
        if name == "jq" {
            entry["syntax_hints"] = json!({
                "builtins": ["length","keys","values","type","select","map","has","contains","unique","sort","sort_by","reverse","first","last","nth","min","max","min_by","max_by","group_by","flatten","ascii_downcase","ascii_upcase","tostring","tonumber","to_entries","from_entries","with_entries","add","empty","not","recurse","walk"],
                "examples": [".foo[0].name",".users[] | select(.active) | .name","[.items[].x] | @csv",".users | sort_by(.age) | map(.name)"],
                "formats": ["@csv","@json","@text","@tsv","@uri","@base64","@html"],
                "operators": ["==","!=","<","<=",">",">=","and","or","not","+","-","*","/","%"]
            });
        }
        entry
    }).collect();

    json!({
        "name": "rxt",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Rust Codex Tools - AI's Cross-Platform IDE",
        "global_flags": [
            {"name": "host", "long": "host", "type": "Option<String>", "help": "远程主机(~/.rxt/hosts.toml)"},
            {"name": "group", "long": "group", "type": "Option<String>", "help": "远程主机组(批量执行)"},
        ],
        "commands": commands,
        "describe_flags": [
            {"name": "describe", "long": "describe", "type": "bool", "help": "输出所有子命令 schema(JSON)"},
        ],
    })
}

/// 从参数名/是否有 long 推断类型(简化, 够 AI 看)
fn infer_type(id: &str, has_long: bool) -> &'static str {
    if id == "help" || id == "version" { return "bool"; }
    if has_long {
        // 带 --flag 的, 多数是 Option 或 bool
        "Option<String>"
    } else {
        "String"
    }
}