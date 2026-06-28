//! mcp — MCP server 模式 (stdio JSON-RPC)
//!
//! 让 rxt 自己当 MCP server, 不依赖 server.py。
//! 本地 stdio 模式: ZCode/客户端通过 stdin/stdout 发 JSON-RPC, rxt 响应。
//!
//! 协议: MCP (Model Context Protocol)
//!   - initialize: 握手, 返回 server info + capabilities
//!   - tools/list: 列出所有 rxt 命令(从 --describe 生成)
//!   - tools/call: 执行指定命令
//!
//! 用法:
//!   rxt mcp              # 启动 stdio MCP server
//!   rxt mcp --sse 8652   # SSE 模式(兼容旧 server.py 客户端)
//!
//! MCP 配置(ZCode):
//!   "rxt": { "type": "stdio", "command": "C:\\rxt\\rxt.exe", "args": ["mcp"] }

use std::io::{self, BufRead, Write, BufReader};
use std::process::Command;
use serde_json::{json, Value};

pub fn run(sse_port: Option<u16>) -> anyhow::Result<()> {
    if let Some(port) = sse_port {
        anyhow::bail!(
            "SSE 模式暂未实现(本地用 stdio 即可)。如需远程, 用 server.py 或 rxt --host。\n\
             本地 MCP 配置: {{\"type\":\"stdio\",\"command\":\"rxt\",\"args\":[\"mcp\"]}}"
        );
    }
    stdio_server()
}

fn stdio_server() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());

    // 输出到 stderr 不干扰 JSON-RPC(stdout)
    eprintln!("rxt MCP server (stdio) ready");

    // 缓存 tools/list 结果(避免每行重算)
    let tools_schema = build_tools_list();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("Parse error: {}",e)}});
                writeln!(stdout, "{}", resp)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => Some(handle_initialize()),
            "initialized" | "notifications/initialized" => None, // 通知, 不回复
            "tools/list" => Some(tools_schema.clone()),
            "tools/call" => Some(handle_tools_call(&params)),
            "ping" => Some(json!({})),
            _ => Some(json!({"error":{"code":-32601,"message":format!("Method not found: {}",method)}})),
        };

        if let Some(res) = result {
            let resp = if res.get("error").is_some() {
                json!({"jsonrpc":"2.0","id":id,"error":res["error"]})
            } else {
                json!({"jsonrpc":"2.0","id":id,"result":res})
            };
            writeln!(stdout, "{}", resp)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": "rxt",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

/// 从 rxt 自身的 --describe 输出生成 MCP tools/list
fn build_tools_list() -> Value {
    // 直接调自身 --describe
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("rxt"));
    let out = Command::new(&exe).arg("--describe").output();
    let describe = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return json!({"tools": []}),
    };
    let schema: Value = match serde_json::from_str(&describe) {
        Ok(v) => v,
        Err(_) => return json!({"tools": []}),
    };

    let commands = schema.get("commands").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let mut tools = Vec::new();

    for cmd in &commands {
        let name = cmd.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name.is_empty() || name == "help" { continue; }
        let about = cmd.get("about").and_then(|a| a.as_str()).unwrap_or("");
        let args = cmd.get("args").and_then(|a| a.as_array()).cloned().unwrap_or_default();

        // 构建 JSON Schema (inputSchema)
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for arg in &args {
            let aname = arg.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if aname == "help" { continue; }
            let atype = arg.get("type").and_then(|t| t.as_str()).unwrap_or("String");
            let help = arg.get("help").and_then(|h| h.as_str()).unwrap_or("");
            let is_required = arg.get("required").and_then(|r| r.as_bool()).unwrap_or(false);

            // MCP 参数名: 位置参数用原名, flag 加前缀避免冲突
            let schema_type = if atype.contains("bool") { "boolean" }
                              else if atype.contains("usize") || atype.contains("u64") || atype.contains("f64") { "number" }
                              else { "string" };
            let mut prop = json!({"type": schema_type, "description": help});
            // 数组类型(Vec)
            if atype.contains("Vec") { prop = json!({"type":"array","description":help}); }
            properties.insert(aname.to_string(), prop);
            if is_required { required.push(aname.to_string()); }
        }

        tools.push(json!({
            "name": name,
            "description": about,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
            }
        }));
    }

    json!({"tools": tools})
}

fn handle_tools_call(params: &Value) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // 查 describe 确定哪些参数是位置参数(无 long), 哪些是 flag(有 long)
    let positional = get_positional_args(name);

    // 把 arguments map 转成 rxt CLI 参数
    // 位置参数: 按 arguments 里出现的顺序收集, 放命令后(不加 --)
    // flag 参数: --key value
    let mut cmd_args: Vec<String> = vec![name.to_string()];
    let mut pos_values: Vec<String> = Vec::new();

    if let Some(obj) = arguments.as_object() {
        // 先处理位置参数(按 describe 里的顺序)
        for pname in &positional {
            if let Some(val) = obj.get(pname) {
                if let Some(s) = val.as_str() { pos_values.push(s.to_string()); }
                else if let Some(n) = val.as_f64() { pos_values.push(if n.fract()==0.0 {format!("{}",n as i64)} else {format!("{}",n)}); }
            }
        }
        // 再处理 flag 参数
        for (key, val) in obj {
            if positional.contains(key) { continue; } // 位置参数已处理
            // bool true -> 加 flag, false -> 跳过
            if let Some(b) = val.as_bool() {
                if b { cmd_args.push(format!("--{}", key)); }
                continue;
            }
            cmd_args.push(format!("--{}", key));
            if let Some(s) = val.as_str() {
                cmd_args.push(s.to_string());
            } else if let Some(n) = val.as_f64() {
                cmd_args.push(if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{}", n) });
            } else if let Some(arr) = val.as_array() {
                for a in arr {
                    if let Some(s) = a.as_str() { cmd_args.push(s.to_string()); }
                }
            }
        }
    }
    // 位置参数放最后(在 flag 之前其实也行,但放命令后更安全)
    // 实际: cmd_args = [name, pos_values..., flags...]
    let mut final_args: Vec<String> = vec![name.to_string()];
    final_args.extend(pos_values);
    final_args.extend(cmd_args.into_iter().skip(1)); // 跳过开头的 name
    let cmd_args = final_args;

    // 执行 rxt 子命令
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("rxt"));
    let result = Command::new(&exe)
        .args(&cmd_args)
        .output();

    let (content, is_error) = match result {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            if !stderr.is_empty() && o.stdout.is_empty() {
                text = stderr;
            } else if !stderr.is_empty() {
                text.push_str("\n");
                text.push_str(&stderr);
            }
            (text, !o.status.success())
        }
        Err(e) => (format!("Error executing rxt: {}", e), true),
    };

    json!({
        "content": [{ "type": "text", "text": content }],
        "isError": is_error
    })
}

/// 查 describe 获取某命令的位置参数名列表(无 long 的参数)
fn get_positional_args(cmd_name: &str) -> Vec<String> {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("rxt"));
    let out = Command::new(&exe).arg("--describe").output();
    let describe = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };
    let schema: Value = match serde_json::from_str(&describe) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let commands = schema.get("commands").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    for cmd in &commands {
        if cmd.get("name").and_then(|n| n.as_str()) == Some(cmd_name) {
            let args = cmd.get("args").and_then(|a| a.as_array()).cloned().unwrap_or_default();
            let mut pos = Vec::new();
            for arg in &args {
                let has_long = arg.get("long").is_some();
                let aname = arg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                // 位置参数: 无 long 且不是 help/host/group
                if !has_long && aname != "help" && aname != "host" && aname != "group" {
                    pos.push(aname.to_string());
                }
            }
            return pos;
        }
    }
    Vec::new()
}
