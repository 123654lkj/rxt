//! HTTP 客户端 — CLI 访问网页、读数据、操作数据。不跑无头浏览器。
//!
//! - GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS
//! - 页面会话：`open` / `snap` / `read` / `fill` / `click` / `attr` / `submit`
//! - `forms` / `cli`：把 HTML 收成 CLI（表单+`<a>`）
//! - `scan`（别名 `apis`）：拉入口 JS，抽出 API，并探测哪个 host 真返回 JSON
//! - `session`：探测登录 Cookie 是否仍有效（过期 exit 2）
//! - `--form` 提交表单；`--browser` / `--cookie-jar` / `--cookie-json` 带登录态
//! - `--select` 抽 CSS 子集（h1 / #id / .class / [name=] / table）
//! - 环境变量回退：`RXT_COOKIE_JSON` / `RXT_COOKIE_JAR` / `RXT_BROWSER` / `RXT_HTTP_SESSION`
//! - Bearer/CSRF 只发给会话 origin 或 `--auth-host` / `RXT_HTTP_AUTH_HOSTS`
//! - 会话目录 0700，认证文件 0600；`rxt http purge` 覆写后删除
//! - `--text` 抽正文 / `--links` 抽链接 / `--budget` 截断 / `-o` 落盘
//! - 多个 URL 并行请求；`-j` 打包成一份 JSON

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[path = "http_browse.rs"]
mod browse;
#[path = "http_cdp.rs"]
mod cdp;

const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const MAX_BODY: usize = 32 * 1024 * 1024;

pub struct HttpOpts<'a> {
    pub method: &'a str,
    pub urls: &'a [String],
    pub headers: &'a [String],
    pub data: Option<&'a str>,
    pub json_body: bool,
    pub auth: Option<&'a str>,
    pub timeout: u64,
    pub show_headers: bool,
    pub body_only: bool,
    pub output: Option<&'a Path>,
    pub browser: Option<&'a str>,
    pub cookie_jar: Option<&'a Path>,
    pub cookies: &'a [String],
    pub user_agent: Option<&'a str>,
    pub text: bool,
    pub links: bool,
    pub budget: Option<usize>,
    pub form: &'a [String],
    pub no_probe: bool,
    pub cookie_json: Option<&'a str>,
    pub select: Option<&'a str>,
    pub session: Option<&'a str>,
    pub engine: Option<&'a str>,
    /// 允许自动附加 Bearer/CSRF 的 host（可重复）。环境变量 `RXT_HTTP_AUTH_HOSTS`。
    pub auth_hosts: &'a [String],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CookieRec {
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    expires: Option<u64>,
    name: String,
    value: String,
}

pub fn run(opts: HttpOpts<'_>) -> anyhow::Result<()> {
    if opts.method.eq_ignore_ascii_case("cookies") {
        return dump_cookies(&opts);
    }
    let (method, urls) = collect_urls(opts.method, opts.urls);
    if browse::is_page_cmd(&method) {
        return browse::run(&opts, &method, &urls);
    }
    let wrap = matches!(method.as_str(), "FORMS" | "CLI" | "WRAP" | "SCAN" | "APIS");
    let session_mode = method == "SESSION";
    let fetch_verb: &str = if wrap || session_mode {
        "GET"
    } else {
        method.as_str()
    };

    if urls.is_empty() {
        anyhow::bail!("需要 URL，例如: rxt http GET https://example.com https://example.org");
    }
    for u in &urls {
        if !is_http_url(u) {
            anyhow::bail!("URL 必须以 http:// 或 https:// 开头: {u}");
        }
    }
    if session_mode && urls.len() != 1 {
        anyhow::bail!("session 只支持一个 URL");
    }
    if urls.len() > 1 {
        return run_batch(&opts, &method, &urls, wrap, fetch_verb);
    }
    let url = urls[0].as_str();
    let ident = load_identity(&opts, url)?;
    let host = host_of(url).unwrap_or_default();
    let https = url.starts_with("https://");
    let cookie_header = cookie_header_for(&ident.cookies, &host, path_of(url), https);
    let merged = ident.cookies.clone();
    let cookie_jar: Option<&Path> = ident.jar.as_deref();
    ident.trace();

    let timeout = Duration::from_secs(opts.timeout.max(1));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let auth_header = build_auth_header(opts.auth);
    let form_body = encode_form_fields(opts.form);
    let body_owned: String;
    let body_data: &str = if !form_body.is_empty() {
        body_owned = form_body;
        &body_owned
    } else {
        opts.data.unwrap_or("")
    };
    let user_has_ua = has_header(opts.headers, "user-agent");
    let user_has_cookie = has_header(opts.headers, "cookie");
    let user_has_ct = has_header(opts.headers, "content-type");
    let user_has_referer = has_header(opts.headers, "referer");
    let user_has_auth = has_header(opts.headers, "authorization") || opts.auth.is_some();
    let ua = opts.user_agent.unwrap_or(DEFAULT_UA);
    let auto_referer = origin_of(url).map(|o| format!("{o}/"));
    let ident_headers = ident.headers.clone();

    macro_rules! paint {
        ($builder:expr) => {{
            let mut r = $builder;
            if !user_has_ua {
                r = r.header("User-Agent", ua);
            }
            if !user_has_cookie {
                if let Some(ref c) = cookie_header {
                    r = r.header("Cookie", c);
                }
            }
            if !user_has_referer {
                if let Some(ref rf) = auto_referer {
                    r = r.header("Referer", rf.as_str());
                }
            }
            if let Some(ref a) = auth_header {
                r = r.header("Authorization", a);
            } else if !user_has_auth {
                if let Some((_, v)) = ident_headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                {
                    r = r.header("Authorization", v);
                }
            }
            for (k, v) in &ident_headers {
                if k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("cookie") {
                    continue;
                }
                if has_header(opts.headers, k) {
                    continue;
                }
                r = r.header(k, v);
            }
            for h in opts.headers {
                let Some((k, v)) = h.split_once(':') else {
                    anyhow::bail!("无效 Header: {}", h);
                };
                r = r.header(k.trim(), v.trim());
            }
            r
        }};
    }

    let result = match fetch_verb {
        "GET" => paint!(agent.get(url)).call(),
        "DELETE" => paint!(agent.delete(url)).call(),
        "HEAD" => paint!(agent.head(url)).call(),
        "OPTIONS" => paint!(agent.options(url)).call(),
        "POST" | "PUT" | "PATCH" => {
            let mut r = match fetch_verb {
                "POST" => paint!(agent.post(url)),
                "PUT" => paint!(agent.put(url)),
                _ => paint!(agent.patch(url)),
            };
            if !user_has_ct {
                if opts.json_body && opts.form.is_empty() {
                    r = r.header("Content-Type", "application/json");
                } else {
                    r = r.header("Content-Type", "application/x-www-form-urlencoded");
                }
            }
            r.send(body_data)
        }
        _ => anyhow::bail!(
            "不支持的方法: {}（GET POST PUT DELETE HEAD OPTIONS PATCH；页面: open/snap/read/fill/click/attr/submit；列 Cookie 用 cookies；包装网页用 forms / cli / scan；探登录用 session）",
            opts.method
        ),
    };

    let response = result.map_err(|e| anyhow::anyhow!("HTTP 请求失败: {}", e))?;
    let status = response.status();
    let header_pairs: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut bytes = Vec::new();
    response
        .into_parts()
        .1
        .into_reader()
        .take(MAX_BODY as u64)
        .read_to_end(&mut bytes)?;

    if let Some(jar) = cookie_jar {
        let set_cookies: Vec<CookieRec> = header_pairs
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .filter_map(|(_, v)| parse_set_cookie(v, &host))
            .collect();
        let mut all = merged.clone();
        upsert_cookies(&mut all, &set_cookies);
        save_netscape(jar, &all)?;
        let _ = persist_login(&browse::session_dir(opts.session), &all, "set-cookie");
        eprintln!(
            "# cookie-jar {} 条 → {} (set-cookie {})",
            all.len(),
            jar.display(),
            set_cookies.len()
        );
    }

    if let Some(path) = opts.output {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, &bytes)?;
        eprintln!(
            "# 已写入 {} ({} bytes) HTTP {}",
            path.display(),
            bytes.len(),
            status
        );
        if opts.body_only && !opts.show_headers && !opts.text && !opts.links {
            return Ok(());
        }
    }

    if !opts.body_only {
        eprintln!("HTTP {}", status);
    }
    if let Some((_, v)) = header_pairs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("mt-gateway-error"))
    {
        eprintln!("# mt-gateway-error: {}", v);
    }
    if opts.show_headers && !opts.body_only {
        for (k, v) in &header_pairs {
            eprintln!("{}: {}", k, v);
        }
        eprintln!();
    }
    let _ = std::io::stderr().flush();

    let ctype = header_pairs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let looks_text = is_probably_text(ctype, &bytes);
    if !looks_text && opts.output.is_none() && !opts.text && !opts.links && !wrap {
        if session_mode {
            eprintln!("# session expired  二进制 type={ctype}");
            std::process::exit(2);
        }
        eprintln!("# 二进制 {} bytes  type={}", bytes.len(), ctype);
        return Ok(());
    }

    let raw = if looks_text {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        String::new()
    };

    if session_mode {
        let sent = cookie_sent_count(cookie_header.as_deref());
        return finish_session(
            sent,
            status.as_u16(),
            ctype,
            &header_pairs,
            &raw,
            opts.budget,
        );
    }

    if !wrap && looks_text && is_spa_shell(&raw) {
        eprintln!(
            "# SPA 壳（无 <form>）。下一步: rxt http scan {}{}",
            url,
            if cookie_jar.is_some()
                || opts.browser.is_some()
                || opts.cookie_json.is_some()
                || !opts.cookies.is_empty()
                || !ident.cookies.is_empty()
            {
                ""
            } else {
                "  （登录态 --cookie-json / --cookie-jar，或 RXT_COOKIE_JSON）"
            }
        );
    }

    if wrap {
        if matches!(method.as_str(), "SCAN" | "APIS") {
            print_page_scan(
                &agent,
                ua,
                &merged,
                url,
                &raw,
                opts.json_body,
                opts.budget,
                !opts.no_probe,
            )?;
        } else {
            print_page_cli(&method, url, &raw, opts.json_body, opts.budget)?;
        }
        return Ok(());
    }

    if opts.links {
        let found = extract_links(url, &raw);
        println!("# links {} (base={})", found.len(), url);
        for l in &found {
            println!("{}", l);
        }
        if !opts.text && opts.output.is_none() {
            return Ok(());
        }
        if opts.text {
            println!();
        }
    }

    let printable = if let Some(sel) = opts.select {
        let parts = browse::extract_select(&raw, sel);
        if parts.is_empty() {
            format!("# --select {sel}  无匹配")
        } else {
            parts.join("\n")
        }
    } else if opts.text {
        html_to_text(&raw)
    } else if looks_text {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
            serde_json::to_string_pretty(&parsed)?
        } else {
            raw
        }
    } else {
        format!("[binary {} bytes type={}]", bytes.len(), ctype)
    };

    let out = match opts.budget {
        Some(n) if printable.len() > n => {
            format!(
                "{}…\n# truncated {}/{} chars",
                &printable[..printable.floor_char_boundary(n)],
                n,
                printable.len()
            )
        }
        _ => printable,
    };
    print!("{}", out);
    if !out.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn is_http_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("http://") || s.starts_with("https://")
}

/// `rxt http https://a https://b` 时第一段会被 clap 当成 method。
fn collect_urls(method: &str, rest: &[String]) -> (String, Vec<String>) {
    if is_http_url(method) {
        let mut urls = Vec::with_capacity(rest.len() + 1);
        urls.push(method.to_string());
        urls.extend(rest.iter().cloned());
        ("GET".to_string(), urls)
    } else {
        (method.to_uppercase(), rest.to_vec())
    }
}

struct FetchOut {
    url: String,
    host: String,
    status: u16,
    header_pairs: Vec<(String, String)>,
    bytes: Vec<u8>,
    merged_cookies: Vec<CookieRec>,
    error: Option<String>,
}

fn run_batch(
    opts: &HttpOpts<'_>,
    method: &str,
    urls: &[String],
    wrap: bool,
    fetch_verb: &str,
) -> anyhow::Result<()> {
    eprintln!("# batch {} urls parallel", urls.len());
    let results = fetch_parallel(opts, fetch_verb, urls);

    if let Some(path) = opts.output {
        if path.exists() && !path.is_dir() {
            anyhow::bail!("多 URL 时 -o 必须是目录: {}", path.display());
        }
        std::fs::create_dir_all(path)?;
        for (i, item) in results.iter().enumerate() {
            let name = file_stem_for_url(i, &item.url);
            let dest = path.join(name);
            std::fs::write(&dest, &item.bytes)?;
            eprintln!(
                "# 已写入 {} ({} bytes) HTTP {}",
                dest.display(),
                item.bytes.len(),
                item.status
            );
        }
    }

    if let Some(jar) = opts.cookie_jar {
        let mut all: Vec<CookieRec> = Vec::new();
        for item in &results {
            upsert_cookies(&mut all, &item.merged_cookies);
        }
        save_netscape(jar, &all)?;
        eprintln!("# cookie-jar {} 条 → {}", all.len(), jar.display());
    }

    if wrap {
        let timeout = Duration::from_secs(opts.timeout.max(1));
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let ua = opts.user_agent.unwrap_or(DEFAULT_UA);
        for item in &results {
            eprintln!("# {}", item.url);
            if let Some(err) = &item.error {
                eprintln!("# error {err}");
                continue;
            }
            let raw = String::from_utf8_lossy(&item.bytes).into_owned();
            if matches!(method, "SCAN" | "APIS") {
                print_page_scan(
                    &agent,
                    ua,
                    &item.merged_cookies,
                    &item.url,
                    &raw,
                    opts.json_body,
                    opts.budget,
                    !opts.no_probe,
                )?;
            } else {
                print_page_cli(method, &item.url, &raw, opts.json_body, opts.budget)?;
            }
        }
        return batch_exit(&results);
    }

    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&batch_json(opts, &results))?
        );
        return batch_exit(&results);
    }

    for (i, item) in results.iter().enumerate() {
        print_batch_text(opts, i + 1, results.len(), item);
    }
    batch_exit(&results)
}

fn batch_exit(results: &[FetchOut]) -> anyhow::Result<()> {
    let fail = results.iter().filter(|r| r.error.is_some()).count();
    if fail == 0 {
        Ok(())
    } else {
        anyhow::bail!("批量 HTTP {fail}/{} 失败", results.len())
    }
}

fn fetch_parallel(opts: &HttpOpts<'_>, fetch_verb: &str, urls: &[String]) -> Vec<FetchOut> {
    let timeout = Duration::from_secs(opts.timeout.max(1));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let jar_cookies = opts
        .cookie_jar
        .and_then(|p| load_netscape(p).ok())
        .unwrap_or_default();
    let extra = parse_cookie_args(opts.cookies);
    urls.par_iter()
        .map(|url| fetch_one(opts, url, fetch_verb, &agent, &jar_cookies, &extra))
        .collect()
}

fn request_one(
    opts: &HttpOpts<'_>,
    url: &str,
    fetch_verb: &str,
    form: &[String],
    jar: Option<&Path>,
) -> FetchOut {
    let timeout = Duration::from_secs(opts.timeout.max(1));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let cookie_env = cookie_env();
    let jar_path = jar.or(opts.cookie_jar).or(cookie_env.jar.as_deref());
    let jar_cookies = jar_path
        .and_then(|p| load_netscape(p).ok())
        .unwrap_or_default();
    let extra = parse_cookie_args(opts.cookies);
    let out = match fetch_one_inner(opts, url, fetch_verb, &agent, &jar_cookies, &extra, form) {
        Ok(out) => out,
        Err(e) => {
            return FetchOut {
                url: url.to_string(),
                host: host_of(url).unwrap_or_default(),
                status: 0,
                header_pairs: Vec::new(),
                bytes: Vec::new(),
                merged_cookies: jar_cookies,
                error: Some(e.to_string()),
            };
        }
    };
    if let Some(p) = jar_path {
        let _ = save_netscape(p, &out.merged_cookies);
    }
    out
}

fn fetch_one(
    opts: &HttpOpts<'_>,
    url: &str,
    fetch_verb: &str,
    agent: &ureq::Agent,
    jar_cookies: &[CookieRec],
    extra: &[CookieRec],
) -> FetchOut {
    match fetch_one_inner(opts, url, fetch_verb, agent, jar_cookies, extra, opts.form) {
        Ok(out) => out,
        Err(e) => FetchOut {
            url: url.to_string(),
            host: host_of(url).unwrap_or_default(),
            status: 0,
            header_pairs: Vec::new(),
            bytes: Vec::new(),
            merged_cookies: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

fn fetch_one_inner(
    opts: &HttpOpts<'_>,
    url: &str,
    fetch_verb: &str,
    agent: &ureq::Agent,
    jar_cookies: &[CookieRec],
    extra: &[CookieRec],
    form: &[String],
) -> anyhow::Result<FetchOut> {
    let ident = load_identity(opts, url)?;
    let mut merged = ident.cookies.clone();
    upsert_cookies(&mut merged, jar_cookies);
    upsert_cookies(&mut merged, extra);
    let host = host_of(url).unwrap_or_default();
    let https = url.starts_with("https://");
    let cookie_header = cookie_header_for(&merged, &host, path_of(url), https);
    let ident_headers = ident.headers.clone();

    let auth_header = build_auth_header(opts.auth);
    let form_body = encode_form_fields(form);
    let body_data: &str = if !form_body.is_empty() {
        &form_body
    } else {
        opts.data.unwrap_or("")
    };
    let user_has_ua = has_header(opts.headers, "user-agent");
    let user_has_cookie = has_header(opts.headers, "cookie");
    let user_has_ct = has_header(opts.headers, "content-type");
    let user_has_referer = has_header(opts.headers, "referer");
    let user_has_auth = has_header(opts.headers, "authorization") || opts.auth.is_some();
    let ua = opts.user_agent.unwrap_or(DEFAULT_UA);
    let auto_referer = origin_of(url).map(|o| format!("{o}/"));

    macro_rules! paint {
        ($builder:expr) => {{
            let mut r = $builder;
            if !user_has_ua {
                r = r.header("User-Agent", ua);
            }
            if !user_has_cookie {
                if let Some(ref c) = cookie_header {
                    r = r.header("Cookie", c);
                }
            }
            if !user_has_referer {
                if let Some(ref rf) = auto_referer {
                    r = r.header("Referer", rf.as_str());
                }
            }
            if let Some(ref a) = auth_header {
                r = r.header("Authorization", a);
            } else if !user_has_auth {
                if let Some((_, v)) = ident_headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                {
                    r = r.header("Authorization", v);
                }
            }
            for (k, v) in &ident_headers {
                if k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("cookie") {
                    continue;
                }
                if has_header(opts.headers, k) {
                    continue;
                }
                r = r.header(k, v);
            }
            for h in opts.headers {
                let Some((k, v)) = h.split_once(':') else {
                    anyhow::bail!("无效 Header: {}", h);
                };
                r = r.header(k.trim(), v.trim());
            }
            r
        }};
    }

    let result = match fetch_verb {
        "GET" => paint!(agent.get(url)).call(),
        "DELETE" => paint!(agent.delete(url)).call(),
        "HEAD" => paint!(agent.head(url)).call(),
        "OPTIONS" => paint!(agent.options(url)).call(),
        "POST" | "PUT" | "PATCH" => {
            let mut r = match fetch_verb {
                "POST" => paint!(agent.post(url)),
                "PUT" => paint!(agent.put(url)),
                _ => paint!(agent.patch(url)),
            };
            if !user_has_ct {
                if opts.json_body && form.is_empty() {
                    r = r.header("Content-Type", "application/json");
                } else {
                    r = r.header("Content-Type", "application/x-www-form-urlencoded");
                }
            }
            r.send(body_data)
        }
        _ => anyhow::bail!(
            "不支持的方法: {}（GET POST PUT DELETE HEAD OPTIONS PATCH）",
            opts.method
        ),
    };

    let response = result.map_err(|e| anyhow::anyhow!("HTTP 请求失败: {}", e))?;
    let status = response.status();
    let header_pairs: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut bytes = Vec::new();
    response
        .into_parts()
        .1
        .into_reader()
        .take(MAX_BODY as u64)
        .read_to_end(&mut bytes)?;

    let set_cookies: Vec<CookieRec> = header_pairs
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, v)| parse_set_cookie(v, &host))
        .collect();
    let mut merged_out = merged;
    upsert_cookies(&mut merged_out, &set_cookies);

    Ok(FetchOut {
        url: url.to_string(),
        host,
        status: status.as_u16(),
        header_pairs,
        bytes,
        merged_cookies: merged_out,
        error: None,
    })
}

fn file_stem_for_url(i: usize, url: &str) -> String {
    let host = host_of(url).unwrap_or_else(|| format!("url{}", i + 1));
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{:02}-{safe}", i + 1)
}

fn batch_json(opts: &HttpOpts<'_>, results: &[FetchOut]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|item| {
            let ctype = content_type_of(&item.header_pairs);
            let looks_text = is_probably_text(ctype, &item.bytes);
            let mut body = if looks_text {
                let raw = String::from_utf8_lossy(&item.bytes).into_owned();
                if opts.text {
                    html_to_text(&raw)
                } else {
                    raw
                }
            } else {
                String::new()
            };
            if let Some(n) = opts.budget {
                if body.len() > n {
                    body = format!("{}…", &body[..body.floor_char_boundary(n)]);
                }
            }
            let links = if opts.links && looks_text {
                extract_links(&item.url, &String::from_utf8_lossy(&item.bytes))
            } else {
                Vec::new()
            };
            let mut obj = serde_json::json!({
                "url": item.url,
                "ok": item.error.is_none(),
                "status": item.status,
                "bytes": item.bytes.len(),
                "content_type": ctype,
                "binary": !looks_text && item.error.is_none(),
                "error": item.error,
            });
            if looks_text {
                obj["body"] = serde_json::Value::String(body);
            }
            if opts.links {
                obj["links"] = serde_json::json!(links);
            }
            if opts.show_headers {
                obj["headers"] = serde_json::json!(item.header_pairs);
            }
            obj
        })
        .collect();
    serde_json::json!({
        "count": results.len(),
        "ok": results.iter().filter(|r| r.error.is_none()).count(),
        "results": items,
    })
}

fn print_batch_text(opts: &HttpOpts<'_>, i: usize, n: usize, item: &FetchOut) {
    if let Some(err) = &item.error {
        eprintln!("# {i}/{n} FAIL {}  {err}", item.url);
        return;
    }
    if !opts.body_only {
        eprintln!(
            "# {i}/{n} HTTP {} {}  {} bytes",
            item.status,
            item.url,
            item.bytes.len()
        );
    }
    if opts.show_headers && !opts.body_only {
        for (k, v) in &item.header_pairs {
            eprintln!("{k}: {v}");
        }
        eprintln!();
    }
    let ctype = content_type_of(&item.header_pairs);
    let looks_text = is_probably_text(ctype, &item.bytes);
    if opts.links && looks_text {
        let raw = String::from_utf8_lossy(&item.bytes);
        let found = extract_links(&item.url, &raw);
        println!("# links {} (base={})", found.len(), item.url);
        for l in &found {
            println!("{l}");
        }
        if !opts.text && opts.output.is_none() {
            return;
        }
        if opts.text {
            println!();
        }
    }
    if opts.output.is_some() && opts.body_only && !opts.text && !opts.links {
        return;
    }
    let raw = if looks_text {
        String::from_utf8_lossy(&item.bytes).into_owned()
    } else {
        String::new()
    };
    let printable = if opts.text {
        html_to_text(&raw)
    } else if looks_text {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
            serde_json::to_string_pretty(&parsed).unwrap_or(raw)
        } else {
            raw
        }
    } else {
        format!("[binary {} bytes type={ctype}]", item.bytes.len())
    };
    let out = match opts.budget {
        Some(n) if printable.len() > n => format!(
            "{}…\n# truncated {}/{} chars",
            &printable[..printable.floor_char_boundary(n)],
            n,
            printable.len()
        ),
        _ => printable,
    };
    print!("{out}");
    if !out.ends_with('\n') {
        println!();
    }
}

fn content_type_of<'a>(headers: &'a [(String, String)]) -> &'a str {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

fn cookie_env() -> CookieEnv {
    CookieEnv {
        json: std::env::var("RXT_COOKIE_JSON")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        jar: std::env::var_os("RXT_COOKIE_JAR")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty()),
        browser: std::env::var("RXT_BROWSER")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}

struct CookieEnv {
    json: Option<String>,
    jar: Option<PathBuf>,
    browser: Option<String>,
}

fn cookie_sent_count(header: Option<&str>) -> usize {
    header
        .map(|s| s.split("; ").filter(|p| p.contains('=')).count())
        .unwrap_or(0)
}

#[derive(Debug, PartialEq, Eq)]
enum SessionVerdict {
    Anon,
    Expired(String),
    Ok(String),
}

fn session_verdict(
    sent_cookies: usize,
    status: u16,
    ctype: &str,
    headers: &[(String, String)],
    body: &str,
) -> SessionVerdict {
    if sent_cookies == 0 {
        return SessionVerdict::Anon;
    }
    if status == 401 || status == 403 {
        return SessionVerdict::Expired(format!("HTTP {status}"));
    }
    let hit = classify_probe(status, ctype, headers, body);
    if hit.auth {
        return SessionVerdict::Expired(hit.note);
    }
    if hit.kind == "html" {
        return SessionVerdict::Expired(
            "响应是 HTML/SPA，换登录探测 URL（如 /api/v1/accounts/token）".into(),
        );
    }
    SessionVerdict::Ok(hit.note)
}

fn finish_session(
    sent: usize,
    status: u16,
    ctype: &str,
    headers: &[(String, String)],
    body: &str,
    budget: Option<usize>,
) -> anyhow::Result<()> {
    match session_verdict(sent, status, ctype, headers, body) {
        SessionVerdict::Anon => {
            let hint = dirs::home_dir()
                .map(|h| h.join(".rxt").join("pos.json"))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| r"%USERPROFILE%\.rxt\pos.json".into());
            eprintln!("# session anon  没有可发送的 Cookie");
            eprintln!("# 浏览器只登录一次，Cookie-Editor 导出 JSON → {hint}");
            eprintln!("# setx RXT_COOKIE_JSON {hint}");
            eprintln!("# 之后 rxt http session / scan / GET，不必再开浏览器");
            std::process::exit(2);
        }
        SessionVerdict::Expired(reason) => {
            eprintln!("# session expired  {reason}");
            print_session_body(body, budget);
            std::process::exit(2);
        }
        SessionVerdict::Ok(note) => {
            eprintln!("# session ok  cookies={sent}  {note}");
            print_session_body(body, budget);
            Ok(())
        }
    }
}

fn print_session_body(body: &str, budget: Option<usize>) {
    let printable = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body.trim()) {
        serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };
    let out = match budget {
        Some(n) if printable.len() > n => format!(
            "{}…\n# truncated {}/{} chars",
            &printable[..printable.floor_char_boundary(n)],
            n,
            printable.len()
        ),
        _ => printable,
    };
    print!("{out}");
    if !out.ends_with('\n') {
        println!();
    }
}

fn dump_cookies(opts: &HttpOpts<'_>) -> anyhow::Result<()> {
    let env = cookie_env();
    let browser = opts.browser.or(env.browser.as_deref()).ok_or_else(|| {
        anyhow::anyhow!("列 Cookie 需要 --browser chrome|edge|firefox|brave|auto（或 RXT_BROWSER）")
    })?;
    let filter = opts.urls.first().map(|s| domain_from_input(s));
    let domains = filter.clone().map(|d| domain_candidates(&d));
    let (src, mut recs) = load_browser(browser, domains)?;
    if let Some(d) = &filter {
        recs.retain(|c| domain_matches(&c.domain, d));
    }

    if filter.is_none() {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for c in &recs {
            *counts.entry(c.domain.clone()).or_insert(0) += 1;
        }
        eprintln!(
            "# {} 共 {} 条 Cookie / {} 个域。查看值请加域名: rxt http cookies --browser {} github.com",
            src,
            recs.len(),
            counts.len(),
            src
        );
        if opts.json_body {
            println!("{}", serde_json::to_string_pretty(&counts)?);
        } else {
            for (d, n) in counts {
                println!("{}\t{}", n, d);
            }
        }
        return Ok(());
    }

    let sess = browse::session_dir(opts.session);
    persist_login(&sess, &recs, &src)?;
    let cookie_jar: Option<&Path> = opts.cookie_jar.or(env.jar.as_deref());
    if let Some(jar) = cookie_jar {
        if jar != sess.join("cookies.txt") {
            save_netscape(jar, &recs)?;
            eprintln!("# 已写入 cookie-jar {} ({} 条)", jar.display(), recs.len());
        }
    }
    eprintln!(
        "# 登录态已存 {} (cookies.txt / cookies.json / login.json)",
        sess.display()
    );

    eprintln!(
        "# {} {} 条 (domain={})",
        src,
        recs.len(),
        filter.as_deref().unwrap_or("")
    );
    if opts.json_body {
        println!("{}", serde_json::to_string_pretty(&recs)?);
    } else {
        println!("domain\tpath\tsecure\thttponly\tname\tvalue");
        for c in recs {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                c.domain, c.path, c.secure, c.http_only, c.name, c.value
            );
        }
    }
    Ok(())
}

const BROWSER_ALL: &[&str] = &[
    "chrome",
    "edge",
    "firefox",
    "brave",
    "chromium",
    "opera",
    "vivaldi",
    "arc",
    "zen",
    "librewolf",
    "opera-gx",
    "tabbit",
];

fn load_browser(
    name: &str,
    domains: Option<Vec<String>>,
) -> anyhow::Result<(String, Vec<CookieRec>)> {
    let raw = name.trim();
    let key = raw.to_ascii_lowercase().replace('_', "-");
    if Path::new(raw).is_dir() {
        let recs = load_chromium_dir(Path::new(raw), domains)?;
        return Ok((raw.to_string(), recs));
    }
    if key == "auto" {
        let mut errs = Vec::new();
        for cand in BROWSER_ALL {
            match load_browser(cand, domains.clone()) {
                Ok(pair) if !pair.1.is_empty() => return Ok(pair),
                Ok(_) => errs.push(format!("{cand}: 0 cookies")),
                Err(e) => errs.push(format!("{cand}: {e}")),
            }
        }
        anyhow::bail!(
            "auto 未读到浏览器 Cookie。{}\nChrome/Edge 127+ 常需管理员；Firefox / --cookie-json 更稳。",
            errs.join(" | ")
        );
    }
    if key == "all" {
        let mut recs = Vec::new();
        let mut srcs = Vec::new();
        for cand in BROWSER_ALL {
            match load_browser(cand, domains.clone()) {
                Ok((_, r)) if !r.is_empty() => {
                    srcs.push(format!("{cand}:{}", r.len()));
                    recs.extend(r);
                }
                _ => {}
            }
        }
        if recs.is_empty() {
            anyhow::bail!("all 未读到任何浏览器 Cookie");
        }
        return Ok((srcs.join(","), recs));
    }
    if key == "tabbit" || key == "tabbit-browser" {
        return load_tabbit(domains);
    }
    #[cfg(not(feature = "cookies"))]
    {
        return load_browser_python(&key, domains);
    }
    #[cfg(feature = "cookies")]
    {
        let recs = match key.as_str() {
            "chrome" => map_rookie(rookie::chrome(domains)),
            "edge" => map_rookie(rookie::edge(domains)),
            "firefox" => map_rookie(rookie::firefox(domains)),
            "brave" => map_rookie(rookie::brave(domains)),
            "chromium" => map_rookie(rookie::chromium(domains)),
            "opera" => map_rookie(rookie::opera(domains)),
            "vivaldi" => map_rookie(rookie::vivaldi(domains)),
            "arc" => map_rookie(rookie::arc(domains)),
            "zen" => map_rookie(rookie::zen(domains)),
            "librewolf" | "libre-wolf" => map_rookie(rookie::librewolf(domains)),
            "opera-gx" | "operagx" => map_rookie(rookie::opera_gx(domains)),
            "octo" | "octo-browser" => map_rookie(rookie::octo_browser(domains)),
            "cachy" => map_rookie(rookie::cachy(domains)),
            #[cfg(target_os = "macos")]
            "safari" => map_rookie(rookie::safari(domains)),
            #[cfg(target_os = "windows")]
            "ie" | "internet-explorer" => map_rookie(rookie::internet_explorer(domains)),
            other => anyhow::bail!(
                "未知浏览器: {other}\n支持: chrome|edge|firefox|brave|chromium|opera|vivaldi|arc|zen|librewolf|opera-gx|tabbit|all|auto，或 User Data 目录路径"
            ),
        }?;
        Ok((key, recs))
    }
}

fn tabbit_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            for n in ["Tabbit", "Tabbit Browser", "tabbit", "TabbitAI"] {
                v.push(PathBuf::from(&local).join(n).join("User Data"));
            }
        }
        if let Ok(roam) = std::env::var("APPDATA") {
            for n in ["Tabbit", "Tabbit Browser", "tabbit"] {
                v.push(PathBuf::from(&roam).join(n).join("User Data"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library").join("Application Support");
        for n in ["Tabbit", "Tabbit Browser", "tabbit"] {
            v.push(base.join(n));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let cfg = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"));
        for n in ["tabbit", "Tabbit", "tabbit-browser", "Tabbit Browser"] {
            v.push(cfg.join(n));
        }
    }
    let _ = home;
    v
}

fn chromium_cookie_pairs(user_data: &Path) -> Vec<(PathBuf, PathBuf)> {
    if !user_data.is_dir() {
        return Vec::new();
    }
    let local_state = user_data.join("Local State");
    let mut profiles = vec![user_data.join("Default")];
    if let Ok(rd) = std::fs::read_dir(user_data) {
        for e in rd.flatten() {
            let n = e.file_name();
            if n.to_string_lossy().starts_with("Profile ") {
                profiles.push(e.path());
            }
        }
    }
    let mut out = Vec::new();
    for p in profiles {
        for c in [p.join("Network").join("Cookies"), p.join("Cookies")] {
            if c.is_file() {
                out.push((local_state.clone(), c));
            }
        }
    }
    out
}

fn load_chromium_dir(
    user_data: &Path,
    domains: Option<Vec<String>>,
) -> anyhow::Result<Vec<CookieRec>> {
    let pairs = chromium_cookie_pairs(user_data);
    if pairs.is_empty() {
        anyhow::bail!(
            "不是 Chromium User Data（没找到 Cookies）: {}",
            user_data.display()
        );
    }
    #[cfg(feature = "cookies")]
    {
        let mut recs = Vec::new();
        let mut last = None;
        for (ls, db) in pairs {
            let db_s = db.to_string_lossy();
            let ls_s = ls.to_string_lossy();
            match map_rookie(rookie::any_browser(
                db_s.as_ref(),
                domains.clone(),
                Some(ls_s.as_ref()),
            )) {
                Ok(r) => recs.extend(r),
                Err(e) => last = Some(e),
            }
        }
        if recs.is_empty() {
            if let Some(e) = last {
                return Err(e);
            }
            anyhow::bail!("User Data 里 Cookie 为空: {}", user_data.display());
        }
        return Ok(recs);
    }
    #[cfg(not(feature = "cookies"))]
    {
        load_browser_python_dir(user_data, domains)
    }
}

fn load_tabbit(domains: Option<Vec<String>>) -> anyhow::Result<(String, Vec<CookieRec>)> {
    let mut last = "未找到 Tabbit User Data 目录".to_string();
    for root in tabbit_roots() {
        if !root.is_dir() {
            continue;
        }
        match load_chromium_dir(&root, domains.clone()) {
            Ok(recs) if !recs.is_empty() => return Ok(("tabbit".into(), recs)),
            Ok(_) => last = format!("{}: 0 cookies", root.display()),
            Err(e) => last = format!("{}: {e}", root.display()),
        }
    }
    anyhow::bail!(
        "Tabbit Cookie 读失败。{last}\n也可 --browser '/path/to/Tabbit/User Data' 或 Cookie-Editor 导出 --cookie-json"
    )
}

#[cfg(feature = "cookies")]
fn map_rookie(
    res: rookie::Result<Vec<rookie::common::enums::Cookie>>,
) -> anyhow::Result<Vec<CookieRec>> {
    let cookies = res.map_err(|e| {
        anyhow::anyhow!(
            "{e}\n提示: Chrome/Edge 127+ 用了 App-Bound Encryption，常需管理员权限；Firefox 通常可直接读。"
        )
    })?;
    Ok(cookies
        .into_iter()
        .map(|c| CookieRec {
            domain: c.domain,
            path: if c.path.is_empty() {
                "/".into()
            } else {
                c.path
            },
            secure: c.secure,
            http_only: c.http_only,
            expires: c.expires,
            name: c.name,
            value: c.value,
        })
        .collect())
}

#[cfg(not(feature = "cookies"))]
fn load_browser_python(
    name: &str,
    domains: Option<Vec<String>>,
) -> anyhow::Result<(String, Vec<CookieRec>)> {
    let py = python_with_rookiepy()?;
    let mut cmd = Command::new(&py);
    if py.eq_ignore_ascii_case("py") {
        cmd.arg("-3");
    }
    cmd.arg("-")
        .env("RXT_BROWSER_NAME", name)
        .env("RXT_COOKIE_DOMAINS", domains.unwrap_or_default().join(","))
        .env("RXT_CHROMIUM_DIR", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("启动 Python 失败: {e}"))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("无法写入 Python stdin"))?;
        stdin.write_all(include_bytes!("http_rookie.py"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| anyhow::anyhow!("等待 Python 失败: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        anyhow::bail!("{}", browser_cookie_error(name, stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let recs = parse_cookie_json(stdout.trim(), "")?;
    Ok((name.trim().to_ascii_lowercase(), recs))
}

#[cfg(not(feature = "cookies"))]
fn load_browser_python_dir(
    dir: &Path,
    domains: Option<Vec<String>>,
) -> anyhow::Result<Vec<CookieRec>> {
    let py = python_with_rookiepy()?;
    let mut cmd = Command::new(&py);
    if py.eq_ignore_ascii_case("py") {
        cmd.arg("-3");
    }
    cmd.arg("-")
        .env("RXT_BROWSER_NAME", "chromium-dir")
        .env("RXT_COOKIE_DOMAINS", domains.unwrap_or_default().join(","))
        .env("RXT_CHROMIUM_DIR", dir.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("启动 Python 失败: {e}"))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("无法写入 Python stdin"))?;
        stdin.write_all(include_bytes!("http_rookie.py"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| anyhow::anyhow!("等待 Python 失败: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    parse_cookie_json(String::from_utf8_lossy(&out.stdout).trim(), "")
}

#[cfg(not(feature = "cookies"))]
fn python_with_rookiepy() -> anyhow::Result<String> {
    for bin in ["python", "py"] {
        let mut c = Command::new(bin);
        if bin == "py" {
            c.arg("-3");
        }
        c.arg("-c").arg("import rookiepy");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        if c.status().map(|s| s.success()).unwrap_or(false) {
            return Ok(bin.to_string());
        }
    }
    anyhow::bail!(
        "本二进制未启用 cookies feature，且未找到带 rookiepy 的 Python。\n\
         pip install rookiepy   或   cargo build --release --features cookies\n\
         也可 --cookie-json / --cookie-jar（浏览器扩展导出，不读 Chrome v20 密文）。"
    )
}

#[cfg(not(feature = "cookies"))]
fn browser_cookie_error(browser: &str, err: &str) -> String {
    let abe = err.contains("CryptUnprotectData")
        || err.contains("null pointer")
        || err.contains("app_bound")
        || err.contains("App-Bound");
    if abe {
        format!(
            "{err}\n\
             Chrome/Edge 127+ 的 Cookie 值是 v20 App-Bound，读磁盘不解值。\n\
             GitHub 上 rookie / yt-dlp --cookies-from-browser 就是这条路（不打开窗口）；\n\
             注入/掏空 chrome.exe 调 IElevator 的项目是 infostealer，rxt 不做。\n\
             可改：管理员终端再跑 rxt http cookies --browser {browser}；\n\
             或 Cookie-Editor 导出 JSON → --cookie-json / RXT_COOKIE_JSON。"
        )
    } else {
        err.to_string()
    }
}

fn parse_cookie_args(args: &[String]) -> Vec<CookieRec> {
    let mut out = Vec::new();
    for a in args {
        for part in a.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            out.push(CookieRec {
                domain: String::new(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            });
        }
    }
    out
}

fn merge_cookies(jar: &[CookieRec], browser: &[CookieRec], extra: &[CookieRec]) -> Vec<CookieRec> {
    let mut all = jar.to_vec();
    upsert_cookies(&mut all, browser);
    upsert_cookies(&mut all, extra);
    all
}

fn upsert_cookies(all: &mut Vec<CookieRec>, incoming: &[CookieRec]) {
    for c in incoming {
        if let Some(old) = all
            .iter_mut()
            .find(|o| o.name == c.name && o.domain == c.domain && o.path == c.path)
        {
            *old = c.clone();
        } else {
            all.push(c.clone());
        }
    }
}

fn cookie_header_for(cookies: &[CookieRec], host: &str, path: &str, https: bool) -> Option<String> {
    let mut parts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in cookies {
        if !cookie_applies(c, host, path, https) {
            continue;
        }
        if !seen.insert(c.name.clone()) {
            continue;
        }
        parts.push(format!("{}={}", c.name, c.value));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn load_netscape(path: &Path) -> anyhow::Result<Vec<CookieRec>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) {
            continue;
        }
        let http_only = line.starts_with("#HttpOnly_");
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        let expires = cols[4].parse::<u64>().ok().filter(|&n| n > 0);
        out.push(CookieRec {
            domain: cols[0].to_string(),
            path: cols[2].to_string(),
            secure: cols[3].eq_ignore_ascii_case("true"),
            http_only,
            expires,
            name: cols[5].to_string(),
            value: cols[6].to_string(),
        });
    }
    Ok(out)
}

fn save_netscape(path: &Path, cookies: &[CookieRec]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            secure_mkdir(parent)?;
        }
    }
    let mut buf = String::new();
    buf.push_str("# Netscape HTTP Cookie File\n");
    buf.push_str("# written by rxt http\n");
    for c in cookies {
        let flag = if c.domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let domain = if c.http_only {
            format!("#HttpOnly_{}", c.domain)
        } else {
            c.domain.clone()
        };
        buf.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            domain,
            flag,
            if c.path.is_empty() { "/" } else { &c.path },
            if c.secure { "TRUE" } else { "FALSE" },
            c.expires.unwrap_or(0),
            c.name,
            c.value
        ));
    }
    secure_write(path, buf.as_bytes())
}

fn cookies_to_devtools(recs: &[CookieRec]) -> serde_json::Value {
    serde_json::Value::Array(
        recs.iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "value": c.value,
                    "domain": c.domain,
                    "path": if c.path.is_empty() { "/" } else { &c.path },
                    "secure": c.secure,
                    "httpOnly": c.http_only,
                    "expirationDate": c.expires,
                })
            })
            .collect(),
    )
}

pub(super) fn persist_login(dir: &Path, recs: &[CookieRec], src: &str) -> anyhow::Result<()> {
    secure_mkdir(dir)?;
    save_netscape(&dir.join("cookies.txt"), recs)?;
    secure_write(
        dir.join("cookies.json"),
        serde_json::to_vec_pretty(&cookies_to_devtools(recs))?,
    )?;
    let mut domains: BTreeMap<String, usize> = BTreeMap::new();
    for c in recs {
        *domains.entry(c.domain.clone()).or_insert(0) += 1;
        record_host_in(dir, &c.domain);
    }
    secure_write(
        dir.join("login.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "imported_from": src,
            "saved_at": now_unix(),
            "cookies": recs.len(),
            "domains": domains,
        }))?,
    )?;
    Ok(())
}

const AUTH_FILES: &[&str] = &[
    "cookies.txt",
    "cookies.json",
    "storage.json",
    "headers.json",
    "login.json",
    "origin.json",
    "hold.json",
    "engine.json",
];

const SESSION_JUNK: &[&str] = &[
    "page.html",
    "meta.json",
    "refs.json",
    "net.json",
    "draft.json",
];

fn chmod_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    let _ = (path, mode);
}

pub(super) fn secure_mkdir(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    chmod_mode(dir, 0o700);
    if let Some(parent) = dir.parent() {
        if parent.file_name().and_then(|s| s.to_str()) == Some("http-session") {
            chmod_mode(parent, 0o700);
        }
    }
    Ok(())
}

pub(super) fn secure_write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            secure_mkdir(parent)?;
        }
    }
    std::fs::write(path, data.as_ref())?;
    chmod_mode(path, 0o600);
    Ok(())
}

fn secure_wipe(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.is_file() {
            let n = (meta.len() as usize).min(8 * 1024 * 1024);
            let _ = std::fs::write(path, vec![0u8; n]);
        }
    }
    let _ = std::fs::remove_file(path);
}

pub(super) fn purge_session(dir: &Path) -> anyhow::Result<()> {
    let _ = cdp::hold_quit(dir);
    for name in AUTH_FILES.iter().chain(SESSION_JUNK.iter()) {
        secure_wipe(&dir.join(name));
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                secure_wipe(&p);
            }
        }
    }
    let _ = std::fs::remove_dir(dir);
    eprintln!("# purged {}", dir.display());
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct OriginBind {
    #[serde(default)]
    origins: Vec<String>,
    #[serde(default)]
    hosts: Vec<String>,
}

fn origin_bind_path(dir: &Path) -> PathBuf {
    dir.join("origin.json")
}

fn load_origin_bind(dir: &Path) -> OriginBind {
    std::fs::read_to_string(origin_bind_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_origin_bind(dir: &Path, b: &OriginBind) {
    let _ = (|| -> anyhow::Result<()> {
        secure_mkdir(dir)?;
        secure_write(origin_bind_path(dir), serde_json::to_vec_pretty(b)?)?;
        Ok(())
    })();
}

fn record_host_in(dir: &Path, host: &str) {
    let h = host.trim_start_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return;
    }
    let mut b = load_origin_bind(dir);
    if !b.hosts.iter().any(|x| x == &h) {
        b.hosts.push(h);
        save_origin_bind(dir, &b);
    }
}

pub(super) fn record_origin(dir: &Path, url: &str) -> anyhow::Result<()> {
    // 只记当前页 origin。同会话先后打开 A 再开 B 时，storage 属于 B，不能把 B 的 token 发给 A。
    let mut b = load_origin_bind(dir);
    if let Some(o) = origin_of(url) {
        b.origins = vec![o];
    }
    if let Some(h) = host_of(url) {
        b.hosts = vec![h];
    }
    secure_mkdir(dir)?;
    secure_write(origin_bind_path(dir), serde_json::to_vec_pretty(&b)?)?;
    Ok(())
}

fn extra_auth_hosts(opts: &HttpOpts<'_>) -> Vec<String> {
    let mut v: Vec<String> = opts.auth_hosts.iter().cloned().collect();
    for env in ["RXT_HTTP_AUTH_HOSTS", "RXT_BEARER_HOST"] {
        if let Ok(s) = std::env::var(env) {
            for p in s.split([',', ' ', ';']) {
                let p = p.trim();
                if !p.is_empty() {
                    v.push(p.to_string());
                }
            }
        }
    }
    v
}

fn host_allowed(host: &str, allow: &str) -> bool {
    let h = host.trim_start_matches('.').to_ascii_lowercase();
    let mut a = allow.trim().to_ascii_lowercase();
    for pfx in ["https://", "http://"] {
        if let Some(rest) = a.strip_prefix(pfx) {
            a = rest.to_string();
            break;
        }
    }
    let a = a
        .split(['/', ':', '?'])
        .next()
        .unwrap_or("")
        .trim_start_matches('.');
    if a.is_empty() {
        return false;
    }
    h == a || h.ends_with(&format!(".{a}"))
}

/// Bearer/CSRF（来自 storage/headers.json/RXT_BEARER）仅同源或 allowlist。
fn auth_ok_for_url(opts: &HttpOpts<'_>, dir: &Path, url: &str) -> bool {
    let host = host_of(url).unwrap_or_default();
    if extra_auth_hosts(opts)
        .iter()
        .any(|a| host_allowed(&host, a))
    {
        return true;
    }
    let b = load_origin_bind(dir);
    if let Some(o) = origin_of(url) {
        if b.origins.iter().any(|r| r.eq_ignore_ascii_case(&o)) {
            return true;
        }
    }
    false
}

fn cookie_applies(c: &CookieRec, host: &str, path: &str, https: bool) -> bool {
    if c.name.is_empty() {
        return false;
    }
    if let Some(exp) = c.expires {
        if exp > 0 && exp < now_unix() {
            return false;
        }
    }
    if c.secure && !https {
        return false;
    }
    if !c.domain.is_empty() && !domain_matches(&c.domain, host) {
        return false;
    }
    path_matches(&c.path, path)
}

pub(super) fn gather_cookies(
    opts: &HttpOpts<'_>,
    host_hint: Option<&str>,
) -> anyhow::Result<(String, Vec<CookieRec>)> {
    let mut src = "session".to_string();
    let mut recs: Vec<CookieRec> = Vec::new();
    let env = cookie_env();
    let browser = opts.browser.or(env.browser.as_deref());
    if let Some(b) = browser {
        let domains = host_hint.map(|h| domain_candidates(h));
        match load_browser(b, domains) {
            Ok((s, r)) => {
                src = s;
                recs.extend(r);
            }
            Err(e) => eprintln!("# 浏览器 Cookie 跳过: {e}"),
        }
    }
    let cookie_json = opts.cookie_json.or(env.json.as_deref());
    if let Some(raw) = cookie_json {
        recs.extend(load_cookie_json(raw, host_hint.unwrap_or(""))?);
        if src == "session" {
            src = "cookie-json".into();
        }
    }
    let extra = parse_cookie_args(opts.cookies);
    if !extra.is_empty() {
        recs.extend(extra);
        if src == "session" {
            src = "cookie".into();
        }
    }
    Ok((src, recs))
}

struct Identity {
    cookies: Vec<CookieRec>,
    headers: Vec<(String, String)>,
    jar: Option<PathBuf>,
    notes: Vec<String>,
}

impl Identity {
    fn trace(&self) {
        if self.cookies.is_empty() && self.headers.is_empty() {
            return;
        }
        let bearer = self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"));
        let csrf = self.headers.iter().any(|(k, _)| {
            let l = k.to_ascii_lowercase();
            l.contains("csrf") || l.contains("xsrf")
        });
        eprintln!(
            "# identity cookies={} bearer={} csrf={} {}",
            self.cookies.len(),
            if bearer { "yes" } else { "no" },
            if csrf { "yes" } else { "no" },
            self.notes.join(" ")
        );
    }
}

pub(super) fn load_identity(opts: &HttpOpts<'_>, url: &str) -> anyhow::Result<Identity> {
    let dir = browse::session_dir(opts.session);
    let host = host_of(url).unwrap_or_default();
    let env = cookie_env();
    let jar = opts
        .cookie_jar
        .map(|p| p.to_path_buf())
        .or(env.jar.clone())
        .unwrap_or_else(|| dir.join("cookies.txt"));
    let mut recs = if jar.exists() {
        load_netscape(&jar).unwrap_or_default()
    } else {
        Vec::new()
    };
    let sess_json = dir.join("cookies.json");
    if sess_json.exists() {
        if let Ok(more) = load_cookie_json(&sess_json.to_string_lossy(), &host) {
            upsert_cookies(&mut recs, &more);
        }
    }
    let host_hint = if host.is_empty() {
        None
    } else {
        Some(host.as_str())
    };
    let (src, more) = gather_cookies(opts, host_hint)?;
    upsert_cookies(&mut recs, &more);
    let mut notes = Vec::new();
    if !src.is_empty() && src != "session" {
        notes.push(format!("from={src}"));
    }
    let storage_ok = auth_ok_for_url(opts, &dir, url);
    if !storage_ok {
        let bind = load_origin_bind(&dir);
        if !bind.origins.is_empty() {
            notes.push(format!("auth-skip origin={}", bind.origins.join(",")));
        } else {
            notes.push("auth-skip no-origin".into());
        }
    }
    let mut headers = sso_headers_from_session(&dir, &recs, url, storage_ok);
    if storage_ok {
        if let Ok(raw) = std::fs::read_to_string(dir.join("headers.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                let obj = v
                    .get("headers")
                    .and_then(|h| h.as_object())
                    .or_else(|| v.as_object());
                if let Some(obj) = obj {
                    for (k, val) in obj {
                        if k == "headers" {
                            continue;
                        }
                        if let Some(s) = val.as_str() {
                            headers.push((k.clone(), s.to_string()));
                        }
                    }
                }
            }
        }
        if let Ok(b) = std::env::var("RXT_BEARER") {
            let b = b.trim();
            if !b.is_empty()
                && !headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            {
                let v = if b.to_ascii_lowercase().starts_with("bearer ") {
                    b.to_string()
                } else {
                    format!("Bearer {b}")
                };
                headers.push(("Authorization".into(), v));
                notes.push("env=RXT_BEARER".into());
            }
        }
    }
    Ok(Identity {
        cookies: recs,
        headers,
        jar: Some(jar),
        notes,
    })
}

fn sso_headers_from_session(
    dir: &Path,
    cookies: &[CookieRec],
    url: &str,
    storage_ok: bool,
) -> Vec<(String, String)> {
    let host = host_of(url).unwrap_or_default();
    let path = path_of(url);
    let https = url.starts_with("https://");
    let matching: Vec<CookieRec> = cookies
        .iter()
        .filter(|c| cookie_applies(c, &host, path, https))
        .cloned()
        .collect();
    let mut headers = Vec::new();
    let token = if storage_ok {
        token_from_storage(dir)
    } else {
        None
    }
    .or_else(|| token_from_cookies(&matching));
    if let Some(t) = token {
        let v = if t.to_ascii_lowercase().starts_with("bearer ") {
            t
        } else {
            format!("Bearer {t}")
        };
        headers.push(("Authorization".into(), v));
    }
    let csrf = if storage_ok {
        csrf_from_storage(dir)
    } else {
        None
    }
    .or_else(|| csrf_from_cookies(&matching));
    if let Some(csrf) = csrf {
        headers.push(("X-CSRF-Token".into(), csrf.clone()));
        headers.push(("X-XSRF-TOKEN".into(), csrf));
    }
    headers
}

fn token_from_storage(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("storage.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut found = Vec::new();
    for bag in ["local", "session"] {
        let Some(obj) = v.get(bag).and_then(|x| x.as_object()) else {
            continue;
        };
        for (k, val) in obj {
            let s = val
                .as_str()
                .map(|x| x.to_string())
                .unwrap_or_else(|| val.to_string());
            if let Some(t) = extract_token_value(k, &s) {
                found.push((k.to_ascii_lowercase(), t));
            }
        }
    }
    pick_best_token(found)
}

fn token_from_cookies(cookies: &[CookieRec]) -> Option<String> {
    let mut found = Vec::new();
    for c in cookies {
        if let Some(t) = extract_token_value(&c.name, &c.value) {
            found.push((c.name.to_ascii_lowercase(), t));
        }
    }
    pick_best_token(found)
}

fn pick_best_token(mut found: Vec<(String, String)>) -> Option<String> {
    if found.is_empty() {
        return None;
    }
    found.sort_by(|a, b| token_score(&b.0).cmp(&token_score(&a.0)));
    Some(found.into_iter().next().unwrap().1)
}

fn token_score(key: &str) -> i32 {
    if key.contains("access") {
        80
    } else if key.contains("id_token") || key.contains("idtoken") {
        70
    } else if key.contains("jwt") {
        60
    } else if key.contains("login") {
        50
    } else if key.contains("auth") || key.contains("token") || key.contains("sso") {
        40
    } else {
        0
    }
}

fn extract_token_value(key: &str, val: &str) -> Option<String> {
    let k = key.to_ascii_lowercase();
    let hit = [
        "token",
        "jwt",
        "access",
        "id_token",
        "authorization",
        "auth",
        "sso",
        "login",
        "sessionid",
        "sid",
    ]
    .iter()
    .any(|t| k.contains(t));
    if !hit {
        return None;
    }
    let v = val.trim();
    if v.is_empty() || v == "null" || v == "undefined" {
        return None;
    }
    if v.starts_with('{') {
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(v) {
            for kk in [
                "access_token",
                "id_token",
                "token",
                "accessToken",
                "idToken",
                "jwt",
            ] {
                if let Some(s) = j.get(kk).and_then(|x| x.as_str()) {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    if k.contains("sid") || k.contains("session") {
        return None; // 会话 cookie 走 Cookie 头，不当 Bearer
    }
    if v.len() < 12 && !looks_like_jwt(v) {
        return None;
    }
    Some(
        v.trim_start_matches("Bearer ")
            .trim_start_matches("bearer ")
            .to_string(),
    )
}

fn looks_like_jwt(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("eyJ") && s.bytes().filter(|b| *b == b'.').count() >= 2
}

fn csrf_from_cookies(cookies: &[CookieRec]) -> Option<String> {
    cookies.iter().find_map(|c| {
        let n = c.name.to_ascii_lowercase();
        if n.contains("csrf") || n.contains("xsrf") {
            Some(c.value.clone())
        } else {
            None
        }
    })
}

fn csrf_from_storage(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("storage.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    for bag in ["local", "session"] {
        let Some(obj) = v.get(bag).and_then(|x| x.as_object()) else {
            continue;
        };
        for (k, val) in obj {
            let n = k.to_ascii_lowercase();
            if n.contains("csrf") || n.contains("xsrf") {
                if let Some(s) = val.as_str() {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

pub(super) fn print_identity(opts: &HttpOpts<'_>, url: Option<&str>) -> anyhow::Result<()> {
    let dir = browse::session_dir(opts.session);
    let bind = load_origin_bind(&dir);
    let dummy = url.unwrap_or("https://localhost/");
    let ident = load_identity(opts, dummy)?;
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "dir": dir.display().to_string(),
                "origins": bind.origins,
                "hosts": bind.hosts,
                "cookies": ident.cookies.len(),
                "headers": ident.headers.iter().map(|(k,v)| serde_json::json!({"name": k, "value": mask_secret(v)})).collect::<Vec<_>>(),
                "jar": ident.jar.as_ref().map(|p| p.display().to_string()),
                "notes": ident.notes,
            }))?
        );
        return Ok(());
    }
    println!("session {}", dir.display());
    if !bind.origins.is_empty() {
        println!("origins {}", bind.origins.join(" "));
    }
    println!("cookies {}", ident.cookies.len());
    let mut domains: BTreeMap<String, usize> = BTreeMap::new();
    for c in &ident.cookies {
        *domains.entry(c.domain.clone()).or_insert(0) += 1;
    }
    for (d, n) in domains {
        println!("  {n}\t{d}");
    }
    for (k, v) in &ident.headers {
        println!("{k}: {}", mask_secret(v));
    }
    if ident.cookies.is_empty() && ident.headers.is_empty() {
        println!("(空。先 rxt http import --browser firefox  或  open 登录页)");
    }
    Ok(())
}

fn mask_secret(s: &str) -> String {
    let s = s.trim();
    if s.len() <= 12 {
        return "***".into();
    }
    format!("{}…{}", &s[..8], s.len())
}

fn parse_set_cookie(header: &str, host: &str) -> Option<CookieRec> {
    let mut parts = header.split(';').map(|s| s.trim());
    let nv = parts.next()?;
    let (name, value) = nv.split_once('=')?;
    let mut rec = CookieRec {
        domain: host.to_string(),
        path: "/".into(),
        secure: false,
        http_only: false,
        expires: None,
        name: name.trim().to_string(),
        value: value.trim().to_string(),
    };
    for p in parts {
        let (k, v) = p
            .split_once('=')
            .map(|(a, b)| (a.trim(), b.trim()))
            .unwrap_or((p, ""));
        match k.to_ascii_lowercase().as_str() {
            "domain" => {
                rec.domain = if v.starts_with('.') {
                    v.to_string()
                } else {
                    format!(".{v}")
                };
            }
            "path" => {
                rec.path = if v.is_empty() {
                    "/".into()
                } else {
                    v.to_string()
                }
            }
            "secure" => rec.secure = true,
            "httponly" => rec.http_only = true,
            "max-age" => {
                if let Ok(secs) = v.parse::<u64>() {
                    rec.expires = Some(now_unix().saturating_add(secs));
                }
            }
            _ => {}
        }
    }
    if rec.name.is_empty() {
        None
    } else {
        Some(rec)
    }
}

fn html_to_text(html: &str) -> String {
    static RE: OnceLock<(regex::Regex, regex::Regex, regex::Regex, regex::Regex)> = OnceLock::new();
    let (re_block, re_br, re_block_tag, re_tag) = RE.get_or_init(|| {
        (
            regex::Regex::new(
                r"(?is)<script\b[^>]*>.*?</script>|<style\b[^>]*>.*?</style>|<noscript\b[^>]*>.*?</noscript>|<svg\b[^>]*>.*?</svg>",
            )
            .unwrap(),
            regex::Regex::new(r"(?i)<br\s*/?>").unwrap(),
            regex::Regex::new(r"(?i)</(p|div|h[1-6]|li|tr|section|article|blockquote)>").unwrap(),
            regex::Regex::new(r"(?s)<[^>]+>").unwrap(),
        )
    });
    let s = re_block.replace_all(html, " ");
    let s = re_br.replace_all(&s, "\n");
    let s = re_block_tag.replace_all(&s, "\n");
    let s = re_tag.replace_all(&s, " ");
    let s = decode_entities(&s);
    let mut out = String::new();
    let mut blank = 0;
    for line in s.lines() {
        let t = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(&t);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn extract_links(base: &str, html: &str) -> Vec<String> {
    collect_attr_urls(base, html, r#"(?i)(?:href|src)\s*=\s*["']([^"']+)["']"#)
}

fn extract_scripts(base: &str, html: &str) -> Vec<String> {
    collect_attr_urls(
        base,
        html,
        r#"(?is)<script\b[^>]*?\bsrc\s*=\s*["']([^"']+)["']"#,
    )
    .into_iter()
    .filter(|u| {
        let l = u.to_ascii_lowercase();
        l.contains(".js") || l.contains("/js") || !l.rsplit('/').next().unwrap_or("").contains('.')
    })
    .collect()
}

fn extract_nav_links(base: &str, html: &str) -> Vec<String> {
    collect_attr_urls(
        base,
        html,
        r#"(?is)<a\b[^>]*?\bhref\s*=\s*["']([^"']+)["']"#,
    )
}

fn collect_attr_urls(base: &str, html: &str, pattern: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(pattern) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in re.captures_iter(html) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if raw.is_empty()
            || raw.starts_with("javascript:")
            || raw.starts_with("data:")
            || raw.starts_with('#')
        {
            continue;
        }
        let abs = resolve_url(base, raw);
        if seen.insert(abs.clone()) {
            out.push(abs);
        }
    }
    out
}

fn is_harvest_noise(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    l.contains("/appupdate/")
        || l.contains("/download/")
        || l.contains(".png")
        || l.contains(".jpg")
        || l.contains(".gif")
        || l.contains(".css")
        || l.contains(".woff")
        || l.contains("googleapis.com")
}

fn is_spa_shell(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    if !lower.contains("<html") && !lower.contains("<!doctype") {
        return false;
    }
    if lower.contains("<form") {
        return false;
    }
    let scripts = lower.matches("<script").count();
    if scripts == 0 {
        return false;
    }
    html_to_text(html).chars().count() < 80
}

fn looks_like_api_origin(origin: &str) -> bool {
    let h = host_of(origin).unwrap_or_default();
    if h.contains("s3.") || h.contains("cdn") || h.contains("static.") {
        return false;
    }
    h.contains("api")
        || h.contains("openapi")
        || h.contains("apigw")
        || h.contains("epassport")
        || h.contains("passport")
        || h.contains("gateway")
}

fn api_origins(page: &str, urls: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(o) = origin_of(page) {
        out.push(o);
    }
    for u in urls {
        if let Some(o) = origin_of(u) {
            if looks_like_api_origin(&o) && !out.iter().any(|x| x == &o) {
                out.push(o);
            }
        }
    }
    out
}

#[derive(Clone, Debug, Serialize)]
struct ProbeHit {
    url: String,
    status: u16,
    kind: String,
    gateway_error: bool,
    nomatch: bool,
    auth: bool,
    note: String,
}

fn classify_probe(status: u16, ctype: &str, headers: &[(String, String)], body: &str) -> ProbeHit {
    let gateway_error = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("mt-gateway-error") && v != "false" && !v.is_empty());
    let cl = ctype.to_ascii_lowercase();
    let kind = if cl.contains("json") || body.trim_start().starts_with('{') {
        "json"
    } else if cl.contains("html") || body.to_ascii_lowercase().contains("<html") {
        "html"
    } else {
        "other"
    };
    let low = body.to_ascii_lowercase();
    let nomatch = low.contains("pathnotmatch") || low.contains("no matched api");
    let auth = low.contains("请重新登录")
        || low.contains("\"token\":null")
        || low.contains("\"token\": null")
        || body.contains("\"code\":14101")
        || body.contains("\"code\": 14101")
        || body.contains("\"code\":601");
    let note = if let Ok(v) = serde_json::from_str::<serde_json::Value>(body.trim()) {
        let code = v.get("code").map(|c| c.to_string()).unwrap_or_default();
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>();
        format!("code={code} {msg}").trim().to_string()
    } else if kind == "html" {
        "html/spa".into()
    } else {
        body.chars().take(60).collect()
    };
    ProbeHit {
        url: String::new(),
        status,
        kind: kind.into(),
        gateway_error,
        nomatch,
        auth,
        note,
    }
}

fn fetch_get(
    agent: &ureq::Agent,
    url: &str,
    ua: &str,
    cookie: Option<&str>,
    referer: Option<&str>,
    limit: usize,
) -> anyhow::Result<(u16, Vec<(String, String)>, String)> {
    let mut req = agent.get(url).header("User-Agent", ua);
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    if let Some(r) = referer {
        req = req.header("Referer", r);
    }
    let resp = req
        .call()
        .map_err(|e| anyhow::anyhow!("GET {url} 失败: {e}"))?;
    let status = resp.status().as_u16();
    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut bytes = Vec::new();
    resp.into_parts()
        .1
        .into_reader()
        .take(limit as u64)
        .read_to_end(&mut bytes)?;
    Ok((
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    ))
}

fn is_telemetry_js(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    [
        "logan",
        "owl_",
        "powl_",
        "/lx.js",
        "h5guard",
        "google-analytics",
        "googletagmanager",
        "baidu.com/hm",
        "sensorsdata",
        "report.meituan",
        "wreport",
        "catfront",
        "certificates.meituan.com/cert",
    ]
    .iter()
    .any(|k| l.contains(k))
}

fn pick_app_scripts(scripts: &[String]) -> Vec<String> {
    let usable: Vec<String> = scripts
        .iter()
        .filter(|s| !is_telemetry_js(s))
        .cloned()
        .collect();
    let mut preferred: Vec<String> = usable
        .iter()
        .filter(|s| {
            let name = s.rsplit('/').next().unwrap_or(s).to_ascii_lowercase();
            name.starts_with("index-")
                || name.starts_with("main.")
                || name.starts_with("app.")
                || name.starts_with("chunk")
                || name.contains("index-")
        })
        .cloned()
        .collect();
    if preferred.is_empty() {
        preferred = usable;
    }
    preferred
}

fn harvest_endpoints(js: &str) -> (Vec<String>, Vec<String>) {
    static RE_URL: OnceLock<regex::Regex> = OnceLock::new();
    static RE_PATH: OnceLock<regex::Regex> = OnceLock::new();
    let re_url = RE_URL.get_or_init(|| {
        regex::Regex::new(r"https?://[a-zA-Z0-9._~:/?#@!$&*+,;=%-]{8,180}").unwrap()
    });
    let re_path = RE_PATH
        .get_or_init(|| regex::Regex::new(r#"["'` ](/[a-zA-Z0-9._~:/=?&-]{6,90})["'` ]"#).unwrap());
    let url_keys = [
        "api.",
        "/api",
        "gateway",
        "login",
        "oauth",
        "sso",
        "auth",
        "graphql",
        "epassport",
        "passport",
        "openapi",
        "apigw",
    ];
    let path_keys = [
        "/api", "/gw", "/login", "/auth", "/sso", "/oauth", "/graphql",
    ];
    let mut urls = Vec::new();
    let mut seen_u = std::collections::HashSet::new();
    for m in re_url.find_iter(js) {
        let u = m.as_str().trim_end_matches('\\').to_string();
        let lu = u.to_ascii_lowercase();
        if url_keys.iter().any(|k| lu.contains(k))
            && !is_harvest_noise(&u)
            && seen_u.insert(u.clone())
        {
            urls.push(u);
        }
    }
    let mut paths = Vec::new();
    let mut seen_p = std::collections::HashSet::new();
    for cap in re_path.captures_iter(js) {
        let p = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let lp = p.to_ascii_lowercase();
        if path_keys.iter().any(|k| lp.contains(k)) && seen_p.insert(p.to_string()) {
            paths.push(p.to_string());
        }
    }
    (urls, paths)
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if let Some(rest) = href.strip_prefix("//") {
        let scheme = if base.starts_with("https") {
            "https"
        } else {
            "http"
        };
        return format!("{scheme}://{rest}");
    }
    let origin = origin_of(base).unwrap_or_else(|| base.to_string());
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    let dir = match base.rfind('/') {
        Some(i) if i > origin.len() => &base[..=i],
        _ => {
            return format!("{origin}/{href}");
        }
    };
    format!("{dir}{href}")
}

fn encode_form_fields(fields: &[String]) -> String {
    let mut parts = Vec::new();
    for f in fields {
        let Some((k, v)) = f.split_once('=') else {
            continue;
        };
        parts.push(format!(
            "{}={}",
            urlencoding::encode(k.trim()),
            urlencoding::encode(v)
        ));
    }
    parts.join("&")
}

fn html_attr(attrs: &str, key: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(
        r#"(?i)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,
        regex::escape(key)
    ))
    .ok()?;
    let cap = re.captures(attrs)?;
    Some(
        cap.get(1)
            .or_else(|| cap.get(2))
            .or_else(|| cap.get(3))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default(),
    )
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct FormField {
    name: String,
    kind: String,
    value: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct HtmlForm {
    action: String,
    method: String,
    name: Option<String>,
    fields: Vec<FormField>,
}

fn parse_forms(base: &str, html: &str) -> Vec<HtmlForm> {
    static RE_FORM: OnceLock<regex::Regex> = OnceLock::new();
    static RE_CTRL: OnceLock<regex::Regex> = OnceLock::new();
    let re_form =
        RE_FORM.get_or_init(|| regex::Regex::new(r"(?is)<form\b([^>]*)>(.*?)</form>").unwrap());
    let re_ctrl = RE_CTRL.get_or_init(|| {
        regex::Regex::new(r"(?is)<(input|textarea|select|button)\b([^>]*)>").unwrap()
    });
    let mut out = Vec::new();
    for cap in re_form.captures_iter(html) {
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let action_raw = html_attr(attrs, "action").unwrap_or_default();
        let action = if action_raw.is_empty() {
            base.to_string()
        } else {
            resolve_url(base, &action_raw)
        };
        let method = html_attr(attrs, "method")
            .unwrap_or_else(|| "GET".into())
            .to_ascii_uppercase();
        let name = html_attr(attrs, "name").or_else(|| html_attr(attrs, "id"));
        let mut fields = Vec::new();
        for c in re_ctrl.captures_iter(body) {
            let tag = c
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or("input")
                .to_ascii_lowercase();
            let a = c.get(2).map(|m| m.as_str()).unwrap_or("");
            let Some(fname) = html_attr(a, "name") else {
                continue;
            };
            if fname.is_empty() {
                continue;
            }
            let kind = html_attr(a, "type")
                .unwrap_or_else(|| tag.clone())
                .to_ascii_lowercase();
            if matches!(kind.as_str(), "submit" | "reset" | "button" | "image") {
                continue;
            }
            let value = html_attr(a, "value").unwrap_or_default();
            fields.push(FormField {
                name: fname,
                kind,
                value,
            });
        }
        out.push(HtmlForm {
            action,
            method,
            name,
            fields,
        });
    }
    out
}

fn sh_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._~/:".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

fn print_page_cli(
    kind: &str,
    url: &str,
    html: &str,
    json: bool,
    budget: Option<usize>,
) -> anyhow::Result<()> {
    let forms = parse_forms(url, html);
    let scripts = extract_scripts(url, html);
    let mut links = extract_nav_links(url, html);
    let max_links = (budget.unwrap_or(4000) / 80).clamp(8, 40);
    if links.len() > max_links {
        links.truncate(max_links);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": url,
                "mode": kind.to_ascii_lowercase(),
                "js": "no",
                "spa": forms.is_empty() && !scripts.is_empty(),
                "hint": "无 JS 执行。SPA 用 scan 抽 API；登录态 --browser 或 --cookie-jar。",
                "forms": forms,
                "scripts": scripts,
                "links": links,
            }))?
        );
        return Ok(());
    }
    if kind == "FORMS" {
        if forms.is_empty() {
            println!("# 无 <form>。SPA/接口页：rxt http scan {}", sh_quote(url));
            return Ok(());
        }
        for (i, f) in forms.iter().enumerate() {
            println!(
                "# form {} {} {} {}",
                i + 1,
                f.method,
                f.name.as_deref().unwrap_or("-"),
                f.action
            );
            for field in &f.fields {
                println!(
                    "  {} {}\t{}",
                    field.kind,
                    field.name,
                    if field.value.is_empty() {
                        "-"
                    } else {
                        &field.value
                    }
                );
            }
        }
        return Ok(());
    }
    println!("# rxt 网页 CLI  {}  （无 JS 执行 / 无头浏览器）", url);
    println!("# 登录态: --browser chrome  或  --cookie-jar jar.txt");
    if forms.is_empty() && !scripts.is_empty() {
        println!("# SPA 壳：没有表单。下一步 rxt http scan {}", sh_quote(url));
    }
    println!(
        "rxt http GET {} --text --budget {}",
        sh_quote(url),
        budget.unwrap_or(4000)
    );
    let app_scripts: Vec<_> = scripts
        .iter()
        .filter(|s| !is_telemetry_js(s))
        .cloned()
        .collect();
    if !app_scripts.is_empty() {
        println!(
            "# scripts {} (跳过统计/H5guard {})",
            app_scripts.len(),
            scripts.len().saturating_sub(app_scripts.len())
        );
        for s in app_scripts.iter().take(12) {
            println!(
                "rxt http GET {} --budget {}",
                sh_quote(s),
                budget.unwrap_or(4000)
            );
        }
    }
    for (i, f) in forms.iter().enumerate() {
        let mut cmd = format!("rxt http {} {}", f.method, sh_quote(&f.action));
        for field in &f.fields {
            let val = if field.value.is_empty() {
                field.name.to_ascii_uppercase()
            } else {
                field.value.clone()
            };
            cmd.push_str(&format!(" --form {}={}", field.name, val));
        }
        cmd.push_str(" --cookie-jar jar.txt");
        println!("# form {} {}", i + 1, f.name.as_deref().unwrap_or("-"));
        println!("{}", cmd);
    }
    if !links.is_empty() {
        println!("# links {}", links.len());
        for l in &links {
            println!(
                "rxt http GET {} --text --budget {}",
                sh_quote(l),
                budget.unwrap_or(4000)
            );
        }
    }
    Ok(())
}

fn fetch_text(
    agent: &ureq::Agent,
    url: &str,
    ua: &str,
    cookie: Option<&str>,
    referer: Option<&str>,
) -> anyhow::Result<String> {
    fetch_get(agent, url, ua, cookie, referer, MAX_BODY).map(|(_, _, body)| body)
}

fn print_page_scan(
    agent: &ureq::Agent,
    ua: &str,
    cookies: &[CookieRec],
    url: &str,
    html: &str,
    json: bool,
    budget: Option<usize>,
    probe: bool,
) -> anyhow::Result<()> {
    let forms = parse_forms(url, html);
    let scripts = extract_scripts(url, html);
    let app_js = pick_app_scripts(&scripts);
    let mut harvested_urls: Vec<String> = Vec::new();
    let mut harvested_paths: Vec<String> = Vec::new();
    let mut scanned: Vec<String> = Vec::new();
    let page_referer = origin_of(url).map(|o| format!("{o}/"));
    for js_url in app_js.iter().take(2) {
        match fetch_text(agent, js_url, ua, None, page_referer.as_deref()) {
            Ok(js) => {
                scanned.push(js_url.clone());
                let (u, p) = harvest_endpoints(&js);
                harvested_urls.extend(u);
                harvested_paths.extend(p);
            }
            Err(e) => eprintln!("⚠ 扫脚本跳过 {}: {e}", js_url),
        }
    }
    harvested_urls.sort();
    harvested_urls.dedup();
    harvested_paths.sort_by(|a, b| {
        probe_path_score(b)
            .cmp(&probe_path_score(a))
            .then_with(|| a.cmp(b))
    });
    harvested_paths.dedup();
    let cap = (budget.unwrap_or(4000) / 80).clamp(8, 30);
    if harvested_urls.len() > cap {
        harvested_urls.truncate(cap);
    }
    if harvested_paths.len() > cap {
        harvested_paths.truncate(cap);
    }
    let origins = api_origins(url, &harvested_urls);
    let mut probes: Vec<ProbeHit> = Vec::new();
    if probe && (!harvested_paths.is_empty() || !harvested_urls.is_empty()) {
        let path_cap = cap.min(10);
        for p in harvested_paths.iter().take(path_cap) {
            let mut found_json = false;
            for o in &origins {
                let abs = format!("{o}{p}");
                match probe_url(agent, ua, cookies, page_referer.as_deref(), &abs) {
                    Ok(hit) => {
                        let useful = hit.kind == "json" && !hit.nomatch;
                        probes.push(hit);
                        if useful {
                            found_json = true;
                            break;
                        }
                    }
                    Err(e) => eprintln!("⚠ 探测跳过 {}: {e}", abs),
                }
            }
            let _ = found_json;
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": url,
                "mode": "scan",
                "js": "harvest",
                "spa": forms.is_empty() && !scripts.is_empty(),
                "scripts": scripts,
                "scanned": scanned,
                "forms": forms.len(),
                "api_urls": harvested_urls,
                "api_paths": harvested_paths,
                "origins": origins,
                "probes": probes,
            }))?
        );
        return Ok(());
    }
    println!("# rxt http scan  {}", url);
    if forms.is_empty() && !scripts.is_empty() {
        println!(
            "# SPA：HTML 无表单，接口在 JS。登录态 --cookie-json / --cookie-jar / --browser。"
        );
        println!("# H5guard 不能裸过，先用真浏览器登录再导出 Cookie。");
    }
    println!(
        "# scripts {}  scanned {}  origins {:?}",
        scripts.len(),
        scanned.len(),
        origins
    );
    for s in &scanned {
        println!("#   {}", s);
    }
    if harvested_urls.is_empty() && harvested_paths.is_empty() {
        println!("# 没抽到 API。可 rxt http GET <入口.js>，或从浏览器 Network 抄。");
        return Ok(());
    }
    if !probes.is_empty() {
        println!("# probe {}", probes.len());
        let mut live: Vec<&ProbeHit> = probes
            .iter()
            .filter(|h| h.kind == "json" && !h.nomatch)
            .collect();
        live.sort_by(|a, b| a.url.cmp(&b.url));
        live.dedup_by(|a, b| a.url == b.url);
        if live.is_empty() {
            println!(
                "# 探测到的都是 PathNotMatch / HTML 壳。带 Cookie 再扫，或从 Network 抄完整 path。"
            );
        } else {
            let need_auth = live.iter().any(|h| h.auth);
            if need_auth {
                println!("# 下面接口已通，但要登录态：--cookie-json cookies.json 或 --cookie-jar jar.txt");
            }
            for h in live {
                let tag = if h.auth { "auth" } else { "ok" };
                println!("# {} {} {}  {}", h.status, tag, h.note, h.url);
                println!(
                    "rxt http GET {} -i --budget 800 --cookie-jar jar.txt",
                    sh_quote(&h.url)
                );
            }
        }
        return Ok(());
    }
    println!("# api_urls {}", harvested_urls.len());
    for u in &harvested_urls {
        println!("rxt http GET {} -i --budget 800", sh_quote(u));
    }
    println!(
        "# api_paths {}  origins {:?}",
        harvested_paths.len(),
        origins
    );
    for p in &harvested_paths {
        for o in &origins {
            let abs = format!("{o}{p}");
            println!("rxt http GET {} -i --budget 800", sh_quote(&abs));
        }
    }
    Ok(())
}

fn probe_url(
    agent: &ureq::Agent,
    ua: &str,
    cookies: &[CookieRec],
    referer: Option<&str>,
    url: &str,
) -> anyhow::Result<ProbeHit> {
    let host = host_of(url).unwrap_or_default();
    let https = url.starts_with("https://");
    let cookie = cookie_header_for(cookies, &host, path_of(url), https);
    let (status, headers, body) = fetch_get(agent, url, ua, cookie.as_deref(), referer, 16_384)?;
    let ctype = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let mut hit = classify_probe(status, ctype, &headers, &body);
    hit.url = url.to_string();
    Ok(hit)
}

fn probe_path_score(path: &str) -> i32 {
    let l = path.to_ascii_lowercase();
    let mut s = 0;
    if l.contains("login") {
        s += 100;
    }
    if l.contains("token") {
        s += 80;
    }
    if l.contains("/auth") || l.contains("oauth") || l.contains("/sso") {
        s += 60;
    }
    if l.contains("/accounts") {
        s += 40;
    }
    if l.contains("/api/v1") {
        s += 10;
    }
    if l.contains("apaas") || l.contains("deldata") {
        s -= 30;
    }
    s
}

fn json_str(v: &serde_json::Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

fn json_bool(v: &serde_json::Value, keys: &[&str]) -> bool {
    keys.iter()
        .any(|k| v.get(*k).and_then(|x| x.as_bool()).unwrap_or(false))
}

fn load_cookie_json(arg: &str, fallback_host: &str) -> anyhow::Result<Vec<CookieRec>> {
    let text = {
        let t = arg.trim();
        if t.starts_with('[') || t.starts_with('{') {
            t.to_string()
        } else {
            std::fs::read_to_string(arg)
                .map_err(|e| anyhow::anyhow!("读 cookie-json {}: {e}", arg))?
        }
    };
    parse_cookie_json(&text, fallback_host)
}

fn parse_cookie_json(text: &str, fallback_host: &str) -> anyhow::Result<Vec<CookieRec>> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| anyhow::anyhow!("cookie-json 不是合法 JSON: {e}"))?;
    let arr: Vec<serde_json::Value> = if let Some(a) = v.as_array() {
        a.clone()
    } else if let Some(a) = v.get("cookies").and_then(|x| x.as_array()) {
        a.clone()
    } else {
        anyhow::bail!("cookie-json 需要数组，或 {{\"cookies\":[...]}}（DevTools 导出）");
    };
    let mut out = Vec::new();
    for item in arr {
        let name = json_str(&item, &["name", "Name"]);
        if name.is_empty() {
            continue;
        }
        let value = json_str(&item, &["value", "Value"]);
        let mut domain = json_str(&item, &["domain", "Domain"]);
        if domain.is_empty() {
            domain = fallback_host.to_string();
        }
        let path = json_str(&item, &["path", "Path"]);
        out.push(CookieRec {
            domain,
            path: if path.is_empty() { "/".into() } else { path },
            secure: json_bool(&item, &["secure", "Secure"]),
            http_only: json_bool(&item, &["httpOnly", "http_only", "HttpOnly"]),
            expires: None,
            name,
            value,
        });
    }
    Ok(out)
}

fn origin_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let hostport = rest.split('/').next()?;
    let scheme = url.split("://").next()?;
    Some(format!("{scheme}://{hostport}"))
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let hostport = rest.split(['/', '?', '#']).next()?;
    let host = hostport
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(hostport);
    Some(host.split(':').next().unwrap_or(host).to_ascii_lowercase())
}

fn path_of(url: &str) -> &str {
    let Some(rest) = url.split("://").nth(1) else {
        return "/";
    };
    match rest.find('/') {
        Some(i) => {
            let p = rest[i..].split(['?', '#']).next().unwrap_or("/");
            if p.is_empty() {
                "/"
            } else {
                p
            }
        }
        None => "/",
    }
}

fn domain_from_input(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") {
        host_of(s).unwrap_or_else(|| s.to_string())
    } else {
        s.trim().trim_start_matches('.').to_ascii_lowercase()
    }
}

fn domain_candidates(host: &str) -> Vec<String> {
    let host = host.trim().trim_start_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Vec::new();
    }
    let mut v = vec![host.clone()];
    if let Some(i) = host.find('.') {
        let parent = &host[i + 1..];
        // 只要带点的上级（www.a.com → a.com），不要 TLD（a.com → com）
        if parent.contains('.') {
            v.push(parent.to_string());
        }
    }
    v
}

fn domain_matches(cookie_domain: &str, host: &str) -> bool {
    let d = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let h = host.trim_start_matches('.').to_ascii_lowercase();
    h == d || h.ends_with(&format!(".{d}"))
}

fn path_matches(cookie_path: &str, req_path: &str) -> bool {
    let cp = if cookie_path.is_empty() {
        "/"
    } else {
        cookie_path
    };
    if cp == "/" || req_path == cp {
        return true;
    }
    let prefix = if cp.ends_with('/') {
        cp.to_string()
    } else {
        format!("{cp}/")
    };
    req_path.starts_with(&prefix)
}

fn has_header(headers: &[String], name: &str) -> bool {
    headers.iter().any(|h| {
        h.split_once(':')
            .map(|(k, _)| k.trim().eq_ignore_ascii_case(name))
            .unwrap_or(false)
    })
}

fn is_probably_text(ctype: &str, bytes: &[u8]) -> bool {
    let c = ctype.to_ascii_lowercase();
    if c.contains("json")
        || c.contains("text/")
        || c.contains("xml")
        || c.contains("javascript")
        || c.contains("html")
    {
        return true;
    }
    if c.contains("octet-stream")
        || c.contains("image/")
        || c.contains("audio/")
        || c.contains("video/")
        || c.contains("font/")
    {
        return false;
    }
    let n = bytes.len().min(512);
    n > 0 && bytes[..n].iter().filter(|b| **b == 0).count() == 0
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn build_auth_header(auth: Option<&str>) -> Option<String> {
    auth.map(|basic| {
        let raw = if basic.contains(':') {
            basic.as_bytes()
        } else {
            basic.as_bytes()
        };
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_urls_promotes_bare_url_method() {
        let rest = vec!["https://b.example/".into()];
        let (m, u) = collect_urls("https://a.example/", &rest);
        assert_eq!(m, "GET");
        assert_eq!(u, vec!["https://a.example/", "https://b.example/"]);
        let (m2, u2) = collect_urls("Post", &rest);
        assert_eq!(m2, "POST");
        assert_eq!(u2, rest);
    }

    fn spawn_plain(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = std::io::Read::read(&mut s, &mut buf);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
            }
        });
        (format!("http://{addr}/"), h)
    }

    fn test_opts<'a>(urls: &'a [String]) -> HttpOpts<'a> {
        HttpOpts {
            method: "GET",
            urls,
            headers: &[],
            data: None,
            json_body: false,
            auth: None,
            timeout: 5,
            show_headers: false,
            body_only: false,
            output: None,
            browser: None,
            cookie_jar: None,
            cookies: &[],
            user_agent: None,
            text: false,
            links: false,
            budget: None,
            form: &[],
            no_probe: true,
            cookie_json: None,
            select: None,
            session: None,
            engine: None,
            auth_hosts: &[],
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn spawn_capture() -> (
        String,
        std::sync::Arc<std::sync::Mutex<String>>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let got = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let got2 = got.clone();
        let h = std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                *got2.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).into_owned();
                let body = b"ok";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    std::str::from_utf8(body).unwrap()
                );
                let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
            }
        });
        (format!("http://{addr}/api"), got, h)
    }

    #[test]
    fn batch_two_local_servers() {
        let (u1, t1) = spawn_plain("hello-a");
        let (u2, t2) = spawn_plain("hello-b");
        let urls = vec![u1, u2];
        let opts = test_opts(&urls);
        let out = fetch_parallel(&opts, "GET", &urls);
        let _ = t1.join();
        let _ = t2.join();
        assert_eq!(out.len(), 2);
        assert!(
            out.iter().all(|x| x.error.is_none()),
            "{:?}",
            out.iter().map(|x| x.error.clone()).collect::<Vec<_>>()
        );
        let bodies: Vec<String> = out
            .iter()
            .map(|x| String::from_utf8_lossy(&x.bytes).into_owned())
            .collect();
        assert!(bodies.iter().any(|b| b.contains("hello-a")), "{bodies:?}");
        assert!(bodies.iter().any(|b| b.contains("hello-b")), "{bodies:?}");
        let js = batch_json(&opts, &out);
        assert_eq!(js["count"], 2);
        assert_eq!(js["ok"], 2);
    }

    #[test]
    fn host_parse() {
        assert_eq!(
            host_of("https://www.GitHub.com/foo").as_deref(),
            Some("www.github.com")
        );
        assert_eq!(path_of("https://x.com/a/b?q=1"), "/a/b");
        assert_eq!(
            origin_of("https://x.com:8443/a").as_deref(),
            Some("https://x.com:8443")
        );
        assert_eq!(
            domain_candidates("example.com"),
            vec!["example.com".to_string()]
        );
        assert_eq!(
            domain_candidates("www.github.com"),
            vec!["www.github.com".to_string(), "github.com".to_string()]
        );
    }

    #[test]
    fn cookie_domain_and_path() {
        assert!(domain_matches(".github.com", "github.com"));
        assert!(domain_matches(".github.com", "gist.github.com"));
        assert!(!domain_matches(".github.com", "evilgithub.com"));
        assert!(path_matches("/", "/login"));
        assert!(path_matches("/api", "/api/v1"));
        assert!(!path_matches("/api", "/apiv1"));
    }

    #[test]
    fn html_and_links() {
        let html = r#"<html><head><script>x=1</script><style>p{}</style></head><body><h1>Hi</h1><p>A &amp; B</p><a href="/x">x</a><img src="https://cdn.example/a.png"></body></html>"#;
        let t = html_to_text(html);
        assert!(t.contains("Hi"));
        assert!(t.contains("A & B"));
        assert!(!t.contains("x=1"));
        let links = extract_links("https://ex.com/dir/page", html);
        assert!(links.iter().any(|l| l == "https://ex.com/x"));
        assert!(links.iter().any(|l| l == "https://cdn.example/a.png"));
    }

    #[test]
    fn netscape_roundtrip() {
        let dir = std::env::temp_dir().join("rxt-http-cookie-test.txt");
        let recs = vec![CookieRec {
            domain: ".example.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: Some(2000000000),
            name: "sid".into(),
            value: "abc".into(),
        }];
        save_netscape(&dir, &recs).unwrap();
        let loaded = load_netscape(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "sid");
        assert_eq!(loaded[0].value, "abc");
        assert!(loaded[0].http_only);
        assert!(loaded[0].secure);
    }

    #[test]
    fn set_cookie_parse() {
        let c = parse_set_cookie(
            "sid=xyz; Path=/app; Domain=ex.com; Secure; HttpOnly; Max-Age=60",
            "www.ex.com",
        )
        .unwrap();
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "xyz");
        assert_eq!(c.path, "/app");
        assert!(c.secure && c.http_only);
        assert!(c.domain.contains("ex.com"));
    }

    #[test]
    fn cookie_header_filters() {
        let cookies = vec![
            CookieRec {
                domain: ".ex.com".into(),
                path: "/".into(),
                secure: true,
                http_only: false,
                expires: None,
                name: "a".into(),
                value: "1".into(),
            },
            CookieRec {
                domain: "other.com".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                name: "b".into(),
                value: "2".into(),
            },
        ];
        let h = cookie_header_for(&cookies, "www.ex.com", "/", true).unwrap();
        assert!(h.contains("a=1"));
        assert!(!h.contains("b=2"));
    }

    #[cfg(not(feature = "cookies"))]
    #[test]
    fn browser_requires_cookies_feature() {
        let e = load_browser("chrome", None).unwrap_err().to_string();
        assert!(e.contains("cookies"));
    }

    #[test]
    fn parse_login_form() {
        let html = r#"<html><form id="login" action="/sess" method="post">
            <input type="hidden" name="token" value="abc">
            <input type="text" name="user">
            <input type="password" name="pass">
            <button type="submit">go</button>
            </form>
            <a href="/about">about</a></html>"#;
        let forms = parse_forms("https://ex.com/login", html);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].method, "POST");
        assert_eq!(forms[0].action, "https://ex.com/sess");
        let names: Vec<_> = forms[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["token", "user", "pass"]);
        assert_eq!(forms[0].fields[0].value, "abc");
        let links = extract_links("https://ex.com/login", html);
        assert!(links.iter().any(|l| l == "https://ex.com/about"));
    }

    #[test]
    fn form_fields_urlencoded() {
        let s = encode_form_fields(&["q=hello world".into(), "n=1".into()]);
        assert!(s.contains("q=hello%20world"));
        assert!(s.contains("n=1"));
    }

    #[test]
    fn harvest_from_minified_js() {
        let js = r#"var A="https://canyin-openapi.meituan.com",B="/api/v1/cashier/h5/logout",C="https://epassport.meituan.com/login";"#;
        let (urls, paths) = harvest_endpoints(js);
        assert!(urls.iter().any(|u| u.contains("canyin-openapi")));
        assert!(urls.iter().any(|u| u.contains("epassport")));
        assert!(paths.iter().any(|p| p.contains("/api/v1/cashier")));
    }

    #[test]
    fn pick_app_skips_telemetry() {
        let scripts = vec![
            "https://x.com/h5guard.js".into(),
            "https://cdn/mpack/index/index-abc.js".into(),
            "https://x.com/logan.js".into(),
        ];
        assert_eq!(
            pick_app_scripts(&scripts),
            vec!["https://cdn/mpack/index/index-abc.js".to_string()]
        );
    }

    #[test]
    fn scripts_vs_nav_links() {
        let html = r#"<html><head>
            <link rel="dns-prefetch" href="//s0.example.net">
            <script src="https://cdn.example/index-1.js"></script>
            </head><body><a href="/about">a</a></body></html>"#;
        let scripts = extract_scripts("https://pos.example.com/", html);
        assert_eq!(scripts, vec!["https://cdn.example/index-1.js".to_string()]);
        let nav = extract_nav_links("https://pos.example.com/", html);
        assert_eq!(nav, vec!["https://pos.example.com/about".to_string()]);
        let all = extract_links("https://pos.example.com/", html);
        assert!(all.iter().any(|l| l.contains("s0.example.net")));
    }

    #[test]
    fn harvest_skips_download_noise() {
        let js = r#"x="https://apimobile.meituan.com/appupdate/download/simple/KDS?channel=next";y="https://canyin-openapi.meituan.com";z="/api/v1/admin/h5/login/v2";"#;
        let (urls, paths) = harvest_endpoints(js);
        assert!(urls.iter().all(|u| !u.contains("/download/")));
        assert!(urls.iter().any(|u| u.contains("canyin-openapi")));
        assert!(paths.iter().any(|p| p.contains("/api/v1/admin/h5/login")));
    }

    #[test]
    fn api_origins_keeps_openapi() {
        let o = api_origins(
            "https://pos.meituan.com/",
            &["https://canyin-openapi.meituan.com/foo".into()],
        );
        assert!(o.iter().any(|x| x == "https://pos.meituan.com"));
        assert!(o.iter().any(|x| x == "https://canyin-openapi.meituan.com"));
    }

    #[test]
    fn classify_gateway_json() {
        let h = vec![("mt-gateway-error".into(), "true".into())];
        let p = classify_probe(
            200,
            "application/json",
            &h,
            r#"{"code":14101,"message":"登录失败，请重新登录"}"#,
        );
        assert!(p.auth);
        assert!(!p.nomatch);
        assert!(p.gateway_error);
        let n = classify_probe(
            200,
            "application/json",
            &h,
            r#"{"code":50001,"message":"找不到请求路径: PathNotMatchException"}"#,
        );
        assert!(n.nomatch);
    }

    #[test]
    fn tabbit_roots_named() {
        let s = format!("{:?}", tabbit_roots()).to_ascii_lowercase();
        assert!(s.contains("tabbit"), "{s}");
    }

    #[test]
    fn chromium_pairs_missing_dir() {
        assert!(chromium_cookie_pairs(Path::new("/no/such/tabbit")).is_empty());
    }

    #[test]
    fn identity_sends_session_cookie_and_bearer() {
        let _g = env_lock();
        let (url, got, _t) = spawn_capture();
        let dir =
            std::env::temp_dir().join(format!("rxt-ident-{}-{}", std::process::id(), now_unix()));
        let _ = std::fs::remove_dir_all(&dir);
        persist_login(
            &dir,
            &[CookieRec {
                domain: "127.0.0.1".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                name: "sid".into(),
                value: "abc".into(),
            }],
            "test",
        )
        .unwrap();
        secure_write(
            &dir.join("storage.json"),
            br#"{"local":{"access_token":"tok_sso_12345678"},"session":{}}"#,
        )
        .unwrap();
        record_origin(&dir, &url).unwrap();
        std::env::set_var("RXT_HTTP_SESSION_DIR", &dir);
        std::env::set_var("RXT_HTTP_ENGINE", "static");
        let urls = vec![url];
        let opts = HttpOpts {
            engine: Some("static"),
            ..test_opts(&urls)
        };
        let _ = run(opts);
        let req = got.lock().unwrap().clone();
        assert!(req.contains("sid=abc"), "{req}");
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer tok_sso_12345678"),
            "{req}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("RXT_HTTP_SESSION_DIR");
        std::env::remove_var("RXT_HTTP_ENGINE");
    }

    #[test]
    fn identity_does_not_send_bearer_cross_origin() {
        let _g = env_lock();
        let (url, got, _t) = spawn_capture();
        let dir = std::env::temp_dir().join(format!(
            "rxt-ident-xorigin-{}-{}",
            std::process::id(),
            now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        persist_login(
            &dir,
            &[CookieRec {
                domain: "internal.example".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                name: "access_token".into(),
                value: "tok_internal_should_not_leak".into(),
            }],
            "test",
        )
        .unwrap();
        secure_write(
            &dir.join("storage.json"),
            br#"{"local":{"access_token":"tok_sso_CROSS_LEAK"},"session":{"csrf":"csrf-internal"}}"#,
        )
        .unwrap();
        record_origin(&dir, "https://internal.example/").unwrap();
        std::env::set_var("RXT_HTTP_SESSION_DIR", &dir);
        std::env::set_var("RXT_HTTP_ENGINE", "static");
        let urls = vec![url];
        let opts = HttpOpts {
            engine: Some("static"),
            ..test_opts(&urls)
        };
        let _ = run(opts);
        let req = got.lock().unwrap().clone();
        let low = req.to_ascii_lowercase();
        assert!(
            !low.contains("authorization"),
            "跨域不得发送 Authorization: {req}"
        );
        assert!(
            !low.contains("tok_sso_cross_leak") && !low.contains("tok_internal_should_not_leak"),
            "跨域不得泄漏 token: {req}"
        );
        assert!(!low.contains("x-csrf") && !low.contains("x-xsrf"), "{req}");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("RXT_HTTP_SESSION_DIR");
        std::env::remove_var("RXT_HTTP_ENGINE");
    }

    #[test]
    fn auth_ok_same_origin_or_allowlist() {
        let dir =
            std::env::temp_dir().join(format!("rxt-origin-{}-{}", std::process::id(), now_unix()));
        let _ = std::fs::remove_dir_all(&dir);
        record_origin(&dir, "https://app.internal/login").unwrap();
        let urls: Vec<String> = vec![];
        let opts = test_opts(&urls);
        assert!(auth_ok_for_url(&opts, &dir, "https://app.internal/api"));
        assert!(!auth_ok_for_url(&opts, &dir, "https://evil.example/api"));
        assert!(!auth_ok_for_url(&opts, &dir, "http://app.internal/api"));
        let allow = vec!["evil.example".into()];
        let opts2 = HttpOpts {
            auth_hosts: &allow,
            ..test_opts(&urls)
        };
        assert!(auth_ok_for_url(&opts2, &dir, "https://evil.example/x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn persist_login_unix_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "rxt-login-mode-{}-{}",
            std::process::id(),
            now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        persist_login(
            &dir,
            &[CookieRec {
                domain: ".ex.com".into(),
                path: "/".into(),
                secure: true,
                http_only: true,
                expires: Some(2000000000),
                name: "sid".into(),
                value: "abc".into(),
            }],
            "firefox",
        )
        .unwrap();
        let file_mode = std::fs::metadata(dir.join("cookies.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let json_mode = std::fs::metadata(dir.join("cookies.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600, "cookies.txt {file_mode:o}");
        assert_eq!(json_mode, 0o600, "cookies.json {json_mode:o}");
        assert_eq!(dir_mode, 0o700, "session dir {dir_mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn purge_wipes_auth_files() {
        let dir =
            std::env::temp_dir().join(format!("rxt-purge-{}-{}", std::process::id(), now_unix()));
        let _ = std::fs::remove_dir_all(&dir);
        persist_login(
            &dir,
            &[CookieRec {
                domain: "ex.com".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                name: "sid".into(),
                value: "secret-cookie".into(),
            }],
            "test",
        )
        .unwrap();
        secure_write(
            &dir.join("storage.json"),
            br#"{"local":{"access_token":"tok"}}"#,
        )
        .unwrap();
        purge_session(&dir).unwrap();
        assert!(!dir.join("cookies.txt").exists());
        assert!(!dir.join("storage.json").exists());
        assert!(!dir.join("origin.json").exists());
        assert!(!dir.exists() || std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0) == 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_login_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rxt-login-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let recs = vec![CookieRec {
            domain: ".ex.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: Some(2000000000),
            name: "sid".into(),
            value: "abc".into(),
        }];
        persist_login(&dir, &recs, "firefox").unwrap();
        let loaded = load_netscape(&dir.join("cookies.txt")).unwrap();
        assert_eq!(loaded[0].name, "sid");
        let js = std::fs::read_to_string(dir.join("cookies.json")).unwrap();
        assert!(js.contains("abc"));
        let login = std::fs::read_to_string(dir.join("login.json")).unwrap();
        assert!(login.contains("firefox"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cookie_json_devtools() {
        let recs = parse_cookie_json(
            r#"[{"name":"loginToken","value":"abc","domain":".meituan.com","path":"/","secure":true,"httpOnly":true}]"#,
            "pos.meituan.com",
        )
        .unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].name, "loginToken");
        assert_eq!(recs[0].value, "abc");
        assert_eq!(recs[0].domain, ".meituan.com");
        assert!(recs[0].http_only && recs[0].secure);
    }

    #[test]
    fn probe_prefers_login_paths() {
        assert!(
            probe_path_score("/api/v1/admin/h5/login/v2") > probe_path_score("/apaas/api/rms/x")
        );
        assert!(probe_path_score("/api/v1/accounts/token") > probe_path_score("/api/cem/c/launch"));
    }

    #[test]
    fn spa_shell_detect() {
        let html = r#"<!doctype html><html><head><script src="a.js"></script></head><body><div id="app"></div></body></html>"#;
        assert!(is_spa_shell(html));
        assert!(!is_spa_shell(r#"<html><form action="/"></form></html>"#));
    }

    #[test]
    fn session_anon_without_cookies() {
        assert_eq!(
            session_verdict(0, 200, "application/json", &[], "{}"),
            SessionVerdict::Anon
        );
    }

    #[test]
    fn session_expired_on_relogin() {
        let v = session_verdict(
            3,
            200,
            "application/json",
            &[],
            r#"{"code":14101,"message":"登录失败，请重新登录"}"#,
        );
        assert!(matches!(v, SessionVerdict::Expired(_)));
    }

    #[test]
    fn session_ok_when_token_present() {
        let v = session_verdict(
            2,
            200,
            "application/json",
            &[],
            r#"{"code":0,"data":{"token":"abc"}}"#,
        );
        assert!(matches!(v, SessionVerdict::Ok(_)));
    }

    #[test]
    fn session_expired_on_html() {
        let v = session_verdict(1, 200, "text/html", &[], "<html><div id=app></div></html>");
        assert!(matches!(v, SessionVerdict::Expired(_)));
    }

    #[test]
    fn extract_select_h1_and_table() {
        let html = r#"<html><h1>Hello</h1><table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table></html>"#;
        assert_eq!(
            browse::extract_select(html, "h1"),
            vec!["Hello".to_string()]
        );
        let t = browse::extract_select(html, "table");
        assert!(!t.is_empty(), "{t:?}");
        assert!(t[0].contains("\"a\""), "{}", t[0]);
        assert!(t[0].contains("1"), "{}", t[0]);
    }

    #[test]
    fn page_session_open_fill_click() {
        let _g = env_lock();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _t = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok((mut s, _)) = listener.accept() {
                    use std::io::Read;
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 1024];
                    loop {
                        let n = s.read(&mut tmp).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&buf[..pos]);
                            let cl = headers
                                .lines()
                                .find_map(|l| {
                                    l.split_once(':').and_then(|(k, v)| {
                                        if k.eq_ignore_ascii_case("content-length") {
                                            v.trim().parse::<usize>().ok()
                                        } else {
                                            None
                                        }
                                    })
                                })
                                .unwrap_or(0);
                            let start = pos + 4;
                            while buf.len() < start + cl {
                                let n = s.read(&mut tmp).unwrap_or(0);
                                if n == 0 {
                                    break;
                                }
                                buf.extend_from_slice(&tmp[..n]);
                            }
                            break;
                        }
                        if buf.len() > 65536 {
                            break;
                        }
                    }
                    let req = String::from_utf8_lossy(&buf);
                    let body = if req.starts_with("POST") {
                        let q = req.split("\r\n\r\n").nth(1).unwrap_or("");
                        format!("<html><title>ok</title><p>got {q}</p></html>")
                    } else {
                        r#"<html><title>search</title>
                        <form action="/go" method="post" name="s">
                          <input type="text" name="q">
                          <button type="submit">go</button>
                        </form>
                        <a href="/about">about</a>
                        </html>"#
                            .into()
                    };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
                }
            }
        });
        let url = format!("http://{addr}/");
        let dir = std::env::temp_dir().join(format!("rxt-http-sess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("RXT_HTTP_SESSION_DIR", &dir);
        std::env::set_var("RXT_HTTP_ENGINE", "static");
        let urls = vec![url.clone()];
        let opts = HttpOpts {
            engine: Some("static"),
            ..test_opts(&urls)
        };
        browse::run(&opts, "OPEN", &urls).unwrap();
        std::env::set_var("RXT_HTTP_SESSION_DIR", &dir);
        let snap = std::fs::read_to_string(dir.join("refs.json")).unwrap();
        assert!(snap.contains("textbox"), "{snap}");
        assert!(snap.contains("button"), "{snap}");
        browse::run(&opts, "FILL", &["@e1".into(), "hello".into()]).unwrap();
        browse::run(&opts, "CLICK", &["@e2".into()]).unwrap();
        let html = std::fs::read_to_string(dir.join("page.html")).unwrap();
        assert!(
            html.contains("got q=hello") || html.contains("q=hello"),
            "{html}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("RXT_HTTP_ENGINE");
        std::env::remove_var("RXT_HTTP_SESSION_DIR");
    }

    #[test]
    fn js_engine_hydrate_click_net() {
        if !cdp::available() {
            return;
        }
        let _g = env_lock();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _srv = std::thread::spawn(move || {
            for _ in 0..8 {
                if let Ok((mut s, _)) = listener.accept() {
                    use std::io::{Read, Write};
                    let mut buf = [0u8; 2048];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let (ctype, body): (&str, String) = if req.contains(" /api") {
                        ("application/json", r#"{"secret":"from-js"}"#.into())
                    } else {
                        (
                            "text/html",
                            r#"<html><title>jsapp</title><body>
<div id="app">loading</div>
<button id="go">clickme</button>
<script>
document.getElementById('app').innerText='hydrated';
document.getElementById('go').addEventListener('click',()=>{document.getElementById('app').innerText='clicked';});
fetch('/api').then(r=>r.json()).then(j=>{window.__api=j;document.getElementById('app').innerText='hydrated '+j.secret;});
</script></body></html>"#
                                .into(),
                        )
                    };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = s.write_all(resp.as_bytes());
                }
            }
        });
        let url = format!("http://{addr}/");
        let dir =
            std::env::temp_dir().join(format!("rxt-http-js-{}-{}", std::process::id(), now_unix()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("RXT_HTTP_SESSION_DIR", &dir);
        std::env::set_var("RXT_HTTP_ENGINE", "js");
        let _ = cdp::hold_quit(&dir);
        let urls = vec![url];
        let opts = HttpOpts {
            engine: Some("js"),
            timeout: 15,
            ..test_opts(&urls)
        };
        browse::run(&opts, "OPEN", &urls).unwrap();
        let _ = browse::run(
            &opts,
            "WAIT",
            &["document.getElementById('app')&&document.getElementById('app').innerText.indexOf('hydrated')>=0".into()],
        );
        let html = std::fs::read_to_string(dir.join("page.html")).unwrap();
        assert!(
            html.contains("hydrated from-js") || html.contains("hydrated"),
            "{html}"
        );
        let net = std::fs::read_to_string(dir.join("net.json")).unwrap_or_default();
        assert!(net.contains("from-js") || net.contains("/api"), "{net}");
        browse::run(&opts, "CLICK", &["@e1".into()]).unwrap();
        let html2 = std::fs::read_to_string(dir.join("page.html")).unwrap();
        assert!(html2.contains("clicked"), "{html2}");
        let _ = cdp::hold_quit(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("RXT_HTTP_ENGINE");
        std::env::remove_var("RXT_HTTP_SESSION_DIR");
    }
}
