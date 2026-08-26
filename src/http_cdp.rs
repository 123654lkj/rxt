//! Lightpanda CDP — 同一套 CLI 后面的 JS 引擎。不启动 Chrome。

use super::*;
use serde_json::{json, Value};
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tungstenite::protocol::WebSocket;
use tungstenite::Message;

const HOOK_JS: &str = r#"
window.__rxt_net = window.__rxt_net || [];
window.__rxt_pending = window.__rxt_pending || 0;
(function () {
  if (window.__rxt_hooked) return;
  window.__rxt_hooked = true;
  const of = window.fetch;
  window.fetch = async function () {
    window.__rxt_pending++;
    try {
      const a0 = arguments[0];
      const u = (a0 && a0.url) ? a0.url : String(a0);
      const r = await of.apply(this, arguments);
      let t = "";
      try { t = await r.clone().text(); } catch (e) {}
      window.__rxt_net.push({
        kind: "fetch",
        url: u,
        status: r.status,
        type: (r.headers && r.headers.get("content-type")) || "",
        body: String(t).slice(0, 16000)
      });
      return r;
    } finally {
      window.__rxt_pending--;
    }
  };
  const XO = XMLHttpRequest.prototype.open;
  const XS = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u) {
    this.__rxt_m = m;
    this.__rxt_u = u;
    return XO.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function () {
    window.__rxt_pending++;
    this.addEventListener("loadend", function () {
      window.__rxt_pending--;
      window.__rxt_net.push({
        kind: "xhr",
        method: this.__rxt_m,
        url: String(this.__rxt_u),
        status: this.status,
        body: String(this.responseText || "").slice(0, 16000)
      });
    });
    return XS.apply(this, arguments);
  };
})();
true;
"#;

const SNAP_JS: &str = r#"
(() => {
  const sel = 'a[href], button, input, textarea, select, [onclick], [role="button"], [role="link"]';
  const els = Array.from(document.querySelectorAll(sel));
  return els.slice(0, 80).map((el, i) => {
    const id = "e" + (i + 1);
    el.setAttribute("data-rxt", id);
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    let role = "textbox";
    if (tag === "a") role = "link";
    else if (tag === "button" || type === "submit" || type === "button") role = "button";
    else if (type === "checkbox") role = "checkbox";
    else if (type === "hidden") role = "hidden";
    else if (tag === "select") role = "combobox";
    const text = (el.innerText || el.getAttribute("aria-label") || el.getAttribute("placeholder") || el.value || "").trim().slice(0, 120);
    return {
      id,
      role,
      name: el.getAttribute("name") || el.id || text,
      text,
      href: el.href || null,
      field: el.getAttribute("name") || null,
      value: el.value || "",
      tag
    };
  }).filter((r) => r.role !== "hidden");
})()
"#;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct EngineState {
    pub port: u16,
    pub pid: u32,
    pub target_id: String,
}

pub(super) fn available() -> bool {
    if let Ok(v) = std::env::var("RXT_HTTP_ENGINE") {
        let v = v.to_ascii_lowercase();
        if v == "static" || v == "http" {
            return false;
        }
        if v == "js" || v == "lightpanda" {
            return find_bin().is_some();
        }
    }
    find_bin().is_some()
}

fn find_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RXT_LIGHTPANDA") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(h) = dirs::home_dir() {
        let p = h.join(".rxt").join("lib").join("lightpanda");
        if p.is_file() {
            return Some(p);
        }
        #[cfg(windows)]
        {
            let p = h.join(".rxt").join("lib").join("lightpanda.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    which("lightpanda")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn rxt_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
}

fn serve_meta_path() -> PathBuf {
    rxt_dir().join("lightpanda.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ServeMeta {
    port: u16,
    pid: u32,
    #[serde(default)]
    heap_mb: u32,
}

fn heap_mb() -> u32 {
    std::env::var("RXT_HTTP_HEAP_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64)
        .clamp(16, 512)
}

fn serve_alive(port: u16, pid: u32) -> bool {
    if pid != 0 && !pid_alive(pid) {
        return false;
    }
    ureq::get(&format!("http://127.0.0.1:{port}/json/version"))
        .header("User-Agent", "rxt")
        .call()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

pub(super) fn ensure_server() -> anyhow::Result<(u16, u32)> {
    let want_heap = heap_mb();
    let meta_p = serve_meta_path();
    if let Ok(raw) = std::fs::read_to_string(&meta_p) {
        if let Ok(m) = serde_json::from_str::<ServeMeta>(&raw) {
            if serve_alive(m.port, m.pid) && m.heap_mb == want_heap {
                return Ok((m.port, m.pid));
            }
            if serve_alive(m.port, m.pid) && m.heap_mb != want_heap {
                #[cfg(unix)]
                unsafe {
                    libc::kill(m.pid as i32, libc::SIGTERM);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    let bin = find_bin().ok_or_else(|| {
        anyhow::anyhow!(
            "没有 Lightpanda。放到 ~/.rxt/lib/lightpanda 或设 RXT_LIGHTPANDA。静态引擎：RXT_HTTP_ENGINE=static"
        )
    })?;
    let port = free_port()?;
    std::fs::create_dir_all(rxt_dir())?;
    let log = rxt_dir().join("lightpanda.log");
    let logf = std::fs::File::create(&log)?;
    let heap = want_heap.to_string();
    let child = Command::new(&bin)
        .env("LIGHTPANDA_DISABLE_TELEMETRY", "true")
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--log-level",
            "warn",
            "--disable-metrics",
            "--disable-subframes",
            "--disable-workers",
            "--v8-max-heap-mb",
            &heap,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(logf.try_clone()?))
        .stderr(Stdio::from(logf))
        .spawn()
        .map_err(|e| anyhow::anyhow!("启动 lightpanda 失败: {e}"))?;
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if serve_alive(port, pid) {
            let m = ServeMeta {
                port,
                pid,
                heap_mb: want_heap,
            };
            std::fs::write(&meta_p, serde_json::to_vec_pretty(&m)?)?;
            eprintln!("# js engine lightpanda pid={pid} port={port} heap={want_heap}MB");
            return Ok((port, pid));
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    anyhow::bail!(
        "lightpanda 未在 :{port} 起来。日志 {}",
        log.display()
    )
}

fn free_port() -> anyhow::Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

pub(super) struct Cdp {
    ws: WebSocket<TcpStream>,
    next_id: u64,
}

impl Cdp {
    pub(super) fn connect(port: u16) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| anyhow::anyhow!("连 CDP :{port} 失败: {e}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        stream.set_write_timeout(Some(Duration::from_secs(20)))?;
        let key = tungstenite::handshake::client::generate_key();
        let req = tungstenite::http::Request::builder()
            .method("GET")
            .uri(format!("ws://127.0.0.1:{port}/"))
            .header("Host", format!("127.0.0.1:{port}"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", key)
            .body(())
            .map_err(|e| anyhow::anyhow!("CDP handshake: {e}"))?;
        let (ws, _) = tungstenite::client::client(req, stream)
            .map_err(|e| anyhow::anyhow!("CDP websocket: {e}"))?;
        Ok(Self { ws, next_id: 0 })
    }

    fn call(&mut self, method: &str, params: Value, sid: Option<&str>) -> anyhow::Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = sid {
            msg["sessionId"] = json!(s);
        }
        self.ws
            .send(Message::Text(msg.to_string()))
            .map_err(|e| anyhow::anyhow!("CDP send {method}: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if Instant::now() > deadline {
                anyhow::bail!("CDP 超时 {method}");
            }
            let msg = self
                .ws
                .read()
                .map_err(|e| anyhow::anyhow!("CDP read {method}: {e}"))?;
            let Message::Text(t) = msg else {
                continue;
            };
            let v: Value = serde_json::from_str(&t)?;
            if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    anyhow::bail!(
                        "CDP {method}: {}",
                        err.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or(&err.to_string())
                    );
                }
                return Ok(v.get("result").cloned().unwrap_or(json!({})));
            }
        }
    }

    pub(super) fn attach_or_create(
        &mut self,
        want_tid: Option<&str>,
        url: &str,
    ) -> anyhow::Result<(String, String)> {
        if let Some(tid) = want_tid {
            if let Ok(att) = self.call(
                "Target.attachToTarget",
                json!({"targetId": tid, "flatten": true}),
                None,
            ) {
                if let Some(sid) = att.get("sessionId").and_then(|s| s.as_str()) {
                    return Ok((tid.to_string(), sid.to_string()));
                }
            }
        }
        let created = self.call("Target.createTarget", json!({"url": url}), None)?;
        let tid = created
            .get("targetId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("createTarget 无 targetId"))?
            .to_string();
        let att = self.call(
            "Target.attachToTarget",
            json!({"targetId": tid, "flatten": true}),
            None,
        )?;
        let sid = att
            .get("sessionId")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("attach 无 sessionId"))?
            .to_string();
        Ok((tid, sid))
    }

    fn enable(&mut self, sid: &str) -> anyhow::Result<()> {
        let _ = self.call("Page.enable", json!({}), Some(sid));
        let _ = self.call("Runtime.enable", json!({}), Some(sid));
        let _ = self.call("Network.enable", json!({}), Some(sid));
        Ok(())
    }

    pub(super) fn eval(&mut self, sid: &str, expr: &str) -> anyhow::Result<Value> {
        let r = self.call(
            "Runtime.evaluate",
            json!({
                "expression": expr,
                "returnByValue": true,
                "awaitPromise": true
            }),
            Some(sid),
        )?;
        if r.get("exceptionDetails").is_some() {
            anyhow::bail!(
                "JS 异常: {}",
                r.get("exceptionDetails")
                    .and_then(|d| d.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or(&r.to_string())
            );
        }
        Ok(r.get("result")
            .and_then(|x| x.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn eval_str(&mut self, sid: &str, expr: &str) -> anyhow::Result<String> {
        Ok(match self.eval(sid, expr)? {
            Value::String(s) => s,
            Value::Null => String::new(),
            other => other.to_string(),
        })
    }

    fn wait_ready(&mut self, sid: &str, timeout: Duration) -> anyhow::Result<()> {
        let start = Instant::now();
        loop {
            let ready = self.eval_str(sid, "document.readyState").unwrap_or_default();
            let pending = self
                .eval(sid, "window.__rxt_pending||0")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if ready == "complete" && pending <= 0 && start.elapsed() > Duration::from_millis(80) {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Ok(()); // 超时也继续，页面可能长轮询
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct HoldMeta {
    port: u16,
    pid: u32,
}

fn hold_path(dir: &Path) -> PathBuf {
    dir.join("hold.json")
}

fn hold_alive(dir: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(hold_path(dir)) else {
        return false;
    };
    let Ok(m) = serde_json::from_str::<HoldMeta>(&raw) else {
        return false;
    };
    hold_rpc_raw(m.port, &json!({"op": "ping"}))
        .ok()
        .and_then(|v| v.get("ok").and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

fn hold_rpc_raw(port: u16, req: &Value) -> anyhow::Result<Value> {
    use std::io::BufRead;
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    s.set_write_timeout(Some(Duration::from_secs(30)))?;
    s.write_all(req.to_string().as_bytes())?;
    s.write_all(b"\n")?;
    let mut r = std::io::BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim())?)
}

fn hold_rpc(dir: &Path, req: Value) -> anyhow::Result<Value> {
    ensure_hold(dir)?;
    let raw = std::fs::read_to_string(hold_path(dir))?;
    let m: HoldMeta = serde_json::from_str(&raw)?;
    let v = hold_rpc_raw(m.port, &req)?;
    if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        anyhow::bail!(
            "{}",
            v.get("error")
                .and_then(|e| e.as_str())
                .unwrap_or(&v.to_string())
        );
    }
    Ok(v)
}

fn ensure_hold(dir: &Path) -> anyhow::Result<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if hold_alive(dir) {
        return Ok(());
    }
    let d = dir.to_path_buf();
    std::fs::create_dir_all(&d)?;
    let exe = std::env::current_exe()?;
    let name = exe
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.contains("rxt-http") || name.contains("rxt-tools") {
        let mut cmd = Command::new(&exe);
        if name.contains("rxt-tools") && !name.contains("rxt-http") {
            cmd.args(["http", "hold"]);
        } else {
            cmd.arg("hold");
        }
        cmd.env("RXT_HTTP_SESSION_DIR", &d)
            .env("RXT_HTTP_ENGINE", "js")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("启动 hold 失败: {e}"))?;
    } else {
        std::thread::Builder::new()
            .name("rxt-http-hold".into())
            .spawn(move || {
                let _ = hold_loop(&d);
            })?;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if hold_alive(dir) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("JS hold 没起来。看 ~/.rxt/lightpanda.log")
}

pub(super) fn hold_loop(dir: &Path) -> anyhow::Result<()> {
    use std::io::{BufRead, Write};
    let (cport, cpid) = ensure_server()?;
    let mut cdp = Cdp::connect(cport)?;
    let (tid, sid) = cdp.attach_or_create(None, "about:blank")?;
    cdp.enable(&sid)?;
    let _ = cdp.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": HOOK_JS}),
        Some(&sid),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let hport = listener.local_addr()?.port();
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        hold_path(dir),
        serde_json::to_vec_pretty(&HoldMeta {
            port: hport,
            pid: std::process::id(),
        })?,
    )?;
    std::fs::write(
        dir.join("engine.json"),
        serde_json::to_vec_pretty(&EngineState {
            port: cport,
            pid: cpid,
            target_id: tid.clone(),
        })?,
    )?;
    for conn in listener.incoming() {
        let mut s = match conn {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut line = String::new();
        {
            let mut br = std::io::BufReader::new(&s);
            if br.read_line(&mut line).is_err() {
                continue;
            }
        }
        let req: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(e) => {
                let _ = s.write_all(format!("{{\"ok\":false,\"error\":\"{e}\"}}\n").as_bytes());
                continue;
            }
        };
        let op = req.get("op").and_then(|x| x.as_str()).unwrap_or("");
        let resp = match op {
            "ping" => json!({"ok": true}),
            "nav" => match hold_nav(&mut cdp, &sid, &tid, cport, cpid, dir, &req) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            },
            "eval" => {
                let expr = req.get("expr").and_then(|x| x.as_str()).unwrap_or("");
                match cdp.eval(&sid, expr) {
                    Ok(v) => json!({"ok": true, "value": v}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            }
            "click" | "fill" => {
                match hold_act(&mut cdp, &sid, &tid, cport, cpid, dir, &req) {
                    Ok(v) => json!({"ok": true, "value": v}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            }
            "dump" => {
                let jar = PathBuf::from(
                    req.get("jar")
                        .and_then(|x| x.as_str())
                        .unwrap_or("cookies.txt"),
                );
                let jar = if jar.is_absolute() {
                    jar
                } else {
                    dir.join(jar)
                };
                match dump_session(&mut cdp, &sid, dir, &jar, cpid, cport, &tid) {
                    Ok(()) => json!({"ok": true}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            }
            "cookies" => match hold_set_cookies(&mut cdp, &sid, dir, &req) {
                Ok(n) => json!({"ok": true, "n": n}),
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            },
            "quit" => {
                let _ = s.write_all(b"{\"ok\":true}\n");
                break;
            }
            _ => json!({"ok": false, "error": format!("unknown op {op}")}),
        };
        let _ = s.write_all(format!("{resp}\n").as_bytes());
    }
    Ok(())
}

fn hold_nav(
    cdp: &mut Cdp,
    sid: &str,
    tid: &str,
    cport: u16,
    cpid: u32,
    dir: &Path,
    req: &Value,
) -> anyhow::Result<()> {
    let url = req
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow::anyhow!("nav 需要 url"))?;
    let timeout = req.get("timeout").and_then(|t| t.as_u64()).unwrap_or(15);
    let jar = req
        .get("jar")
        .and_then(|x| x.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("cookies.txt"));
    let mut recs = Vec::new();
    if jar.exists() {
        recs.extend(load_netscape(&jar).unwrap_or_default());
    }
    let json_p = dir.join("cookies.json");
    if json_p.exists() {
        if let Ok(more) = load_cookie_json(&json_p.to_string_lossy(), "") {
            upsert_cookies(&mut recs, &more);
        }
    }
    if let Some(arr) = req.get("cookies").and_then(|x| x.as_array()) {
        if let Ok(more) = serde_json::from_value::<Vec<CookieRec>>(serde_json::Value::Array(arr.clone())) {
            upsert_cookies(&mut recs, &more);
        }
    }
    for c in recs {
        let mut p = json!({"name": c.name, "value": c.value, "path": c.path});
        if !c.domain.is_empty() {
            p["domain"] = json!(c.domain.trim_start_matches('.'));
        }
        if c.secure {
            p["secure"] = json!(true);
        }
        if c.http_only {
            p["httpOnly"] = json!(true);
        }
        let _ = cdp.call("Network.setCookie", p, Some(sid));
    }
    cdp.call("Page.navigate", json!({"url": url}), Some(sid))?;
    cdp.wait_ready(sid, Duration::from_secs(timeout.max(3)))?;
    dump_session(cdp, sid, dir, &jar, cpid, cport, tid)
}

fn hold_act(
    cdp: &mut Cdp,
    sid: &str,
    tid: &str,
    cport: u16,
    cpid: u32,
    dir: &Path,
    req: &Value,
) -> anyhow::Result<Value> {
    let id = req
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("需要 id"))?;
    let _ = cdp.eval(sid, SNAP_JS)?;
    let expr = if req.get("op").and_then(|x| x.as_str()) == Some("fill") {
        let value = req.get("value").and_then(|x| x.as_str()).unwrap_or("");
        let vjson = serde_json::to_string(value)?;
        format!(
            r#"(() => {{
              const el = document.querySelector('[data-rxt="{id}"]');
              if (!el) return "missing";
              el.focus();
              if ('value' in el) el.value = {vjson};
              el.dispatchEvent(new Event('input', {{bubbles:true}}));
              el.dispatchEvent(new Event('change', {{bubbles:true}}));
              return "ok";
            }})()"#
        )
    } else {
        format!(
            r#"(() => {{
              const el = document.querySelector('[data-rxt="{id}"]');
              if (!el) return "missing";
              el.click();
              return "ok";
            }})()"#
        )
    };
    let v = cdp.eval(sid, &expr)?;
    if v.as_str() == Some("missing") {
        let info = cdp
            .eval(
                sid,
                r#"JSON.stringify({title:document.title,href:location.href,buttons:document.querySelectorAll('button').length,rxt:document.querySelectorAll('[data-rxt]').length,html:(document.body&&document.body.innerHTML||'').slice(0,180)})"#,
            )
            .unwrap_or(Value::Null);
        anyhow::bail!("没有 @{id}。{info}");
    }
    let jar = dir.join("cookies.txt");
    dump_session(cdp, sid, dir, &jar, cpid, cport, tid)?;
    Ok(v)
}

fn hold_set_cookies(
    cdp: &mut Cdp,
    sid: &str,
    dir: &Path,
    req: &Value,
) -> anyhow::Result<usize> {
    let recs: Vec<CookieRec> = if let Some(arr) = req.get("cookies").and_then(|x| x.as_array()) {
        serde_json::from_value(serde_json::Value::Array(arr.clone()))?
    } else {
        let mut recs = load_netscape(&dir.join("cookies.txt")).unwrap_or_default();
        if dir.join("cookies.json").exists() {
            if let Ok(more) = load_cookie_json(&dir.join("cookies.json").to_string_lossy(), "") {
                upsert_cookies(&mut recs, &more);
            }
        }
        recs
    };
    for c in &recs {
        let mut p = json!({"name": c.name, "value": c.value, "path": c.path});
        if !c.domain.is_empty() {
            p["domain"] = json!(c.domain.trim_start_matches('.'));
        }
        let _ = cdp.call("Network.setCookie", p, Some(sid));
    }
    Ok(recs.len())
}

pub(super) fn inject_session_cookies(dir: &Path, recs: &[CookieRec]) -> anyhow::Result<()> {
    if recs.is_empty() {
        return Ok(());
    }
    if hold_path(dir).exists() {
        let _ = hold_rpc(
            dir,
            json!({"op": "cookies", "cookies": recs}),
        );
    }
    Ok(())
}

pub(super) fn open_js(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    url: &str,
) -> anyhow::Result<EngineState> {
    let host = host_of(url);
    let mut recs = if jar.exists() {
        load_netscape(jar).unwrap_or_default()
    } else {
        Vec::new()
    };
    let (src, more) = gather_cookies(opts, host.as_deref())?;
    upsert_cookies(&mut recs, &more);
    if !recs.is_empty() {
        persist_login(dir, &recs, &src)?;
        eprintln!("# 登录态 {} 条 from {src} → {}", recs.len(), dir.display());
    }
    hold_rpc(
        dir,
        json!({
            "op": "nav",
            "url": url,
            "timeout": opts.timeout,
            "jar": jar.display().to_string(),
            "cookies": recs,
        }),
    )?;
    load_engine(dir)
}

pub(super) fn refresh(_opts: &HttpOpts<'_>, dir: &Path, jar: &Path) -> anyhow::Result<()> {
    hold_rpc(
        dir,
        json!({"op": "dump", "jar": jar.display().to_string()}),
    )?;
    Ok(())
}

pub(super) fn eval_js(dir: &Path, expr: &str) -> anyhow::Result<Value> {
    let v = hold_rpc(dir, json!({"op": "eval", "expr": expr}))?;
    Ok(v.get("value").cloned().unwrap_or(Value::Null))
}

pub(super) fn click_js(dir: &Path, id: &str) -> anyhow::Result<()> {
    hold_rpc(dir, json!({"op": "click", "id": id}))?;
    Ok(())
}

pub(super) fn fill_js(dir: &Path, id: &str, value: &str) -> anyhow::Result<()> {
    hold_rpc(dir, json!({"op": "fill", "id": id, "value": value}))?;
    Ok(())
}

pub(super) fn hold_quit(dir: &Path) -> anyhow::Result<()> {
    if let Ok(raw) = std::fs::read_to_string(hold_path(dir)) {
        if let Ok(m) = serde_json::from_str::<HoldMeta>(&raw) {
            let _ = hold_rpc_raw(m.port, &json!({"op": "quit"}));
        }
    }
    let _ = std::fs::remove_file(hold_path(dir));
    Ok(())
}

pub(super) fn close_js() -> anyhow::Result<()> {
    if let Some(home) = dirs::home_dir() {
        let sess = home.join(".rxt").join("http-session");
        if let Ok(rd) = std::fs::read_dir(sess) {
            for e in rd.flatten() {
                let p = e.path();
                let _ = hold_rpc(&p, json!({"op": "quit"}));
                let _ = std::fs::remove_file(p.join("hold.json"));
            }
        }
    }
    let p = serve_meta_path();
    if let Ok(raw) = std::fs::read_to_string(&p) {
        if let Ok(m) = serde_json::from_str::<ServeMeta>(&raw) {
            #[cfg(unix)]
            unsafe {
                libc::kill(m.pid as i32, libc::SIGTERM);
            }
        }
    }
    let _ = std::fs::remove_file(p);
    println!("OK closed js engine");
    Ok(())
}

fn dump_session(
    cdp: &mut Cdp,
    sid: &str,
    dir: &Path,
    jar: &Path,
    pid: u32,
    port: u16,
    tid: &str,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let html = cdp.eval_str(sid, "document.documentElement.outerHTML")?;
    let url = cdp.eval_str(sid, "location.href")?;
    let title = cdp.eval_str(sid, "document.title")?;
    let refs = cdp.eval(sid, SNAP_JS)?;
    let net = cdp.eval(sid, "window.__rxt_net||[]")?;
    let storage = cdp.eval(
        sid,
        r#"(() => { const o=(s)=>{const x={}; try{for(let i=0;i<s.length;i++){const k=s.key(i); x[k]=s.getItem(k);} }catch(e){} return x;}; return {local:o(localStorage), session:o(sessionStorage)}; })()"#,
    )?;
    std::fs::write(dir.join("page.html"), html.as_bytes())?;
    std::fs::write(
        dir.join("meta.json"),
        serde_json::to_vec_pretty(&json!({"url": url, "title": title, "status": 200}))?,
    )?;
    std::fs::write(dir.join("refs.json"), serde_json::to_vec_pretty(&refs)?)?;
    std::fs::write(dir.join("net.json"), serde_json::to_vec_pretty(&net)?)?;
    std::fs::write(dir.join("storage.json"), serde_json::to_vec_pretty(&storage)?)?;
    std::fs::write(
        dir.join("engine.json"),
        serde_json::to_vec_pretty(&EngineState {
            port,
            pid,
            target_id: tid.to_string(),
        })?,
    )?;
    if let Ok(cookies) = pull_cookies(cdp, sid) {
        if !cookies.is_empty() {
            let _ = save_netscape(jar, &cookies);
            let _ = persist_login(dir, &cookies, "engine");
        }
    }
    Ok(())
}

fn inject_cookies(
    cdp: &mut Cdp,
    sid: &str,
    opts: &HttpOpts<'_>,
    jar: &Path,
) -> anyhow::Result<()> {
    let mut recs = if jar.exists() {
        load_netscape(jar).unwrap_or_default()
    } else {
        Vec::new()
    };
    recs.extend(parse_cookie_args(opts.cookies));
    if let Some(raw) = opts.cookie_json.or(cookie_env().json.as_deref()) {
        if let Ok(more) = load_cookie_json(raw, "") {
            recs.extend(more);
        }
    }
    for c in recs {
        let mut p = json!({
            "name": c.name,
            "value": c.value,
            "path": c.path,
        });
        if !c.domain.is_empty() {
            p["domain"] = json!(c.domain.trim_start_matches('.'));
        }
        if c.secure {
            p["secure"] = json!(true);
        }
        if c.http_only {
            p["httpOnly"] = json!(true);
        }
        let _ = cdp.call("Network.setCookie", p, Some(sid));
    }
    Ok(())
}

fn pull_cookies(cdp: &mut Cdp, sid: &str) -> anyhow::Result<Vec<CookieRec>> {
    let r = cdp.call("Network.getAllCookies", json!({}), Some(sid))?;
    let mut out = Vec::new();
    if let Some(arr) = r.get("cookies").and_then(|c| c.as_array()) {
        for c in arr {
            out.push(CookieRec {
                domain: c
                    .get("domain")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                path: c
                    .get("path")
                    .and_then(|x| x.as_str())
                    .unwrap_or("/")
                    .to_string(),
                secure: c.get("secure").and_then(|x| x.as_bool()).unwrap_or(false),
                http_only: c.get("httpOnly").and_then(|x| x.as_bool()).unwrap_or(false),
                expires: c.get("expires").and_then(|x| x.as_f64()).map(|f| f as u64),
                name: c
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                value: c
                    .get("value")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(out)
}

pub(super) fn load_engine(dir: &Path) -> anyhow::Result<EngineState> {
    let raw = std::fs::read_to_string(dir.join("engine.json")).map_err(|_| {
        anyhow::anyhow!("当前会话不是 JS 引擎。rxt http open 会自动用 Lightpanda")
    })?;
    Ok(serde_json::from_str(&raw)?)
}

pub(super) fn engine_wanted(opts: &HttpOpts<'_>) -> bool {
    let v = opts
        .engine
        .map(|s| s.to_string())
        .or_else(|| std::env::var("RXT_HTTP_ENGINE").ok())
        .unwrap_or_else(|| "auto".into())
        .to_ascii_lowercase();
    match v.as_str() {
        "static" | "http" | "off" | "0" => false,
        "js" | "lightpanda" | "on" | "1" => true,
        _ => available(),
    }
}
