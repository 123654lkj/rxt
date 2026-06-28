use std::path::Path;
use std::fs;
use serde_json::Value;

pub fn run(path: &Path, last: usize, json_output: bool) -> anyhow::Result<()> {
    let _ = fs::File::open(path)?;
    let raw_bytes = fs::read(path)?;
    let sig = crate::signature::FileSignature::detect(&raw_bytes);
    let text = crate::signature::to_utf8_lf(&raw_bytes, &sig);

    let mut exchanges: Vec<(String, String)> = Vec::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            let t = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if t == "event_msg" {
                if let Some(payload) = val.get("payload") {
                    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let text = payload.get("message")
                        .or_else(|| payload.get("text"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !text.is_empty() {
                        if msg_type == "user_message" {
                            exchanges.push((text.to_string(), String::new()));
                        } else if msg_type == "agent_message" {
                            if let Some(last_entry) = exchanges.last_mut() {
                                if last_entry.1.is_empty() {
                                    last_entry.1 = text.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let start = if exchanges.len() > last { exchanges.len() - last } else { 0 };
    let recent = &exchanges[start..];

    if json_output {
        println!("{}", serde_json::to_string_pretty(&recent.iter().map(|(u, a)| {
            serde_json::json!({"user": u, "assistant": a})
        }).collect::<Vec<_>>())?);
    } else {
        println!("=== Session: {} ===", path.file_name().unwrap_or_default().to_string_lossy());
        println!("Total exchanges: {}", exchanges.len());
        println!();
        for (i, (user, assistant)) in recent.iter().enumerate() {
            println!("--- [{}] ---", start + i);
            println!("USER: {}", truncate(user, 120));
            println!("AI:   {}", truncate(assistant, 120));
            println!();
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}