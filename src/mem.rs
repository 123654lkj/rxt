use std::io::Write;

const STARK_HUB: &str = "http://127.0.0.1:26671";

/// 星枢记忆 — 保存/搜索跨会话记忆（纯 Rust 实现）

/// 保存记忆到星枢
pub fn run_save(content: &str, category: &str, importance: f64) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "content": content,
        "category": category,
        "importance": importance
    });

    let client = ureq::agent();
    let resp = client
        .post(&format!("{}/memory/extract", STARK_HUB))
        .send_json(&payload)?;

    let body = resp.into_body().read_to_string()?;
    println!("{}", body);
    Ok(())
}

/// 搜索星枢记忆
///
/// v0.7 修复: 星枢 daemon(nebula-memory, :26671) 的搜索端点是
///   POST /search  body={"query":..,"top_k":..}
/// 旧版误用 GET /memory/search?q= 会被当成 memory id → "Invalid id" 报错.
pub fn run_search(query: &str, top_k: usize) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "query": query,
        "top_k": top_k,
    });
    let url = format!("{}/search", STARK_HUB);
    let client = ureq::agent();
    let resp = client.post(&url).send_json(&payload)?;

    let body = resp.into_body().read_to_string()?;
    let data: serde_json::Value = serde_json::from_str(&body)?;

    // 提取 results 数组
    let results = data.get("results").unwrap_or(&data);

    if let Some(arr) = results.as_array() {
        if arr.is_empty() {
            println!("(无匹配记忆, query={:?})", query);
            return Ok(());
        }
        for r in arr {
            let cat = r.get("category").and_then(|v| v.as_str()).unwrap_or("");
            let imp = r.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let score = r.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!("[{}]({:.1} score={:.3}) {}", cat, imp, score, content);
            println!();
        }
    } else {
        // 直接输出 JSON
        println!("{}", serde_json::to_string_pretty(&data)?);
    }

    Ok(())
}

/// 获取星枢统计信息
///
/// v0.7 修复: 统计端点在底层 nebula 向量引擎(:26670)的 /stats,
/// 不是 daemon(:26671). daemon 只暴露 /health /search /memory/extract 等.
pub fn run_stats() -> anyhow::Result<()> {
    const NEBULA: &str = "http://127.0.0.1:26670";
    let url = format!("{}/stats", NEBULA);

    let client = ureq::agent();
    let resp = client.get(&url).call()?;

    let body = resp.into_body().read_to_string()?;
    let data: serde_json::Value = serde_json::from_str(&body)?;
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}
