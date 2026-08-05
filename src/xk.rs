//! 星控浏览器控制 — rxt 集成模块
//!
//! 解决的坑：
//! 1. --host 远程模式：SSH 到虎虎，用 curl 调远程 xkd
//! 2. snapshot 精简输出：默认只返回 refs 列表，AI 友好
//! 3. screenshot 存文件：--save 参数存到下载目录
//! 4. cookies 导出/导入：--export-cookies / --inject 命令
//! 5. Chrome headless 启动：--start-chrome 命令（在远程主机上启动 headless Chrome）
//! 6. UA 伪装：inject 命令自动隐藏 HeadlessChrome

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;

const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:26800";

#[derive(Serialize)]
struct CommandRequest {
    action: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    args: Value,
}

#[derive(Deserialize)]
struct CommandResponse {
    ok: bool,
    data: Option<Value>,
    error: Option<Value>,
}

/// 获取 daemon URL（环境变量或默认值）
fn daemon_url() -> String {
    std::env::var("XK_DAEMON_URL").unwrap_or_else(|_| DEFAULT_DAEMON_URL.to_string())
}

/// 本地调用 daemon
fn call_local(action: &str, args: Value) -> Result<Value> {
    let url = daemon_url();

    if action == "status" {
        let resp: Value = ureq::get(&format!("{}/status", url))
            .call()
            .context("无法连接星控 daemon")?
            .into_body()
            .read_json()?;
        return Ok(resp);
    }

    let req = CommandRequest {
        action: action.to_string(),
        args,
    };

    let resp: CommandResponse = ureq::post(&format!("{}/command", url))
        .send_json(&req)
        .context("无法连接星控 daemon")?
        .into_body()
        .read_json()?;

    if resp.ok {
        Ok(resp.data.unwrap_or(json!({})))
    } else {
        let msg = resp.error
            .and_then(|e| e.get("message").cloned())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "未知错误".to_string());
        bail!("{}", msg);
    }
}

/// 远程调用 daemon（通过 SSH）
fn call_remote(host: &str, action: &str, args: Value) -> Result<Value> {
    let req_json = serde_json::to_string(&json!({"action": action, "args": args}))?;

    let curl_cmd = if action == "status" {
        "curl -s http://127.0.0.1:26800/status".to_string()
    } else {
        format!("curl -s -X POST http://127.0.0.1:26800/command -H 'Content-Type: application/json' -d '{}'",
                req_json.replace("'", "'\\''"))
    };

    let output = Command::new(std::env::current_exe()?)
        .args(["exec", "--host", host, "--json"])
        .arg(&curl_cmd)
        .output()
        .context("SSH 执行失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim())
        .context(format!("解析远程响应失败: {}", stdout))?;

    if action == "status" {
        return Ok(value);
    }

    if value["ok"].as_bool().unwrap_or(false) {
        Ok(value["data"].clone())
    } else {
        bail!("{}", value["error"]["message"].as_str().unwrap_or("远程错误"))
    }
}

/// 统一调用入口（自动选择本地/远程）
fn call(action: &str, args: Value, host: Option<&str>) -> Result<Value> {
    if let Some(h) = host {
        call_remote(h, action, args)
    } else {
        call_local(action, args)
    }
}

// ===== rxt xk 命令入口 =====

pub fn run(
    action: &str,
    host: Option<&str>,
    url: Option<&str>,
    new_tab: bool,
    selector: Option<&str>,
    value: Option<&str>,
    code: Option<&str>,
    keys: Option<&str>,
    save: Option<&str>,
    full: bool,
    json_output: bool,
) -> Result<()> {
    match action {
        // ===== 浏览器操作 =====
        "status" => {
            let r = call("status", json!({}), host)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!("运行: {}  版本: {}  扩展: {}  运行时间: {}s",
                    r["running"].as_bool().unwrap_or(false),
                    r["version"].as_str().unwrap_or("?"),
                    if r["extension_connected"].as_bool().unwrap_or(false) { "已连接" } else { "未连接" },
                    r["uptime_seconds"].as_u64().unwrap_or(0));
            }
        }

        "nav" | "navigate" => {
            let u = url.context("需要 --url")?;
            let r = call("navigate", json!({"url": u, "newTab": new_tab}), host)?;
            println!("OK: {} → {}", r["url"].as_str().unwrap_or("?"), r["tabId"].as_str().or(r["tabId"].as_u64().map(|n| n.to_string()).as_deref()).unwrap_or(""));
        }

        "snap" | "snapshot" => {
            let r = call("snapshot", json!({"full": full}), host)?;
            if full {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                // AI 友好精简输出
                println!("URL: {}", r["url"].as_str().unwrap_or(""));
                println!("Title: {}", r["title"].as_str().unwrap_or(""));
                if let Some(refs) = r["refs"].as_array() {
                    println!("可交互元素 ({} 个):", refs.len());
                    for rf in refs {
                        let name = rf["name"].as_str().unwrap_or("");
                        let role = rf["role"].as_str().unwrap_or("");
                        let ref_str = rf["ref"].as_str().unwrap_or("");
                        let display = if name.is_empty() { role.to_string() } else { format!("{} \"{}\"", role, name) };
                        println!("  {:8} {}", ref_str, display);
                    }
                }
            }
        }

        "click" => {
            let s = selector.context("需要 --selector")?;
            call("click", json!({"selector": s}), host)?;
            println!("OK");
        }

        "fill" => {
            let s = selector.context("需要 --selector")?;
            let v = value.context("需要 --value")?;
            call("fill", json!({"selector": s, "value": v}), host)?;
            println!("OK");
        }

        "eval" | "evaluate" => {
            let c = code.context("需要 --code")?;
            let r = call("evaluate", json!({"code": c}), host)?;
            let val = r["value"].clone();
            if val.is_null() {
                println!("{}", r["type"].as_str().unwrap_or("undefined"));
            } else if val.is_string() {
                println!("{}", val.as_str().unwrap_or(""));
            } else {
                println!("{}", serde_json::to_string_pretty(&val)?);
            }
        }

        "shot" | "screenshot" => {
            let mut args = json!({});
            if let Some(s) = save { args["save"] = json!(s); }
            let r = call("screenshot", args, host)?;
            if r.get("saved").is_some() {
                println!("已保存: {}", r["path"].as_str().unwrap_or(""));
            } else {
                println!("格式: {} 大小: {} bytes",
                    r["format"].as_str().unwrap_or("?"),
                    r["dataLength"].as_u64().unwrap_or(0));
            }
        }

        "tabs" | "list_tabs" => {
            let r = call("list_tabs", json!({}), host)?;
            if let Some(tabs) = r["tabs"].as_array() {
                for tab in tabs {
                    let mark = if tab["active"].as_bool().unwrap_or(false) { " *" } else { "  " };
                    println!("{}{}", mark, tab["title"].as_str().unwrap_or(""));
                    println!("    {}", tab["url"].as_str().unwrap_or(""));
                }
            }
        }

        "close" | "close_tab" => {
            call("close_tab", json!({}), host)?;
            println!("OK");
        }

        // ===== Cookie 导出/导入 =====
        "export-cookies" | "export_full" => {
            let domain = url.unwrap_or("all");
            let save_name = save.unwrap_or("login-state.json");
            let r = call("export_full", json!({"domain": domain, "save": save_name}), host)?;
            println!("导出完成:");
            println!("  cookies: {}", r["cookieCount"].as_u64().unwrap_or(0));
            println!("  域名数: {}", r.get("domains").and_then(|d| d.as_array()).map(|a| a.len()).unwrap_or(0));
            let fp_keys: Vec<String> = r.get("fingerprintKeys").and_then(|f| f.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            println!("  指纹字段: {:?}", fp_keys);
            if let Some(p) = r["path"].as_str() {
                println!("  保存到: ~/Downloads/{}", p);
            }
        }

        "export-fp" | "export_fingerprint" => {
            let save_name = save.unwrap_or("fingerprint.json");
            let r = call("export_fingerprint", json!({"save": save_name}), host)?;
            println!("指纹导出:");
            println!("  UA: {}", r["userAgent"].as_str().unwrap_or(""));
            println!("  屏幕: {}x{}", r["screen"]["width"].as_u64().unwrap_or(0), r["screen"]["height"].as_u64().unwrap_or(0));
            println!("  时区: {}", r["timezone"].as_str().unwrap_or(""));
            if let Some(p) = r["path"].as_str() {
                println!("  保存到: ~/Downloads/{}", p);
            }
        }

        // ===== Chrome 管理（远程）=====
        "start-chrome" => {
            let h = host.context("start-chrome 需要 --host")?;
            start_headless_chrome(h)?;
        }

        "inject" => {
            let h = host.context("inject 需要 --host")?;
            let cookies_file = value.context("inject 需要文件路径参数")?;
            inject_cookies(h, cookies_file)?;
        }

        // ===== 通用 raw 调用 =====
        "raw" => {
            let raw_action = url.context("raw 需要 action 名称作为参数")?;
            let mut args = json!({});
            if let Some(s) = selector { args["selector"] = json!(s); }
            if let Some(v) = value { args["value"] = json!(v); }
            if let Some(c) = code { args["code"] = json!(c); }
            if let Some(k) = keys { args["keys"] = json!(k); }
            if full { args["full"] = json!(true); }
            let r = call(raw_action, args, host)?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }

        "hclick" | "human_click" | "human-click" => {
            let s = selector.context("需要 --selector")?;
            let r = call("human_click", json!({"selector": s}), host)?;
            println!("OK: ({},{})", r["x"].as_f64().unwrap_or(0.0), r["y"].as_f64().unwrap_or(0.0));
        }

        "htype" | "human_type" | "human-type" => {
            let v = value.context("需要 --value")?;
            let mut args = json!({"value": v});
            if let Some(s) = selector { args["selector"] = json!(s); }
            let r = call("human_type", args, host)?;
            println!("OK: 输入了 {} 个字符", r["typed"].as_u64().unwrap_or(0));
        }

        "stealth" | "enable_stealth" => {
            let r = call("enable_stealth", json!({}), host)?;
            println!("OK: {}", r["message"].as_str().unwrap_or("已注入"));
        }

        _ => {
            bail!("未知动作: {}。\n可用动作:\n  浏览器: status, nav, snap, click, fill, eval, shot, tabs, close\n  人类模拟: hclick, htype, stealth\n  Cookie: export-cookies, export-fp\n  Chrome: start-chrome, inject\n  通用: raw <action>", action);
        }
    }

    Ok(())
}

/// 在远程主机上启动 headless Chrome + xkd
fn start_headless_chrome(host: &str) -> Result<()> {
    let script = r#"pkill -f 'xingkong-chrome' 2>/dev/null; pkill -f 'xkd' 2>/dev/null; sleep 1; rm -f /home/huhu/xingkong-chrome/SingletonLock; google-chrome --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --remote-debugging-port=9222 --remote-debugging-address=127.0.0.1 --remote-allow-origins=* --user-data-dir=/home/huhu/xingkong-chrome --window-size=1920,1080 --disable-blink-features=AutomationControlled --no-first-run --disable-popup-blocking about:blank &>/home/huhu/xk-chrome.log & sleep 3; WS_URL=$(curl -s http://127.0.0.1:9222/json/version | python3 -c 'import json,sys; print(json.load(sys.stdin)["webSocketDebuggerUrl"])'); cd /home/huhu/xingkong-crates && ./target/release/xkd --port 26800 --cdp-url "$WS_URL" &>/home/huhu/xk-daemon.log & sleep 2; curl -s http://127.0.0.1:26800/status"#;

    let output = Command::new(std::env::current_exe()?)
        .args(["exec", "--host", host])
        .arg(script)
        .output()
        .context("远程执行失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Chrome + daemon 启动:");
    println!("{}", stdout);
    Ok(())
}

/// 远程注入 cookies + 伪装 UA
fn inject_cookies(host: &str, cookies_file: &str) -> Result<()> {
    // 检查远程文件是否存在
    let script = format!(
        r#"WS_URL=$(curl -s http://127.0.0.1:9222/json/version | python3 -c 'import json,sys; print(json.load(sys.stdin)["webSocketDebuggerUrl"])')
python3 -c '
import json, sys, websocket, time
ws = websocket.create_connection(sys.argv[1], timeout=5)
ws.settimeout(5)
cid = [0]
def cdp(method, params=None, sid=None):
    cid[0] += 1
    msg = {{"id": cid[0], "method": method}}
    if params: msg["params"] = params
    if sid: msg["sessionId"] = sid
    ws.send(json.dumps(msg))
    for _ in range(50):
        r = json.loads(ws.recv())
        if r.get("id") == cid[0]: return r
    return {{}}
# attach
r = cdp("Target.getTargets")
tid = next((t["targetId"] for t in r.get("result",{{}}).get("targetInfos",[]) if t["type"]=="page"), None)
if not tid: print("ERROR: no page"); sys.exit(1)
r = cdp("Target.attachToTarget", {{"targetId": tid, "flatten": True}})
sid = r.get("result",{{}}).get("sessionId","")
# 伪装 UA
FAKE_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36"
cdp("Network.setUserAgentOverride", {{"userAgent": FAKE_UA}}, sid)
# 注入 cookies
with open(sys.argv[2]) as f: data = json.load(f)
cookies = data.get("cookies", data) if isinstance(data, dict) else data
ok = fail = 0
for c in cookies:
    try:
        p = {{"name": c["name"], "value": c["value"], "domain": c["domain"], "path": c.get("path","/")}}
        if c.get("expirationDate"): p["expires"] = c["expirationDate"]
        if c.get("secure"): p["secure"] = True
        if c.get("httpOnly"): p["httpOnly"] = True
        r = cdp("Network.setCookie", p, sid)
        if r.get("result",{{}}).get("success", True): ok += 1
        else: fail += 1
    except: fail += 1
# 验证
r = cdp("Runtime.evaluate", {{"expression": "navigator.userAgent", "returnByValue": True}}, sid)
ua = r.get("result",{{}}).get("result",{{}}).get("value","")
print(f"Cookies: {{ok}} ok, {{fail}} fail")
print(f"UA: {{ua}}")
print(f"Hidden: {{'HeadlessChrome' not in ua}}")
ws.close()
' "$WS_URL" {}"#,
        cookies_file
    );

    let output = Command::new(std::env::current_exe()?)
        .args(["exec", "--host", host])
        .arg(script)
        .output()
        .context("远程注入失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        println!("{}", stdout);
    }
    if !stderr.is_empty() && !output.status.success() {
        bail!("{}", stderr);
    }
    Ok(())
}
