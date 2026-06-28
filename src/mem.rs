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
pub fn run_search(query: &str, top_k: usize) -> anyhow::Result<()> {
    let encoded_query = urlencoding::encode(query);
    let url = format!("{}/memory/search?q={}&top_k={}", STARK_HUB, encoded_query, top_k);

    let client = ureq::agent();
    let resp = client.get(&url).call()?;

    let body = resp.into_body().read_to_string()?;
    let data: serde_json::Value = serde_json::from_str(&body)?;

    // 尝试提取 results 数组
    let results = data.get("results").unwrap_or(&data);

    if let Some(arr) = results.as_array() {
        for r in arr {
            let cat = r.get("category").and_then(|v| v.as_str()).unwrap_or("");
            let imp = r.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
            println!("[{}]({:.1}) {}", cat, imp, content);
            println!();
        }
    } else {
        // 直接输出 JSON
        println!("{}", serde_json::to_string_pretty(&data)?);
    }

    Ok(())
}

/// 获取星枢统计信息
pub fn run_stats() -> anyhow::Result<()> {
    let url = format!("{}/stats", STARK_HUB);

    let client = ureq::agent();
    let resp = client.get(&url).call()?;

    let body = resp.into_body().read_to_string()?;
    let data: serde_json::Value = serde_json::from_str(&body)?;
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}
