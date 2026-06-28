//! HTTP 客户端 — 类似 curl,但为 LLM 优化
//!
//! - GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS
//! - 自动 JSON 格式化响应
//! - --headers 显示响应头
//! - --data / --json (-d/-j)
//! - --auth basic user:pass

use std::io::Read;

pub fn run(method: &str, url: &str, headers: &[String], data: Option<&str>, json_body: bool, auth: Option<&str>, show_headers: bool, body_only: bool) -> anyhow::Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("URL must start with http:// or https://");
    }

    let m_upper = method.to_uppercase();
    let auth_header = build_auth_header(auth);
    let body_data = data.unwrap_or("");
    let is_json = json_body;

    // Inline the request building — ureq 3.x has private generic types
    // that can't be named, so we use macros to chain operations.
    let result = match m_upper.as_str() {
        "GET" => {
            let mut r = ureq::get(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            r.call()
        }
        "DELETE" => {
            let mut r = ureq::delete(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            r.call()
        }
        "HEAD" => {
            let mut r = ureq::head(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            r.call()
        }
        "OPTIONS" => {
            let mut r = ureq::options(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            r.call()
        }
        "POST" => {
            let mut r = ureq::post(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            if is_json { r = r.header("Content-Type", "application/json"); } else { r = r.header("Content-Type", "application/x-www-form-urlencoded"); }
            r.send(body_data)
        }
        "PUT" => {
            let mut r = ureq::put(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            if is_json { r = r.header("Content-Type", "application/json"); } else { r = r.header("Content-Type", "application/x-www-form-urlencoded"); }
            r.send(body_data)
        }
        "PATCH" => {
            let mut r = ureq::patch(url);
            for h in headers { if let Some((k, v)) = h.split_once(':') { r = r.header(k.trim(), v.trim()); } else { anyhow::bail!("Invalid header: {}", h); } }
            if let Some(ref a) = auth_header { r = r.header("Authorization", a); }
            if is_json { r = r.header("Content-Type", "application/json"); } else { r = r.header("Content-Type", "application/x-www-form-urlencoded"); }
            r.send(body_data)
        }
        _ => anyhow::bail!("Unsupported HTTP method: {} (supported: GET POST PUT DELETE HEAD OPTIONS PATCH)", method),
    };

    match result {
        Ok(response) => {
            let status = response.status();
            let mut body = String::new();
            response.into_parts().1.into_reader().read_to_string(&mut body)?;

            if show_headers && !body_only {
                eprintln!("HTTP/1.1 {} OK", status);
            }

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
                if !body_only { println!("HTTP/1.1 {} (JSON)", status); }
                println!("{}", serde_json::to_string_pretty(&parsed)?);
            } else {
                if !body_only { println!("HTTP/1.1 {}", status); }
                print!("{}", body);
                if !body.ends_with('\n') { println!(); }
            }
            Ok(())
        }
        Err(e) => match &e {
            ureq::Error::StatusCode(s) => {
                eprintln!("HTTP/1.1 {} ERROR", s);
                anyhow::bail!("HTTP error: {}", s)
            }
            _ => anyhow::bail!("HTTP request failed: {}", e),
        },
    }
}

fn build_auth_header(auth: Option<&str>) -> Option<String> {
    auth.map(|basic| {
        if let Some((u, p)) = basic.split_once(':') {
            format!("Basic {}", base64_encode(format!("{}:{}", u, p).as_bytes()))
        } else {
            format!("Basic {}", base64_encode(basic.as_bytes()))
        }
    })
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8) | (data[i+2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}