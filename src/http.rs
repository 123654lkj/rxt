//! HTTP 客户端 — curl 的 LLM 友好版，可借用本机浏览器 Cookie。
//!
//! - GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS
//! - 真实超时、默认 Chrome UA、4xx 仍打印 body
//! - `--browser chrome|edge|firefox|brave|auto` 读本机 Cookie
//! - `--cookie-jar` Netscape 罐；`--cookie` 额外键值
//! - `rxt http cookies --browser chrome github.com` 列出 Cookie
//! - `--text` 抽正文 / `--links` 抽链接 / `--budget` 截断 / `-o` 落盘

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::Serialize;

const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const MAX_BODY: usize = 32 * 1024 * 1024;

pub struct HttpOpts<'a> {
    pub method: &'a str,
    pub url: Option<&'a str>,
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
}

#[derive(Clone, Debug, Serialize)]
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
    let method = opts.method.to_uppercase();
    if method == "COOKIES" {
        return dump_cookies(&opts);
    }

    let url = opts
        .url
        .ok_or_else(|| anyhow::anyhow!("需要 URL，例如: rxt http GET https://example.com"))?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("URL 必须以 http:// 或 https:// 开头");
    }

    let host = host_of(url).unwrap_or_default();
    let jar_cookies = if let Some(p) = opts.cookie_jar {
        load_netscape(p)?
    } else {
        Vec::new()
    };
    let mut browser_src: Option<String> = None;
    let browser_cookies = if let Some(b) = opts.browser {
        let domains = domain_candidates(&host);
        let (src, recs) = load_browser(b, Some(domains))?;
        browser_src = Some(src);
        recs
    } else {
        Vec::new()
    };
    let extra = parse_cookie_args(opts.cookies);
    let merged = merge_cookies(&jar_cookies, &browser_cookies, &extra);
    let https = url.starts_with("https://");
    let cookie_header = cookie_header_for(&merged, &host, path_of(url), https);
    if let Some(src) = &browser_src {
        let sent = cookie_header.as_ref().map(|s| s.split("; ").count()).unwrap_or(0);
        eprintln!(
            "# 从 {} 读到 {} 条，本次按 host={} 发送 {} 条",
            src,
            browser_cookies.len(),
            host,
            sent
        );
    }

    let timeout = Duration::from_secs(opts.timeout.max(1));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let auth_header = build_auth_header(opts.auth);
    let body_data = opts.data.unwrap_or("");
    let user_has_ua = has_header(opts.headers, "user-agent");
    let user_has_cookie = has_header(opts.headers, "cookie");
    let user_has_ct = has_header(opts.headers, "content-type");
    let ua = opts.user_agent.unwrap_or(DEFAULT_UA);

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
            if let Some(ref a) = auth_header {
                r = r.header("Authorization", a);
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

    let result = match method.as_str() {
        "GET" => paint!(agent.get(url)).call(),
        "DELETE" => paint!(agent.delete(url)).call(),
        "HEAD" => paint!(agent.head(url)).call(),
        "OPTIONS" => paint!(agent.options(url)).call(),
        "POST" | "PUT" | "PATCH" => {
            let mut r = match method.as_str() {
                "POST" => paint!(agent.post(url)),
                "PUT" => paint!(agent.put(url)),
                _ => paint!(agent.patch(url)),
            };
            if !user_has_ct {
                if opts.json_body {
                    r = r.header("Content-Type", "application/json");
                } else {
                    r = r.header("Content-Type", "application/x-www-form-urlencoded");
                }
            }
            r.send(body_data)
        }
        _ => anyhow::bail!(
            "不支持的方法: {}（GET POST PUT DELETE HEAD OPTIONS PATCH；列 Cookie 用 cookies）",
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

    if let Some(jar) = opts.cookie_jar {
        let set_cookies: Vec<CookieRec> = header_pairs
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .filter_map(|(_, v)| parse_set_cookie(v, &host))
            .collect();
        let mut all = merged;
        upsert_cookies(&mut all, &set_cookies);
        save_netscape(jar, &all)?;
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
        eprintln!("# 已写入 {} ({} bytes) HTTP {}", path.display(), bytes.len(), status);
        if opts.body_only && !opts.show_headers && !opts.text && !opts.links {
            return Ok(());
        }
    }

    if !opts.body_only {
        eprintln!("HTTP {}", status);
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
    if !looks_text && opts.output.is_none() && !opts.text && !opts.links {
        eprintln!("# 二进制 {} bytes  type={}", bytes.len(), ctype);
        return Ok(());
    }

    let raw = if looks_text {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        String::new()
    };

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

    let printable = if opts.text {
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

fn dump_cookies(opts: &HttpOpts<'_>) -> anyhow::Result<()> {
    let browser = opts
        .browser
        .ok_or_else(|| anyhow::anyhow!("列 Cookie 需要 --browser chrome|edge|firefox|brave|auto"))?;
    let filter = opts.url.map(domain_from_input);
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

    if let Some(jar) = opts.cookie_jar {
        save_netscape(jar, &recs)?;
        eprintln!("# 已写入 cookie-jar {} ({} 条)", jar.display(), recs.len());
    }

    eprintln!("# {} {} 条 (domain={})", src, recs.len(), filter.as_deref().unwrap_or(""));
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

fn load_browser(name: &str, domains: Option<Vec<String>>) -> anyhow::Result<(String, Vec<CookieRec>)> {
    #[cfg(not(feature = "cookies"))]
    {
        let _ = (name, domains);
        anyhow::bail!(
            "本二进制未启用 cookies（rookie）。\n\
             编译: cargo build --release --features cookies  或  --features net\n\
             或改用 --cookie-jar / --cookie，不读浏览器。"
        );
    }
    #[cfg(feature = "cookies")]
    {
    let key = name.trim().to_ascii_lowercase();
    if key == "auto" {
        let mut errs = Vec::new();
        for cand in ["chrome", "edge", "firefox", "brave"] {
            match load_browser(cand, domains.clone()) {
                Ok(pair) if !pair.1.is_empty() => return Ok(pair),
                Ok(_) => errs.push(format!("{cand}: 0 cookies")),
                Err(e) => errs.push(format!("{cand}: {e}")),
            }
        }
        anyhow::bail!(
            "auto 未读到浏览器 Cookie。{}\nChrome 127+ 可能要管理员运行；或改用 --browser firefox / --cookie-jar",
            errs.join(" | ")
        );
    }

    let recs = match key.as_str() {
        "chrome" => map_rookie(rookie::chrome(domains)),
        "edge" => map_rookie(rookie::edge(domains)),
        "firefox" => map_rookie(rookie::firefox(domains)),
        "brave" => map_rookie(rookie::brave(domains)),
        "chromium" => map_rookie(rookie::chromium(domains)),
        other => anyhow::bail!("未知浏览器: {other}（chrome|edge|firefox|brave|chromium|auto）"),
    }?;
    Ok((key, recs))
    }
}

#[cfg(feature = "cookies")]
fn map_rookie(res: rookie::Result<Vec<rookie::common::enums::Cookie>>) -> anyhow::Result<Vec<CookieRec>> {
    let cookies = res.map_err(|e| {
        anyhow::anyhow!(
            "{e}\n提示: Chrome/Edge 127+ 用了 App-Bound Encryption，常需管理员权限；Firefox 通常可直接读。"
        )
    })?;
    Ok(cookies
        .into_iter()
        .map(|c| CookieRec {
            domain: c.domain,
            path: if c.path.is_empty() { "/".into() } else { c.path },
            secure: c.secure,
            http_only: c.http_only,
            expires: c.expires,
            name: c.name,
            value: c.value,
        })
        .collect())
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
        if let Some(old) = all.iter_mut().find(|o| {
            o.name == c.name && o.domain == c.domain && o.path == c.path
        }) {
            *old = c.clone();
        } else {
            all.push(c.clone());
        }
    }
}

fn cookie_header_for(cookies: &[CookieRec], host: &str, path: &str, https: bool) -> Option<String> {
    let now = now_unix();
    let mut parts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in cookies {
        if c.name.is_empty() {
            continue;
        }
        if let Some(exp) = c.expires {
            if exp > 0 && exp < now {
                continue;
            }
        }
        if c.secure && !https {
            continue;
        }
        if !c.domain.is_empty() && !domain_matches(&c.domain, host) {
            continue;
        }
        if !path_matches(&c.path, path) {
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
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "# Netscape HTTP Cookie File")?;
    writeln!(f, "# written by rxt http")?;
    for c in cookies {
        let flag = if c.domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let domain = if c.http_only {
            format!("#HttpOnly_{}", c.domain)
        } else {
            c.domain.clone()
        };
        writeln!(
            f,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            domain,
            flag,
            if c.path.is_empty() { "/" } else { &c.path },
            if c.secure { "TRUE" } else { "FALSE" },
            c.expires.unwrap_or(0),
            c.name,
            c.value
        )?;
    }
    Ok(())
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
        let (k, v) = p.split_once('=').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((p, ""));
        match k.to_ascii_lowercase().as_str() {
            "domain" => {
                rec.domain = if v.starts_with('.') { v.to_string() } else { format!(".{v}") };
            }
            "path" => rec.path = if v.is_empty() { "/".into() } else { v.to_string() },
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
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"(?i)(?:href|src)\s*=\s*["']([^"']+)["']"#).unwrap());
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in re.captures_iter(html) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if raw.is_empty() || raw.starts_with("javascript:") || raw.starts_with("data:") || raw.starts_with('#') {
            continue;
        }
        let abs = resolve_url(base, raw);
        if seen.insert(abs.clone()) {
            out.push(abs);
        }
    }
    out
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if let Some(rest) = href.strip_prefix("//") {
        let scheme = if base.starts_with("https") { "https" } else { "http" };
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

fn origin_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let hostport = rest.split('/').next()?;
    let scheme = url.split("://").next()?;
    Some(format!("{scheme}://{hostport}"))
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let hostport = rest.split(['/', '?', '#']).next()?;
    let host = hostport.rsplit_once('@').map(|(_, h)| h).unwrap_or(hostport);
    Some(host.split(':').next().unwrap_or(host).to_ascii_lowercase())
}

fn path_of(url: &str) -> &str {
    let Some(rest) = url.split("://").nth(1) else {
        return "/";
    };
    match rest.find('/') {
        Some(i) => {
            let p = rest[i..].split(['?', '#']).next().unwrap_or("/");
            if p.is_empty() { "/" } else { p }
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
    let cp = if cookie_path.is_empty() { "/" } else { cookie_path };
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
    if c.contains("json") || c.contains("text/") || c.contains("xml") || c.contains("javascript") || c.contains("html") {
        return true;
    }
    if c.contains("octet-stream") || c.contains("image/") || c.contains("audio/") || c.contains("video/") || c.contains("font/") {
        return false;
    }
    let n = bytes.len().min(512);
    n > 0 && bytes[..n].iter().filter(|b| **b == 0).count() == 0
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
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
    fn host_parse() {
        assert_eq!(host_of("https://www.GitHub.com/foo").as_deref(), Some("www.github.com"));
        assert_eq!(path_of("https://x.com/a/b?q=1"), "/a/b");
        assert_eq!(origin_of("https://x.com:8443/a").as_deref(), Some("https://x.com:8443"));
        assert_eq!(domain_candidates("example.com"), vec!["example.com".to_string()]);
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
        let c = parse_set_cookie("sid=xyz; Path=/app; Domain=ex.com; Secure; HttpOnly; Max-Age=60", "www.ex.com").unwrap();
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "xyz");
        assert_eq!(c.path, "/app");
        assert!(c.secure && c.http_only);
        assert!(c.domain.contains("ex.com"));
    }

    #[test]
    fn cookie_header_filters() {
        let cookies = vec![
            CookieRec { domain: ".ex.com".into(), path: "/".into(), secure: true, http_only: false, expires: None, name: "a".into(), value: "1".into() },
            CookieRec { domain: "other.com".into(), path: "/".into(), secure: false, http_only: false, expires: None, name: "b".into(), value: "2".into() },
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
}
