//! 自描述协议 — `rxt --describe` 输出所有子命令 schema
//!
//! v0.4.0: 改成从 clap 反射自动生成, 加命令无需手写.
//! 之前是 36 个手写 commands.push, 极易漂移.

use clap::{Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Value};

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
            // 类型(从 clap 反射的真实 value type 推断, 区分 bool/number/string)
            obj["type"] = json!(infer_type(a));
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

/// 从 clap Arg 的真实 action/value type 推断 schema 类型字符串。
/// 用 get_action() 判断 bool flag (ArgAction::SetTrue), 用 value_parser 判断数值。
/// 这是 v0.4.1 修复: 之前用 "有 long 就当 Option<String>" 的启发式,
/// 导致所有 bool flag(count/regex/json/functions...) 全被报成 Option<String>,
/// 进而让 mcp.rs 生成的 MCP schema 把它们标成 string, AI 被迫传 "true" 字符串。
fn infer_type(a: &Arg) -> &'static str {
    // bool flag: clap 对 #[arg(...)] bool 字段自动用 SetTrue action
    if matches!(a.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
        return "bool";
    }
    // help/version 也是 bool
    let id = a.get_id().to_string();
    if id == "help" || id == "version" {
        return "bool";
    }
    // 数值: 通过 value_parser 的类型名判断
    let parser = a.get_value_parser();
    let type_name = format!("{:?}", parser);
    if type_name.contains("usize")
        || type_name.contains("u64")
        || type_name.contains("i64")
        || type_name.contains("u32")
        || type_name.contains("i32")
    {
        return "number";
    }
    // 默认: 字符串(位置参数 String, 或 Option<String>/Option<PathBuf> flag)
    "Option<String>"
}
