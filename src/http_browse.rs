//! CLI 网页会话 — 打开 / 读 / 点 / 填。不跑无头浏览器。
//!
//! rxt http open URL
//! rxt http snap
//! rxt http read [@e1 | --select h1]
//! rxt http fill @e2 value
//! rxt http click @e3
//! rxt http attr @e1 href
//! rxt http submit

use super::*;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const PAGE_CMDS: &[&str] = &[
    "OPEN", "GO", "NAV", "SNAP", "SNAPSHOT", "CLICK", "FILL", "READ", "SHOW", "ATTR", "SUBMIT",
    "EVAL", "JS", "NET", "WAIT", "CLOSE", "STORAGE", "HOLD", "IMPORT", "AUTH", "SSO", "IDENT",
];

pub(super) fn is_page_cmd(method: &str) -> bool {
    PAGE_CMDS.contains(&method)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PageRef {
    id: String,
    role: String,
    name: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    form: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    #[serde(default)]
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PageMeta {
    url: String,
    title: String,
    status: u16,
}

pub(super) fn run(opts: &HttpOpts<'_>, method: &str, args: &[String]) -> anyhow::Result<()> {
    let dir = session_dir(opts.session);
    fs::create_dir_all(&dir)?;
    let jar_owned = dir.join("cookies.txt");
    let jar_path: &Path = opts.cookie_jar.unwrap_or(jar_owned.as_path());
    if method == "HOLD" {
        return super::cdp::hold_loop(&dir);
    }
    let js = super::cdp::engine_wanted(opts);
    if js {
        return run_js(opts, method, args, &dir, jar_path);
    }
    match method {
        "OPEN" | "GO" | "NAV" => {
            let url = args.first().ok_or_else(|| {
                anyhow::anyhow!("需要 URL，例如: rxt http open https://example.com")
            })?;
            if !is_http_url(url) {
                anyhow::bail!("URL 必须以 http:// 或 https:// 开头: {url}");
            }
            open(opts, &dir, jar_path, url)
        }
        "SNAP" | "SNAPSHOT" => snap(opts, &dir),
        "READ" | "SHOW" => read_cmd(opts, &dir, args),
        "ATTR" => attr_cmd(opts, &dir, args),
        "FILL" => fill_cmd(&dir, args),
        "CLICK" => click_cmd(opts, &dir, jar_path, args),
        "SUBMIT" => submit_cmd(opts, &dir, jar_path, args),
        "IMPORT" => import_cmd(opts, &dir, args),
        "AUTH" | "IDENT" | "SSO" => {
            if method == "SSO" {
                sso_cmd(opts, &dir, jar_path, args)
            } else {
                super::print_identity(opts, args.first().map(|s| s.as_str()))
            }
        }
        "EVAL" | "JS" | "NET" | "WAIT" | "STORAGE" | "CLOSE" => anyhow::bail!(
            "这个命令需要 JS 引擎。安装 ~/.rxt/lib/lightpanda 或去掉 RXT_HTTP_ENGINE=static"
        ),
        _ => anyhow::bail!("未知页面命令: {method}"),
    }
}

fn sso_cmd(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    args: &[String],
) -> anyhow::Result<()> {
    if let Some(url) = args.first() {
        if is_http_url(url) {
            if super::cdp::engine_wanted(opts) {
                super::cdp::open_js(opts, dir, jar, url)?;
            } else {
                open(opts, dir, jar, url)?;
            }
        }
    }
    super::print_identity(opts, args.first().map(|s| s.as_str()))
}

fn import_cmd(opts: &HttpOpts<'_>, dir: &Path, args: &[String]) -> anyhow::Result<()> {
    let host = args.first().map(|s| domain_from_input(s));
    let (src, mut recs) = gather_cookies(opts, host.as_deref())?;
    if recs.is_empty() && opts.browser.is_none() && opts.cookie_json.is_none() && opts.cookies.is_empty() {
        anyhow::bail!(
            "import 需要来源：--browser firefox|chrome|edge|brave|auto  或  --cookie-json file.json"
        );
    }
    if let Some(h) = &host {
        recs.retain(|c| domain_matches(&c.domain, h));
    }
    let jar = dir.join("cookies.txt");
    if jar.exists() {
        let old = load_netscape(&jar).unwrap_or_default();
        recs = merge_cookies(&old, &recs, &[]);
    }
    persist_login(dir, &recs, &src)?;
    let _ = super::cdp::inject_session_cookies(dir, &recs);
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "from": src,
                "cookies": recs.len(),
                "dir": dir.display().to_string(),
            }))?
        );
    } else {
        println!(
            "OK import {} 条 from {src} → {} (cookies.txt / cookies.json / login.json)",
            recs.len(),
            dir.display()
        );
    }
    Ok(())
}

fn run_js(
    opts: &HttpOpts<'_>,
    method: &str,
    args: &[String],
    dir: &Path,
    jar: &Path,
) -> anyhow::Result<()> {
    use super::cdp;
    match method {
        "OPEN" | "GO" | "NAV" => {
            let url = args.first().ok_or_else(|| {
                anyhow::anyhow!("需要 URL，例如: rxt http open https://example.com")
            })?;
            if !is_http_url(url) {
                anyhow::bail!("URL 必须以 http:// 或 https:// 开头: {url}");
            }
            cdp::open_js(opts, dir, jar, url)?;
            after_js(opts, dir)
        }
        "SNAP" | "SNAPSHOT" => {
            cdp::refresh(opts, dir, jar)?;
            snap(opts, dir)
        }
        "READ" | "SHOW" => {
            cdp::refresh(opts, dir, jar)?;
            read_cmd(opts, dir, args)
        }
        "ATTR" => attr_cmd(opts, dir, args),
        "FILL" => {
            let sel = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("需要 @eN，例如: rxt http fill @e2 hello"))?;
            let id = parse_ref_id(sel).ok_or_else(|| anyhow::anyhow!("fill 需要 @eN"))?;
            let value = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                String::new()
            };
            cdp::fill_js(dir, &id, &value)?;
            println!("OK fill {id}");
            Ok(())
        }
        "CLICK" => {
            let sel = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("需要 @eN，例如: rxt http click @e1"))?;
            let id = parse_ref_id(sel).ok_or_else(|| anyhow::anyhow!("click 需要 @eN"))?;
            cdp::click_js(dir, &id)?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            cdp::refresh(opts, dir, jar)?;
            after_js(opts, dir)
        }
        "SUBMIT" => {
            // 点第一个 button
            let refs = load_refs(dir)?;
            let btn = refs
                .iter()
                .find(|r| r.role == "button")
                .ok_or_else(|| anyhow::anyhow!("没有 button"))?;
            cdp::click_js(dir, &btn.id)?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            cdp::refresh(opts, dir, jar)?;
            after_js(opts, dir)
        }
        "EVAL" | "JS" => {
            let expr = args.join(" ");
            if expr.trim().is_empty() {
                anyhow::bail!("需要 JS，例如: rxt http eval 'document.title'");
            }
            let v = cdp::eval_js(dir, &expr)?;
            if opts.json_body {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if let Some(s) = v.as_str() {
                println!("{s}");
            } else if v.is_null() {
                println!("null");
            } else {
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            Ok(())
        }
        "NET" => {
            cdp::refresh(opts, dir, jar)?;
            let p = dir.join("net.json");
            let raw = fs::read_to_string(&p).unwrap_or_else(|_| "[]".into());
            println!("{raw}");
            Ok(())
        }
        "STORAGE" => {
            cdp::refresh(opts, dir, jar)?;
            let raw = fs::read_to_string(dir.join("storage.json")).unwrap_or_else(|_| "{}".into());
            println!("{raw}");
            Ok(())
        }
        "WAIT" => {
            let spec = args.join(" ");
            if spec.is_empty() {
                anyhow::bail!("rxt http wait 'document.querySelector(\"#app\")'");
            }
            let expr = if spec.starts_with("document.") || spec.contains('(') {
                spec
            } else {
                format!("!!document.querySelector({:?})", spec)
            };
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_secs(opts.timeout.max(3));
            loop {
                let v = cdp::eval_js(dir, &expr)?;
                let ok = match &v {
                    serde_json::Value::Bool(b) => *b,
                    serde_json::Value::Null => false,
                    serde_json::Value::String(s) => !s.is_empty(),
                    other => !other.is_null(),
                };
                if ok {
                    println!("OK wait");
                    return cdp::refresh(opts, dir, jar);
                }
                if std::time::Instant::now() > deadline {
                    anyhow::bail!("wait 超时: {expr}");
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        "IMPORT" => import_cmd(opts, dir, args),
        "AUTH" | "IDENT" => super::print_identity(opts, args.first().map(|s| s.as_str())),
        "SSO" => sso_cmd(opts, dir, jar, args),
        "CLOSE" => cdp::close_js(),
        "HOLD" => super::cdp::hold_loop(dir),
        _ => anyhow::bail!("未知页面命令: {method}"),
    }
}

fn after_js(opts: &HttpOpts<'_>, dir: &Path) -> anyhow::Result<()> {
    let meta = load_meta(dir)?;
    let refs = load_refs(dir).unwrap_or_default();
    let net_n = fs::read_to_string(dir.join("net.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0);
    if opts.json_body {
        let html = load_html(dir).unwrap_or_default();
        let net: serde_json::Value = fs::read_to_string(dir.join("net.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!([]));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "engine": "js",
                "url": meta.url,
                "title": meta.title,
                "status": meta.status,
                "refs": refs,
                "net": net,
                "text": apply_budget(opts.budget.or(Some(2000)), &html_to_text(&html)),
            }))?
        );
        return Ok(());
    }
    eprintln!("JS  {}  net={net_n}", meta.url);
    print_snap(opts, &meta, &refs)?;
    if opts.text {
        let html = load_html(dir)?;
        println!();
        print_out(&apply_budget(opts.budget.or(Some(2000)), &html_to_text(&html)));
    }
    if net_n > 0 {
        eprintln!("# XHR/fetch {net_n} 条。看: rxt http net");
    }
    Ok(())
}

pub(super) fn session_dir(name: Option<&str>) -> PathBuf {
    if let Ok(p) = std::env::var("RXT_HTTP_SESSION_DIR") {
        let p = p.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let n = name
        .map(|s| s.to_string())
        .or_else(|| std::env::var("RXT_HTTP_SESSION").ok())
        .unwrap_or_else(|| "default".into());
    let safe: String = n
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
        .join("http-session")
        .join(if safe.is_empty() {
            "default".into()
        } else {
            safe
        })
}

fn open(opts: &HttpOpts<'_>, dir: &Path, jar: &Path, url: &str) -> anyhow::Result<()> {
    let out = request_one(opts, url, "GET", &[], Some(jar));
    if let Some(err) = &out.error {
        anyhow::bail!("打开失败: {err}");
    }
    save_page(dir, jar, &out)?;
    let html = String::from_utf8_lossy(&out.bytes);
    let refs = build_refs(&out.url, &html);
    save_refs(dir, &refs)?;
    save_draft(dir, &BTreeMap::new())?;
    print_after_load(opts, &out, &refs, &html)
}

fn snap(opts: &HttpOpts<'_>, dir: &Path) -> anyhow::Result<()> {
    let meta = load_meta(dir)?;
    let refs = load_refs(dir)?;
    print_snap(opts, &meta, &refs)
}

fn read_cmd(opts: &HttpOpts<'_>, dir: &Path, args: &[String]) -> anyhow::Result<()> {
    let html = load_html(dir)?;
    let meta = load_meta(dir)?;
    let sel = args.first().map(|s| s.as_str()).or(opts.select);
    let text = if let Some(sel) = sel {
        if let Some(id) = parse_ref_id(sel) {
            let refs = load_refs(dir)?;
            let r = find_ref(&refs, &id)?;
            r.text.clone()
        } else {
            extract_select(&html, sel).join("\n")
        }
    } else {
        html_to_text(&html)
    };
    let out = apply_budget(opts.budget, &text);
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": meta.url,
                "select": sel,
                "text": out,
            }))?
        );
    } else {
        print_out(&out);
    }
    Ok(())
}

fn attr_cmd(opts: &HttpOpts<'_>, dir: &Path, args: &[String]) -> anyhow::Result<()> {
    let sel = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("需要选择器，例如: rxt http attr @e1 href"))?;
    let key = args.get(1).map(|s| s.as_str()).unwrap_or("href");
    let id = parse_ref_id(sel).ok_or_else(|| anyhow::anyhow!("attr 需要 @eN"))?;
    let refs = load_refs(dir)?;
    let r = find_ref(&refs, &id)?;
    let val = match key {
        "href" => r.href.clone().unwrap_or_default(),
        "name" => r.name.clone(),
        "value" => r.value.clone(),
        "role" => r.role.clone(),
        "action" => r.action.clone().unwrap_or_default(),
        "method" => r.method.clone().unwrap_or_default(),
        "text" => r.text.clone(),
        "field" => r.field.clone().unwrap_or_default(),
        other => anyhow::bail!("未知属性: {other}（href|name|value|role|action|method|text|field）"),
    };
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ref": r.id,
                "attr": key,
                "value": val,
            }))?
        );
    } else {
        println!("{val}");
    }
    Ok(())
}

fn fill_cmd(dir: &Path, args: &[String]) -> anyhow::Result<()> {
    let sel = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("需要选择器，例如: rxt http fill @e2 hello"))?;
    let value = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        String::new()
    };
    let id = parse_ref_id(sel).ok_or_else(|| anyhow::anyhow!("fill 需要 @eN"))?;
    let refs = load_refs(dir)?;
    let r = find_ref(&refs, &id)?;
    let field = r
        .field
        .clone()
        .ok_or_else(|| anyhow::anyhow!("@{} 不是输入框（role={}）", r.id, r.role))?;
    let mut draft = load_draft(dir)?;
    draft.insert(field.clone(), value.clone());
    save_draft(dir, &draft)?;
    println!("OK fill {} {}={}", r.id, field, value);
    Ok(())
}

fn click_cmd(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    args: &[String],
) -> anyhow::Result<()> {
    let sel = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("需要选择器，例如: rxt http click @e1"))?;
    let id = parse_ref_id(sel).ok_or_else(|| anyhow::anyhow!("click 需要 @eN"))?;
    let refs = load_refs(dir)?;
    let r = find_ref(&refs, &id)?;
    match r.role.as_str() {
        "link" => {
            let href = r
                .href
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("@{} 没有 href", r.id))?;
            follow(opts, dir, jar, href, "GET", &[])
        }
        "button" | "submit" => submit_form(opts, dir, jar, r.form),
        "checkbox" => {
            let field = r
                .field
                .clone()
                .ok_or_else(|| anyhow::anyhow!("@{} 没有 name", r.id))?;
            let mut draft = load_draft(dir)?;
            let cur = draft.get(&field).cloned().unwrap_or_else(|| r.value.clone());
            let next = if cur == "on" || cur == "true" || cur == "1" {
                ""
            } else {
                "on"
            };
            draft.insert(field.clone(), next.to_string());
            save_draft(dir, &draft)?;
            println!("OK check {} {}={}", r.id, field, next);
            Ok(())
        }
        other => anyhow::bail!("@{} 不能点（role={other}）。link/button 才能 click", r.id),
    }
}

fn submit_cmd(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    args: &[String],
) -> anyhow::Result<()> {
    let form = if let Some(sel) = args.first() {
        if let Some(id) = parse_ref_id(sel) {
            let refs = load_refs(dir)?;
            find_ref(&refs, &id)?.form
        } else {
            sel.parse::<usize>().ok()
        }
    } else {
        Some(0)
    };
    submit_form(opts, dir, jar, form)
}

fn submit_form(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    form_idx: Option<usize>,
) -> anyhow::Result<()> {
    let html = load_html(dir)?;
    let meta = load_meta(dir)?;
    let forms = parse_forms(&meta.url, &html);
    if forms.is_empty() {
        anyhow::bail!("当前页没有 <form>，SPA 请改用 API（rxt http scan）");
    }
    let idx = form_idx.unwrap_or(0);
    let form = forms.get(idx).ok_or_else(|| {
        anyhow::anyhow!("没有 form {idx}（本页 {} 个表单）", forms.len())
    })?;
    let draft = load_draft(dir)?;
    let mut pairs: Vec<String> = Vec::new();
    for f in &form.fields {
        let val = draft.get(&f.name).cloned().unwrap_or_else(|| f.value.clone());
        pairs.push(format!("{}={val}", f.name));
    }
    for extra in opts.form {
        pairs.push(extra.clone());
    }
    let verb = if form.method == "GET" { "GET" } else { "POST" };
    let mut action = form.action.clone();
    if verb == "GET" && !pairs.is_empty() {
        action = append_query(&action, &pairs);
        follow(opts, dir, jar, &action, "GET", &[])
    } else {
        follow(opts, dir, jar, &action, verb, &pairs)
    }
}

fn follow(
    opts: &HttpOpts<'_>,
    dir: &Path,
    jar: &Path,
    url: &str,
    verb: &str,
    form: &[String],
) -> anyhow::Result<()> {
    let out = request_one(opts, url, verb, form, Some(jar));
    if let Some(err) = &out.error {
        anyhow::bail!("请求失败: {err}");
    }
    save_page(dir, jar, &out)?;
    let html = String::from_utf8_lossy(&out.bytes);
    let refs = build_refs(&out.url, &html);
    save_refs(dir, &refs)?;
    save_draft(dir, &BTreeMap::new())?;
    print_after_load(opts, &out, &refs, &html)
}

fn print_after_load(
    opts: &HttpOpts<'_>,
    out: &FetchOut,
    refs: &[PageRef],
    html: &str,
) -> anyhow::Result<()> {
    let title = page_title(html);
    let meta = PageMeta {
        url: out.url.clone(),
        title: title.clone(),
        status: out.status,
    };
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": out.url,
                "status": out.status,
                "title": title,
                "refs": refs,
                "text": apply_budget(opts.budget.or(Some(2000)), &html_to_text(html)),
            }))?
        );
        return Ok(());
    }
    eprintln!("HTTP {}  {}", out.status, out.url);
    print_snap(opts, &meta, refs)?;
    if opts.text {
        println!();
        print_out(&apply_budget(opts.budget.or(Some(2000)), &html_to_text(html)));
    }
    Ok(())
}

fn print_snap(opts: &HttpOpts<'_>, meta: &PageMeta, refs: &[PageRef]) -> anyhow::Result<()> {
    if opts.json_body {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": meta.url,
                "title": meta.title,
                "status": meta.status,
                "refs": refs,
            }))?
        );
        return Ok(());
    }
    println!("URL: {}", meta.url);
    if !meta.title.is_empty() {
        println!("Title: {}", meta.title);
    }
    if refs.is_empty() {
        println!("(无可交互元素)");
        return Ok(());
    }
    println!("可交互 ({}):", refs.len());
    for r in refs {
        let extra = r
            .href
            .as_ref()
            .or(r.field.as_ref())
            .map(|s| format!("  {s}"))
            .unwrap_or_default();
        let label = if r.text.is_empty() {
            r.name.clone()
        } else {
            r.text.clone()
        };
        println!("  {:4} {:8} {}{}", r.id, r.role, label, extra);
    }
    Ok(())
}

fn build_refs(base: &str, html: &str) -> Vec<PageRef> {
    let mut refs = Vec::new();
    let mut n = 0usize;
    let forms = parse_forms(base, html);
    for (fi, form) in forms.iter().enumerate() {
        for f in &form.fields {
            n += 1;
            let role = match f.kind.as_str() {
                "checkbox" => "checkbox",
                "hidden" => "hidden",
                "password" => "textbox",
                "radio" => "radio",
                _ => "textbox",
            };
            if role == "hidden" {
                continue;
            }
            refs.push(PageRef {
                id: format!("e{n}"),
                role: role.into(),
                name: f.name.clone(),
                text: f.name.clone(),
                href: None,
                form: Some(fi),
                field: Some(f.name.clone()),
                value: f.value.clone(),
                action: Some(form.action.clone()),
                method: Some(form.method.clone()),
            });
        }
        n += 1;
        refs.push(PageRef {
            id: format!("e{n}"),
            role: "button".into(),
            name: form.name.clone().unwrap_or_else(|| format!("form{fi}")),
            text: form
                .name
                .clone()
                .unwrap_or_else(|| "submit".into()),
            href: None,
            form: Some(fi),
            field: None,
            value: String::new(),
            action: Some(form.action.clone()),
            method: Some(form.method.clone()),
        });
    }
    let links = extract_nav_links(base, html);
    for href in links {
        n += 1;
        let name = link_label(html, &href).unwrap_or_else(|| href.clone());
        refs.push(PageRef {
            id: format!("e{n}"),
            role: "link".into(),
            name: name.clone(),
            text: name,
            href: Some(href),
            form: None,
            field: None,
            value: String::new(),
            action: None,
            method: None,
        });
        if refs.len() >= 40 {
            break;
        }
    }
    refs
}

fn link_label(html: &str, abs: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"(?is)<a\b([^>]*)>(.*?)</a>"#).unwrap());
    for cap in re.captures_iter(html) {
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let href = html_attr(attrs, "href")?;
        // 相对/绝对都对一下太贵，只比后缀
        if abs.ends_with(href.trim_start_matches('.')) || abs.contains(&href) {
            let t = html_to_text(inner).trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn page_title(html: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
    re.captures(html)
        .and_then(|c| c.get(1))
        .map(|m| html_to_text(m.as_str()).trim().to_string())
        .unwrap_or_default()
}

pub(super) fn extract_select(html: &str, sel: &str) -> Vec<String> {
    let sel = sel.trim();
    if sel.eq_ignore_ascii_case("table") {
        return extract_tables(html);
    }
    if let Some(id) = sel.strip_prefix('#') {
        return extract_tag_attr(html, "id", id);
    }
    if let Some(cls) = sel.strip_prefix('.') {
        return extract_class(html, cls);
    }
    if let Some(inner) = sel.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some((k, v)) = inner.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            return extract_tag_attr(html, k.trim(), v);
        }
    }
    extract_tag(html, sel)
}

fn extract_tag(html: &str, tag: &str) -> Vec<String> {
    let tag = regex::escape(tag);
    let re = regex::Regex::new(&format!(r"(?is)<{tag}\b[^>]*>(.*?)</{tag}>")).ok();
    let Some(re) = re else {
        return Vec::new();
    };
    re.captures_iter(html)
        .filter_map(|c| c.get(1))
        .map(|m| html_to_text(m.as_str()).trim().to_string())
        .filter(|s| !s.is_empty())
        .take(40)
        .collect()
}

fn extract_tag_attr(html: &str, attr: &str, val: &str) -> Vec<String> {
    let re = regex::Regex::new(&format!(
        r#"(?is)<([a-zA-Z0-9]+)([^>]*\b{}\s*=\s*["']?{}["']?[^>]*)>(.*?)</"#,
        regex::escape(attr),
        regex::escape(val)
    ));
    let Ok(re) = re else {
        return Vec::new();
    };
    re.captures_iter(html)
        .filter_map(|c| c.get(3))
        .map(|m| html_to_text(m.as_str()).trim().to_string())
        .filter(|s| !s.is_empty())
        .take(40)
        .collect()
}

fn extract_class(html: &str, cls: &str) -> Vec<String> {
    extract_tag_attr(html, "class", cls)
}

fn extract_tables(html: &str) -> Vec<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?is)<table\b[^>]*>(.*?)</table>").unwrap());
    let mut out = Vec::new();
    for cap in re.captures_iter(html).take(8) {
        let body = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let rows = table_rows(body);
        if rows.is_empty() {
            continue;
        }
        if let Ok(js) = serde_json::to_string(&rows) {
            out.push(js);
        }
    }
    out
}

fn table_rows(table_html: &str) -> Vec<serde_json::Value> {
    static RE_TR: OnceLock<regex::Regex> = OnceLock::new();
    static RE_CELL: OnceLock<regex::Regex> = OnceLock::new();
    let re_tr = RE_TR.get_or_init(|| regex::Regex::new(r"(?is)<tr\b[^>]*>(.*?)</tr>").unwrap());
    let re_cell =
        RE_CELL.get_or_init(|| regex::Regex::new(r"(?is)<t[dh]\b[^>]*>(.*?)</t[dh]>").unwrap());
    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in re_tr.captures_iter(table_html) {
        let body = tr.get(1).map(|m| m.as_str()).unwrap_or("");
        let cells: Vec<String> = re_cell
            .captures_iter(body)
            .filter_map(|c| c.get(1))
            .map(|m| html_to_text(m.as_str()).trim().to_string())
            .collect();
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    if rows.len() < 2 {
        return rows
            .into_iter()
            .map(|r| serde_json::json!(r))
            .collect();
    }
    let headers = &rows[0];
    rows.iter()
        .skip(1)
        .map(|r| {
            let mut obj = serde_json::Map::new();
            for (i, h) in headers.iter().enumerate() {
                obj.insert(
                    h.clone(),
                    serde_json::Value::String(r.get(i).cloned().unwrap_or_default()),
                );
            }
            serde_json::Value::Object(obj)
        })
        .collect()
}

fn parse_ref_id(s: &str) -> Option<String> {
    let s = s.trim().trim_start_matches('@');
    if s.is_empty() {
        return None;
    }
    if s.starts_with('e')
        && s[1..].chars().all(|c| c.is_ascii_digit())
        && s.len() > 1
    {
        return Some(s.to_string());
    }
    None
}

fn find_ref<'a>(refs: &'a [PageRef], id: &str) -> anyhow::Result<&'a PageRef> {
    refs.iter()
        .find(|r| r.id == id)
        .ok_or_else(|| anyhow::anyhow!("没有 @{id}。先 rxt http snap"))
}

fn save_page(dir: &Path, jar: &Path, out: &FetchOut) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("page.html"), &out.bytes)?;
    let title = page_title(&String::from_utf8_lossy(&out.bytes));
    let meta = PageMeta {
        url: out.url.clone(),
        title,
        status: out.status,
    };
    fs::write(dir.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
    save_netscape(jar, &out.merged_cookies)?;
    Ok(())
}

fn save_refs(dir: &Path, refs: &[PageRef]) -> anyhow::Result<()> {
    fs::write(dir.join("refs.json"), serde_json::to_vec_pretty(refs)?)?;
    Ok(())
}

fn save_draft(dir: &Path, draft: &BTreeMap<String, String>) -> anyhow::Result<()> {
    fs::write(dir.join("draft.json"), serde_json::to_vec_pretty(draft)?)?;
    Ok(())
}

fn load_html(dir: &Path) -> anyhow::Result<String> {
    fs::read_to_string(dir.join("page.html")).map_err(|_| {
        anyhow::anyhow!(
            "没有打开的页面 ({})。先 rxt http open <URL>",
            dir.display()
        )
    })
}

fn load_meta(dir: &Path) -> anyhow::Result<PageMeta> {
    let raw = fs::read_to_string(dir.join("meta.json")).map_err(|_| {
        anyhow::anyhow!(
            "没有打开的页面 ({})。先 rxt http open <URL>",
            dir.display()
        )
    })?;
    Ok(serde_json::from_str(&raw)?)
}

fn load_refs(dir: &Path) -> anyhow::Result<Vec<PageRef>> {
    let raw = fs::read_to_string(dir.join("refs.json")).map_err(|_| {
        anyhow::anyhow!(
            "没有打开的页面 ({})。先 rxt http open <URL>",
            dir.display()
        )
    })?;
    Ok(serde_json::from_str(&raw)?)
}

fn load_draft(dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let p = dir.join("draft.json");
    if !p.exists() {
        return Ok(BTreeMap::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(p)?)?)
}

fn apply_budget(budget: Option<usize>, s: &str) -> String {
    match budget {
        Some(n) if s.len() > n => format!(
            "{}…\n# truncated {}/{} chars",
            &s[..s.floor_char_boundary(n)],
            n,
            s.len()
        ),
        _ => s.to_string(),
    }
}

fn print_out(s: &str) {
    print!("{s}");
    if !s.ends_with('\n') {
        println!();
    }
}

fn append_query(url: &str, pairs: &[String]) -> String {
    let mut q = String::new();
    for p in pairs {
        if let Some((k, v)) = p.split_once('=') {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str(&urlencoding::encode(k));
            q.push('=');
            q.push_str(&urlencoding::encode(v));
        }
    }
    if q.is_empty() {
        return url.to_string();
    }
    if url.contains('?') {
        format!("{url}&{q}")
    } else {
        format!("{url}?{q}")
    }
}
