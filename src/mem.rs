//! 星枢记忆 CLI — 对接 Nebula v5（真牛逼）
//!
//! 修复痛点（相对 0.8.2）：
//! 1. 默认连 huhu `:26670`，不再误打 daemon `:26671`
//! 2. search 走 `/ask`（contract/pack），省 token
//! 3. save 走 `/memory/add`（密钥拦截）
//! 4. 支持 extract / bootstrap / health
//! 5. URL 可由 RXT_NEBULA_URL / NEBULA_URL 覆盖
//! 6. `mem ask` = `mem search` 别名（Agent 友好）

use crate::common::setup_utf8_console;
use std::io::Write;
use std::time::Duration;

/// 默认星枢地址（局域网 huhu；本机跑服务时用 env 覆盖）
fn nebula_base() -> String {
    if let Ok(u) = std::env::var("RXT_NEBULA_URL") {
        if !u.trim().is_empty() {
            return u.trim().trim_end_matches('/').to_string();
        }
    }
    if let Ok(u) = std::env::var("NEBULA_URL") {
        if !u.trim().is_empty() {
            return u.trim().trim_end_matches('/').to_string();
        }
    }
    // Windows/跨机默认 huhu；在 huhu 本机也可设 NEBULA_URL=http://127.0.0.1:26670
    "http://192.168.31.252:26670".to_string()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(90)))
        .build()
        .into()
}

/// 直连失败时：经 `ssh huhu` 本机 curl 127.0.0.1:26670（修 Win/代理拦 26670 痛点）
fn ssh_host() -> String {
    std::env::var("RXT_NEBULA_SSH").unwrap_or_else(|_| "huhu".to_string())
}

fn post_via_ssh(path: &str, payload: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let host = ssh_host();
    let body = serde_json::to_string(payload)?;
    let remote = format!(
        "curl -sS -m 90 -X POST 'http://127.0.0.1:26670{path}' -H 'Content-Type: application/json; charset=utf-8' --data-binary @-"
    );
    let out = std::process::Command::new("ssh")
        .arg(&host)
        .arg(remote)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(body.as_bytes())?;
            }
            child.wait_with_output()
        })?;
    if !out.status.success() {
        anyhow::bail!(
            "ssh {} curl 失败: {}",
            host,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&s).map_err(|e| {
        anyhow::anyhow!(
            "ssh JSON 解析失败: {} — {}",
            e,
            s.chars().take(200).collect::<String>()
        )
    })
}

fn get_via_ssh(path: &str) -> anyhow::Result<serde_json::Value> {
    let host = ssh_host();
    let remote = format!("curl -sS -m 30 'http://127.0.0.1:26670{path}'");
    let out = std::process::Command::new("ssh")
        .arg(&host)
        .arg(remote)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "ssh {} curl 失败: {}",
            host,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(serde_json::from_str(&s)?)
}

fn post_json(path: &str, payload: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", nebula_base(), path);
    match agent().post(&url).send_json(payload) {
        Ok(resp) => {
            let body = resp.into_body().read_to_string()?;
            serde_json::from_str(&body).map_err(|e| {
                anyhow::anyhow!(
                    "JSON 解析失败: {} — {}",
                    e,
                    body.chars().take(200).collect::<String>()
                )
            })
        }
        Err(e) => {
            // 直连失败（Win 透明代理/503/断连）→ ssh 跳板
            if std::env::var("RXT_MEM_NO_SSH").is_ok() {
                return Err(anyhow::anyhow!(
                    "星枢请求失败 {} — {}\n提示: 检查 RXT_NEBULA_URL 或设 RXT_NEBULA_SSH=huhu",
                    url,
                    e
                ));
            }
            post_via_ssh(path, payload).map_err(|e2| {
                anyhow::anyhow!(
                    "星枢直连失败 ({})；ssh 跳板也失败 ({})\n提示: RXT_NEBULA_URL / RXT_NEBULA_SSH / ssh huhu",
                    e,
                    e2
                )
            })
        }
    }
}

fn get_json(path: &str) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", nebula_base(), path);
    match agent().get(&url).call() {
        Ok(resp) => {
            let body = resp.into_body().read_to_string()?;
            Ok(serde_json::from_str(&body)?)
        }
        Err(e) => {
            if std::env::var("RXT_MEM_NO_SSH").is_ok() {
                return Err(anyhow::anyhow!("星枢 GET 失败 {} — {}", url, e));
            }
            get_via_ssh(path)
                .map_err(|e2| anyhow::anyhow!("星枢 GET 直连失败 ({})；ssh 跳板也失败 ({})", e, e2))
        }
    }
}

fn out(s: &str) {
    setup_utf8_console();
    let mut stdout = std::io::stdout().lock();
    let _ = crate::common::maybe_write_bom(&mut stdout);
    let _ = writeln!(stdout, "{}", s);
}

/// 保存记忆 → POST /memory/add（直连失败自动 ssh 跳板，与 search 一致）
pub fn run_save(content: &str, category: &str, importance: f64) -> anyhow::Result<()> {
    setup_utf8_console();
    let payload = serde_json::json!({
        "content": content,
        "category": category,
        "importance": importance,
        "source": "rxt-mem",
    });
    let data = post_json("/memory/add", &payload).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("400") || msg.to_lowercase().contains("secret") {
            anyhow::anyhow!("写入失败(可能密钥拦截): {}\n改用 Vaultwarden / bw-ai", msg)
        } else {
            anyhow::anyhow!("星枢写入失败: {}", msg)
        }
    })?;
    if data.get("status").and_then(|x| x.as_str()) == Some("rejected")
        || data.get("error").and_then(|x| x.as_str()) == Some("secret_in_content")
    {
        anyhow::bail!(
            "密钥明文被拒绝: {}\n请用 bw-ai / POST /secrets/store，勿把 key 写入星枢",
            data.get("message")
                .and_then(|x| x.as_str())
                .unwrap_or_else(|| "secret_in_content")
        );
    }
    out(&serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// 搜索 → POST /ask（省 token，打印 contract/pack）
pub fn run_search(query: &str, top_k: usize) -> anyhow::Result<()> {
    setup_utf8_console();
    let payload = serde_json::json!({
        "query": query,
        "top_k": top_k,
        "llm_deep": "off",
        "llm_answer": false,
    });
    let data = post_json("/ask", &payload)?;

    // 优先 contract
    if let Some(c) = data.get("contract").and_then(|v| v.as_str()) {
        if !c.is_empty() {
            out("=== contract ===");
            out(c);
            out("");
        }
    }
    if let Some(p) = data.get("pack").and_then(|v| v.as_str()) {
        if !p.is_empty() {
            out("=== pack ===");
            out(p);
            out("");
        }
    }

    let composed = data
        .get("composed")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let conf = composed
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let exec = composed.get("executable").and_then(|v| v.as_bool());
    let engine = data.get("engine").and_then(|v| v.as_str()).unwrap_or("?");
    let cache = data
        .get("result_cache_hit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let est = data
        .get("token_stats")
        .and_then(|t| t.get("pack_est_tokens").or_else(|| t.get("est_tokens")))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    out(&format!(
        "--- meta: engine={} conf={:.2} executable={:?} cache={} est_tok≈{} base={}",
        engine,
        conf,
        exec,
        cache,
        est,
        nebula_base()
    ));

    // 精简 results 一行摘要
    if let Some(arr) = data.get("results").and_then(|v| v.as_array()) {
        out(&format!("--- hits: {}", arr.len()));
        for (i, r) in arr.iter().take(top_k).enumerate() {
            let id = r
                .get("id")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into());
            let trust = r.get("trust").and_then(|v| v.as_str()).unwrap_or("?");
            let src = r
                .get("src")
                .or_else(|| r.get("source_file"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let one = content.replace('\n', " ");
            let short: String = one.chars().take(120).collect();
            out(&format!(
                "{}. [{}] #{} {} | {}",
                i + 1,
                trust,
                id,
                src,
                short
            ));
        }
    }
    Ok(())
}

/// 统计 → /v5/health + /stats 摘要
pub fn run_stats() -> anyhow::Result<()> {
    setup_utf8_console();
    let health = get_json("/v5/health").or_else(|_| get_json("/health"))?;
    out("=== /v5/health ===");
    out(&serde_json::to_string_pretty(&health)?);
    if let Ok(stats) = get_json("/stats") {
        out("\n=== /stats ===");
        // 只打关键字段，省 token
        let slim = serde_json::json!({
            "total_memories": stats.get("total_memories"),
            "categories": stats.get("categories"),
            "db_size": stats.get("db_size"),
            "result_cache": stats.get("result_cache"),
            "base": nebula_base(),
        });
        out(&serde_json::to_string_pretty(&slim)?);
    }
    Ok(())
}

/// 会话抽取 → /v5/session-extract
pub fn run_extract(transcript: &str, focus: &str, dry_run: bool) -> anyhow::Result<()> {
    setup_utf8_console();
    let payload = serde_json::json!({
        "transcript": transcript,
        "focus": focus,
        "auto_write": !dry_run,
        "dry_run": dry_run,
    });
    let data = post_json("/v5/session-extract", &payload)?;
    out(&serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// 开场注入 → /v5/bootstrap
pub fn run_bootstrap(focus: &str, budget: u32) -> anyhow::Result<()> {
    setup_utf8_console();
    let payload = serde_json::json!({
        "focus": focus,
        "budget_chars": budget,
    });
    let data = post_json("/v5/bootstrap", &payload)?;
    if let Some(b) = data.get("bootstrap").and_then(|v| v.as_str()) {
        out(b);
        out(&format!(
            "\n--- meta: chars={} est_tok≈{} base={}",
            data.get("chars").and_then(|v| v.as_u64()).unwrap_or(0),
            data.get("est_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            nebula_base()
        ));
    } else {
        out(&serde_json::to_string_pretty(&data)?);
    }
    Ok(())
}

/// 分层计划 → /v5/layered-recall
pub fn run_layers(focus: &str) -> anyhow::Result<()> {
    setup_utf8_console();
    let q = urlencoding::encode(focus);
    let data = get_json(&format!("/v5/layered-recall?focus={}", q))?;
    out(&serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// 用法（短，不灌 full 文档）
pub fn run_help() -> anyhow::Result<()> {
    setup_utf8_console();
    out(&format!(
        "rxt mem → 星枢 {}\n\
         search <q>     POST /ask（contract+pack）\n\
         save <text>    POST /memory/add（直连失败自动 ssh）\n\
         stats          /v5/health + /stats\n\
         bootstrap <f>  会话开场注入\n\
         extract <t>    会话抽取写回（--dry-run）\n\
         layers <f>     分层调用计划\n\
         help           本帮助\n\
         环境:\n\
           RXT_NEBULA_URL / NEBULA_URL  覆盖 API 地址\n\
           RXT_NEBULA_SSH / --host X    直连失败时的 ssh 跳板主机（默认 huhu）\n\
           RXT_MEM_NO_SSH=1            禁用 ssh 跳板\n\
           RXT_AGENT=1                 管道捕获时写 UTF-8 BOM（防 PS 乱码）\n\
         密钥: 明文禁入；用 bw-ai / :26670/secrets/*\n\
         例: rxt --host huhu mem search 网关",
        nebula_base()
    ));
    Ok(())
}
