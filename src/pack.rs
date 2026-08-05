//! pack — AI 一键项目简报（省调用次数 + 省上下文）
//!
//! 把 map 树 + 优先文件 digest + 可选 focus 搜索 压成 **一条命令、一个硬预算**。
//! Agent 第一次进仓库：用 `rxt pack .` 替代 list_dir×N + read×N + grep×N。
//!
//! 用法:
//!   rxt pack                         # 当前目录，默认 budget=6000 字符
//!   rxt pack ./backend --budget 4000
//!   rxt pack . --focus poster -d 2
//!   rxt pack . --json
//!   rxt --host huhu pack /home/huhu/torrent-panel-v2

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use serde_json::json;

/// 粗略 token 估计：中英混排约 3.5 字符 ≈ 1 token
fn est_tokens(chars: usize) -> usize {
    (chars as f64 / 3.5).ceil() as usize
}

/// 在 char 边界截断，避免 UTF-8 panic
fn truncate_chars(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

/// 入口点/高价值文件名启发式（排在 digest 前面）
fn entry_score(rel: &str) -> i32 {
    let name = Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let path_l = rel.replace('\\', "/").to_ascii_lowercase();
    let mut s = 0i32;
    // 文件名
    for (pat, w) in [
        ("main.", 100),
        ("app.", 90),
        ("index.", 85),
        ("mod.rs", 80),
        ("lib.rs", 80),
        ("router", 70),
        ("routes", 70),
        ("api.", 65),
        ("server.", 60),
        ("config.", 50),
        ("models.", 45),
        ("utils.", 40),
        ("types.", 40),
        ("schema", 35),
        ("player", 30),
        ("dashboard", 30),
    ] {
        if name.contains(pat) || path_l.contains(pat) {
            s += w;
        }
    }
    // 降权测试/生成物
    for (pat, w) in [
        ("test", -80),
        ("spec.", -80),
        ("__pycache__", -200),
        (".min.", -100),
        ("node_modules", -200),
        ("dist/", -100),
        ("build/", -80),
        ("vendor/", -80),
        ("mock", -40),
    ] {
        if path_l.contains(pat) {
            s += w;
        }
    }
    s
}

struct FileSkeleton {
    rel: String,
    lines: usize,
    symbols: Vec<String>, // 已格式化的单行
    score: i32,
}

pub fn run(
    dir: &Path,
    budget: usize,
    depth: usize,
    focus: Option<&str>,
    max_files: Option<usize>,
    per_file: usize,
    threshold: usize,
    no_tree: bool,
    no_digest: bool,
    json_output: bool,
    remote: Option<&mut crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    // 中文输出 / Agent 管道捕获：先设 UTF-8，再可选 BOM
    crate::common::setup_utf8_console();

    // 远程：优先让远端 rxt 自己算，结果一次回传（省本地拉代码）
    if let Some(rc) = remote {
        let path_s = dir.display().to_string();
        let mut args: Vec<String> = vec![
            "pack".into(),
            path_s,
            "--budget".into(),
            budget.to_string(),
            "-d".into(),
            depth.to_string(),
            "--per-file".into(),
            per_file.to_string(),
            "-t".into(),
            threshold.to_string(),
        ];
        if let Some(f) = focus {
            args.push("--focus".into());
            args.push(f.to_string());
        }
        if let Some(m) = max_files {
            args.push("--max-files".into());
            args.push(m.to_string());
        }
        if no_tree {
            args.push("--no-tree".into());
        }
        if no_digest {
            args.push("--no-digest".into());
        }
        if json_output {
            args.push("--json".into());
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        if let Some(out) = rc.try_exec_rxt(&arg_refs) {
            let mut stdout = std::io::stdout().lock();
            let _ = crate::common::maybe_write_bom(&mut stdout);
            use std::io::Write;
            let _ = write!(stdout, "{}", out);
            if !out.ends_with('\n') {
                let _ = writeln!(stdout);
            }
            return Ok(());
        }
        // 远端无 rxt：降级提示（不拉全仓库）
        anyhow::bail!(
            "远端无 rxt，无法 pack。请先在目标机安装 rxt 0.8.2+，或本地同步代码后 pack。"
        );
    }

    let root = crate::common::safe_resolve(dir);
    if !root.is_dir() {
        anyhow::bail!("不是目录: {}", root.display());
    }

    let kind = crate::common::detect_kind(&root);
    let kind_s = kind
        .as_ref()
        .map(|k| k.kind.clone())
        .unwrap_or_else(|| "unknown".into());
    let name_s = kind.as_ref().map(|k| k.name.clone()).unwrap_or_else(|| {
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".")
            .to_string()
    });

    // ---- 源文件 ----
    let mut files: Vec<PathBuf> = crate::common::walk_clean(&root, None, None)
        .into_iter()
        .filter(|p| crate::langs::is_supported(p))
        .filter(|p| {
            let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            !(n.ends_with(".bak") || n.contains(".bak.") || n.ends_with(".orig") || n.ends_with("~"))
        })
        .collect();
    files.sort();

    let mut total_loc = 0usize;
    let mut skeletons: Vec<FileSkeleton> = Vec::new();

    if !no_digest {
        for f in &files {
            let content = match std::fs::read_to_string(f) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lines: Vec<&str> = content.lines().collect();
            let nlines = lines.len();
            total_loc += nlines;
            let symbols = match crate::langs::extract_symbols(f, &content) {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };
            let rel = f
                .strip_prefix(&root)
                .map(|r| r.display().to_string())
                .unwrap_or_else(|_| f.display().to_string())
                .replace('\\', "/");
            let mut rows: Vec<(i32, String)> = Vec::new();
            for s in &symbols {
                let body = crate::digest::count_body(&lines, s.line, &s.kind);
                let folded = body > threshold;
                let sig = s.signature.trim_end_matches('{').trim().to_string();
                let row = if folded {
                    format!("{:>4} {} ·{}L", s.line, compact_sig(&sig), body)
                } else {
                    format!("{:>4} {}", s.line, compact_sig(&sig))
                };
                // 优先展示 fn/class/export 级
                let kind_w = match s.kind.as_str() {
                    "fn" | "function" | "def" | "method" => 3,
                    "struct" | "class" | "interface" | "type" | "enum" | "trait" => 4,
                    _ => 1,
                };
                rows.push((kind_w, row));
            }
            // 类型/函数优先，再截断 per_file
            rows.sort_by(|a, b| b.0.cmp(&a.0));
            rows.truncate(per_file);
            let score = entry_score(&rel) + (symbols.len() as i32).min(30);
            skeletons.push(FileSkeleton {
                rel,
                lines: nlines,
                symbols: rows.into_iter().map(|(_, r)| r).collect(),
                score,
            });
        }
        skeletons.sort_by(|a, b| match b.score.cmp(&a.score) {
            Ordering::Equal => a.rel.cmp(&b.rel),
            o => o,
        });
    } else {
        for f in &files {
            if let Ok(c) = std::fs::read_to_string(f) {
                total_loc += c.lines().count();
            }
        }
    }

    let file_cap = max_files.unwrap_or_else(|| {
        // 按预算自适应：6000 字符大约塞 8~15 个文件骨架
        ((budget / 450).max(4)).min(24)
    });

    // ---- 预算切片：skeleton 优先（AI 最需要），tree/focus 吃剩余 ----
    // 页眉+页脚约 200；focus 最多 20%；tree 最多 25%；其余给 skeleton
    let footer_reserve = 180usize;
    let focus_budget = if focus.is_some() {
        (budget / 5).clamp(200, 900)
    } else {
        0
    };
    let tree_budget = if no_tree {
        0
    } else {
        (budget / 4).clamp(200, 1200)
    };
    let skeleton_budget = budget
        .saturating_sub(footer_reserve + focus_budget + tree_budget)
        .max(budget / 2);

    // ---- 结构树（轻量 + 硬上限）----
    let tree = if no_tree {
        String::new()
    } else {
        let mut t = build_compact_tree(&root, depth, 48);
        if t.len() > tree_budget {
            truncate_chars(&mut t, tree_budget.saturating_sub(12));
            t.push_str("\n…\n");
        }
        t
    };

    // ---- focus 搜索 ----
    let focus_hits = if let Some(q) = focus {
        compact_grep(&root, q, 12)
    } else {
        Vec::new()
    };

    // ---- 拼装 + 硬预算截断 ----
    if json_output {
        let mut digest_arr = Vec::new();
        let mut used_files = 0usize;
        for sk in skeletons.iter().take(file_cap) {
            digest_arr.push(json!({
                "file": sk.rel,
                "lines": sk.lines,
                "score": sk.score,
                "symbols": sk.symbols,
            }));
            used_files += 1;
        }
        let mut out = json!({
            "cmd": "pack",
            "root": root.display().to_string(),
            "kind": kind_s,
            "name": name_s,
            "stats": {
                "files": files.len(),
                "loc": total_loc,
                "digest_files": used_files,
            },
            "tree": tree,
            "digest": digest_arr,
            "focus": focus,
            "focus_hits": focus_hits,
            "budget_chars": budget,
            "next": [
                "rxt digest <file> --budget 800",
                "rxt refs <Symbol> -p <dir> --callers",
                "rxt read <file> -H 60 -b 2000",
                "rxt grep <pat> <dir> --head 20",
            ],
        });
        // JSON 也做预算：超长则砍 digest 尾部
        let mut s = serde_json::to_string(&out)?;
        while s.len() > budget && out["digest"].as_array().map(|a| a.len()).unwrap_or(0) > 1 {
            if let Some(arr) = out["digest"].as_array_mut() {
                arr.pop();
            }
            s = serde_json::to_string(&out)?;
        }
        out["used_chars"] = json!(s.len());
        out["est_tokens"] = json!(est_tokens(s.len()));
        emit_pack_text(&serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    // 文本：先 skeleton（核心），再 tree / focus
    let mut body = String::new();
    body.push_str(&format!(
        "# pack  {}  kind={}  files={}  loc={}\n",
        root.display(),
        kind_s,
        files.len(),
        total_loc
    ));

    if !skeletons.is_empty() {
        body.push_str("## skeleton\n");
        let mut n = 0usize;
        let skel_start = body.len();
        for sk in &skeletons {
            if n >= file_cap {
                break;
            }
            let mut block = format!("▶ {} ({}L)\n", sk.rel, sk.lines);
            for row in &sk.symbols {
                block.push_str(row);
                block.push('\n');
            }
            if body.len() - skel_start + block.len() > skeleton_budget {
                body.push_str(&format!(
                    "… 截断：已列 {}/{} 优先文件（--budget↑ 或缩小目录）\n",
                    n,
                    skeletons.len()
                ));
                break;
            }
            body.push_str(&block);
            n += 1;
        }
        if n < skeletons.len() && !body.contains("截断") {
            body.push_str(&format!("… 其余 {} 文件未展开\n", skeletons.len() - n));
        }
    }

    if !tree.is_empty() {
        body.push_str("## tree\n");
        body.push_str(&tree);
        if !tree.ends_with('\n') {
            body.push('\n');
        }
    }

    if let Some(q) = focus {
        body.push_str(&format!("## focus \"{}\"\n", q));
        if focus_hits.is_empty() {
            body.push_str("(无命中)\n");
        } else {
            let mut used_f = 0usize;
            for h in &focus_hits {
                if used_f + h.len() + 1 > focus_budget {
                    body.push_str("… focus 截断\n");
                    break;
                }
                body.push_str(h);
                body.push('\n');
                used_f += h.len() + 1;
            }
        }
    }

    body.push_str("## next\n");
    body.push_str("digest <f> | refs <Sym> -p . --callers | read <f> -H 60 | grep <pat> --head 20\n");

    // 硬截断兜底（UTF-8 安全）
    if body.len() > budget {
        truncate_chars(&mut body, budget.saturating_sub(40));
        body.push_str("\n…[budget cut]\n");
    }

    let used = body.len();
    body.push_str(&format!(
        "— used {} chars (~{} tok) / budget {} | 1 call replaces map+digest+grep\n",
        used,
        est_tokens(used),
        budget
    ));
    emit_pack_text(&body);
    Ok(())
}

/// 统一 stdout：UTF-8 控制台 + Agent 管道时写 BOM
fn emit_pack_text(text: &str) {
    crate::common::setup_utf8_console();
    let mut stdout = std::io::stdout().lock();
    let _ = crate::common::maybe_write_bom(&mut stdout);
    use std::io::Write;
    let _ = write!(stdout, "{}", text);
    if !text.ends_with('\n') {
        let _ = writeln!(stdout);
    }
}

fn compact_sig(sig: &str) -> String {
    let mut s = sig.replace('\t', " ");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    // 去掉过长泛型/默认参数（UTF-8 安全）
    if s.len() > 100 {
        truncate_chars(&mut s, 97);
        s.push_str("...");
    }
    s
}

fn build_compact_tree(root: &Path, depth: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");
    out.push_str(name);
    out.push('\n');
    let ignore = crate::common::load_gitignore_pub(root);
    tree_rec(root, "", depth, 0, &ignore, &mut out, max_lines);
    out
}

fn tree_rec(
    dir: &Path,
    prefix: &str,
    max_depth: usize,
    cur: usize,
    ignore: &[String],
    out: &mut String,
    max_lines: usize,
) {
    if cur >= max_depth || out.lines().count() >= max_lines {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.flatten().collect();
    items.sort_by(|a, b| {
        let ad = a.path().is_dir();
        let bd = b.path().is_dir();
        match (ad, bd) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });
    let visible: Vec<_> = items
        .into_iter()
        .filter(|e| should_show(e, ignore))
        .collect();
    let total = visible.len();
    for (i, entry) in visible.iter().enumerate() {
        if out.lines().count() >= max_lines {
            out.push_str(prefix);
            out.push_str("…\n");
            return;
        }
        let last = i + 1 == total;
        let name = entry.file_name().to_string_lossy().into_owned();
        let branch = if last { "└─ " } else { "├─ " };
        out.push_str(prefix);
        out.push_str(branch);
        out.push_str(&name);
        out.push('\n');
        if entry.path().is_dir() {
            let ext = if last { "   " } else { "│  " };
            tree_rec(
                &entry.path(),
                &format!("{}{}", prefix, ext),
                max_depth,
                cur + 1,
                ignore,
                out,
                max_lines,
            );
        }
    }
}

fn should_show(entry: &std::fs::DirEntry, ignore: &[String]) -> bool {
    let name = entry.file_name().to_string_lossy().into_owned();
    if name.starts_with('.') {
        return name == ".github" || name == ".rxt";
    }
    // 备份/临时
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bak")
        || lower.contains(".bak.")
        || lower.ends_with(".orig")
        || lower.ends_with("~")
        || lower.ends_with(".tmp")
    {
        return false;
    }
    const SKIP: &[&str] = &[
        "node_modules",
        "target",
        "dist",
        "build",
        "__pycache__",
        ".git",
        "vendor",
        ".venv",
        "venv",
        "coverage",
        ".next",
        ".rxt-cache",
    ];
    if SKIP.contains(&name.as_str()) {
        return false;
    }
    for pat in ignore {
        if name == *pat || name.starts_with(pat.trim_end_matches('*')) {
            return false;
        }
    }
    true
}

/// 极简 grep：路径:行:snippet，最多 max 条
fn compact_grep(root: &Path, query: &str, max: usize) -> Vec<String> {
    let q = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    let files = crate::common::walk_clean(root, None, None);
    for f in files {
        if hits.len() >= max {
            break;
        }
        let Ok(content) = std::fs::read_to_string(&f) else {
            continue;
        };
        let rel = f
            .strip_prefix(root)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| f.display().to_string())
            .replace('\\', "/");
        for (i, line) in content.lines().enumerate() {
            if line.to_ascii_lowercase().contains(&q) {
                let snip = line.trim();
                let snip = if snip.len() > 90 {
                    let mut t = snip.to_string();
                    truncate_chars(&mut t, 87);
                    format!("{}...", t)
                } else {
                    snip.to_string()
                };
                hits.push(format!("{}:{}: {}", rel, i + 1, snip));
                if hits.len() >= max {
                    break;
                }
            }
        }
    }
    hits
}
