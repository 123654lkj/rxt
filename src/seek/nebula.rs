/// 星枢后端 — 调用 Nebula 记忆系统 API
///
/// index: POST /memory/add (逐条)
/// search: POST /search
/// clear: DELETE /memory/<id> (逐个删除)

use super::provider::{CodeChunk, IndexStats, SearchHit, SearchProvider};

pub struct NebulaProvider {
    base_url: String,
    client: ureq::Agent,
}

impl NebulaProvider {
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        let client = ureq::agent();
        // 健康检查
        let url = format!("{}/health", base_url.trim_end_matches('/'));
        match client.get(&url).call() {
            Ok(_) => {}
            Err(e) => {
                anyhow::bail!("星枢连接失败 ({}): {} — 确认虎虎开机且星枢服务运行中", url, e);
            }
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }
}

impl SearchProvider for NebulaProvider {
    fn name(&self) -> &str {
        "nebula"
    }

    fn index(&self, chunks: &[CodeChunk], project: &str) -> anyhow::Result<usize> {
        let url = format!("{}/memory/add", self.base_url);
        let mut success = 0;

        for chunk in chunks {
            // 构造星枢友好的 content 格式
            let content = format!(
                "[seek:{}] {} in {}\n{}",
                chunk.language, chunk.name, chunk.file,
                chunk.content
            );

            let payload = serde_json::json!({
                "content": content,
                "category": "code",
                "importance": 0.4,
                "tags": ["seek", project, chunk.language.as_str()]
            });

            match self.client.post(&url).send_json(&payload) {
                Ok(resp) => {
                    let body = resp.into_body().read_to_string().unwrap_or_default();
                    let data: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    if data.get("status").and_then(|v| v.as_str()) == Some("ok") {
                        success += 1;
                    } else if data.get("is_duplicate").and_then(|v| v.as_bool()) == Some(true) {
                        // 重复也算成功
                        success += 1;
                    }
                }
                Err(e) => {
                    eprintln!("  索引失败 {}: {}", chunk.name, e);
                }
            }
        }

        Ok(success)
    }

    fn search(
        &self,
        query: &str,
        top_k: usize,
        project: &str,
        language: Option<&str>,
    ) -> anyhow::Result<Vec<SearchHit>> {
        let url = format!("{}/search", self.base_url);

        let mut query_text = query.to_string();
        // 加上项目限定
        if !project.is_empty() {
            query_text = format!("{} (项目: {})", query, project);
        }

        let payload = serde_json::json!({
            "query": query_text,
            "top_k": top_k * 2, // 多取一些，后面过滤
            "category": "code",
            "rewrite": false,
            "use_hybrid": true,
            "enable_time_decay": false
        });

        let resp = self.client.post(&url)
            .send_json(&payload)?;
        let body = resp.into_body().read_to_string()?;
        let data: serde_json::Value = serde_json::from_str(&body)?;

        let results = data.get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("星枢搜索无 results 字段"))?;

        let mut hits = Vec::new();

        for r in results {
            let score = r.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");

            // 语言过滤
            if let Some(lang) = language {
                if !content.contains(&format!("[lang: {}]", lang))
                    && !content.contains(&format!("[seek:{}]", lang))
                {
                    continue;
                }
            }

            // 从 content 反解 file 和 line
            let chunk = parse_chunk_from_content(content);

            hits.push(SearchHit {
                chunk,
                score,
                provider: "nebula".to_string(),
            });

            if hits.len() >= top_k {
                break;
            }
        }

        Ok(hits)
    }

    fn clear(&self, _project: &str) -> anyhow::Result<()> {
        // 目前星枢没有按 project 批量删除的 API
        // 可以通过搜索 + 逐条删除实现，但先标记为 TODO
        anyhow::bail!("清除索引暂未实现（星枢缺少按 project 批量删除 API）")
    }

    fn stats(&self, project: &str) -> anyhow::Result<IndexStats> {
        let url = format!("{}/stats", self.base_url);
        let resp = self.client.get(&url).call()?;
        let body = resp.into_body().read_to_string()?;
        let data: serde_json::Value = serde_json::from_str(&body)?;

        let total = data.get("total_memories")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        Ok(IndexStats {
            provider: "nebula".to_string(),
            project: project.to_string(),
            total_chunks: total, // 注意: 这是总记忆数，不是该项目的
            total_files: 0,      // 星枢不提供此信息
            last_indexed: None,
        })
    }
}

/// 从星枢返回的 content 中解析出 CodeChunk
fn parse_chunk_from_content(content: &str) -> CodeChunk {
    let mut file = String::new();
    let mut language = String::new();
    let mut kind = String::new();
    let mut name = String::new();
    let mut line: usize = 0;

    // 解析头部标记: [file: xxx] [lang: xxx] [kind: xxx]
    // 或 [seek:xxx] name in file
    for line_text in content.lines().take(5) {
        // [seek:rust] fn run in src/main.rs
        if line_text.starts_with("[seek:") {
            if let Some(rest) = line_text.strip_prefix("[seek:") {
                if let Some(lang_end) = rest.find(']') {
                    language = rest[..lang_end].to_string();
                    let rest = &rest[lang_end + 1..];
                    // " fn run in src/main.rs"
                    if let Some(in_pos) = rest.find(" in ") {
                        name = rest[..in_pos].trim().to_string();
                        file = rest[in_pos + 4..].trim().to_string();
                        // 从 name 提取 kind
                        if let Some(sp) = name.find(' ') {
                            kind = name[..sp].to_string();
                        }
                    }
                }
            }
            break;
        }

        // [file: xxx] [lang: xxx] [kind: xxx]
        if line_text.contains("[file:") {
            file = extract_bracket_value(line_text, "file").unwrap_or_default();
            language = extract_bracket_value(line_text, "lang").unwrap_or_default();
            kind = extract_bracket_value(line_text, "kind").unwrap_or_default();
            break;
        }
    }

    // 尝试从签名行提取行号
    for line_text in content.lines() {
        if let Some(rest) = line_text.strip_prefix("// line ") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                line = n;
                break;
            }
        }
    }

    CodeChunk {
        file,
        line,
        end_line: 0,
        name,
        kind,
        language,
        content: content.to_string(),
        md5: String::new(),
    }
}

/// 从 `[key: value]` 格式中提取 value
fn extract_bracket_value(text: &str, key: &str) -> Option<String> {
    let pattern = format!("[{}: ", key);
    if let Some(start) = text.find(&pattern) {
        let rest = &text[start + pattern.len()..];
        if let Some(end) = rest.find(']') {
            return Some(rest[..end].to_string());
        }
    }
    None
}
