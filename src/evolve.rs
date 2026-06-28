//! evolve — 差分测试 / 代码进化验证
//!
//! 让"重构/移植/自举"从赌博变成可度量工程:
//! 同一组输入喂给参照实现和候选实现, 逐个对比输出, 报告一致率。
//!
//! 核心场景: Zero 自举
//!   --ref "bootstrap.exe runz {input}"     (Go 编译器)
//!   --cand "bootstrap.exe self runz {input}" (Zero 自举编译器)
//!   --inputs tests/*.zero
//!   → 一致率 100% = 该能力进化完成
//!
//! 用法:
//!   rxt evolve --ref "cmd A {input}" --cand "cmd B {input}" --inputs "tests/*.zero"
//!   rxt evolve --ref "..." --cand "..." --inputs "dir/" --mode json
//!   rxt evolve --ref "..." --cand "..." --inputs "f1,f2,f3" --timeout 10
//!   rxt evolve --ref "..." --cand "..." --inputs "..." --first-fail  (首个失败即停,显示diff)

use std::process::Command;
use std::time::Duration;
use std::path::Path;

pub fn run(reference: &str, candidate: &str, inputs: &str, mode: &str, timeout_secs: u64, first_fail: bool, json: bool) -> anyhow::Result<()> {
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };

    // 解析输入集: 支持 glob 目录 / 逗号列表 / 单文件
    let input_files = expand_inputs(inputs)?;
    if input_files.is_empty() {
        anyhow::bail!("没有找到输入文件: {}", inputs);
    }

    println!("🧬 差分进化测试");
    println!("   参照: {}", reference);
    println!("   候选: {}", candidate);
    println!("   输入: {} 个文件", input_files.len());
    println!("   模式: {}\n", mode);

    let mut matches = 0usize;
    let mut mismatches = Vec::new();
    let mut ref_fails = 0usize;
    let mut cand_fails = 0usize;
    let mut both_fail = 0usize;

    for (i, input) in input_files.iter().enumerate() {
        let inp_str = input.display().to_string();
        let ref_cmd = reference.replace("{input}", &inp_str);
        let cand_cmd = candidate.replace("{input}", &inp_str);

        let ref_out = run_timed(shell, flag, &ref_cmd, timeout_secs);
        let cand_out = run_timed(shell, flag, &cand_cmd, timeout_secs);

        let short_name = input.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| inp_str.clone());

        // 判定一致(根据 mode)
        let verdict = judge(&ref_out, &cand_out, mode);

        match verdict {
            Verdict::Match => {
                matches += 1;
                if !json { eprint!("\r  ✓ {}/{} 一致", i + 1, input_files.len()); }
            }
            Verdict::Mismatch(ref_str, cand_str) => {
                mismatches.push((short_name.clone(), ref_str, cand_str));
                if !json {
                    eprint!("\r  ✗ {}/{} 不一致: {}                        \n", i + 1, input_files.len(), short_name);
                }
                if first_fail {
                    // 首个失败即停, 详细显示
                    break;
                }
            }
            Verdict::RefOnlyFail => { ref_fails += 1; }
            Verdict::CandOnlyFail => { cand_fails += 1; }
            Verdict::BothFail => { both_fail += 1; }
        }
    }
    eprintln!();

    let total = input_files.len();
    let consistency = if total > 0 { matches as f64 / total as f64 * 100.0 } else { 0.0 };

    if json {
        let report = serde_json::json!({
            "total": total,
            "matches": matches,
            "mismatches": mismatches.len(),
            "ref_only_fail": ref_fails,
            "cand_only_fail": cand_fails,
            "both_fail": both_fail,
            "consistency_pct": (consistency * 10.0).round() / 10.0,
            "verdict": if consistency == 100.0 { "EVOLVED" } else if consistency >= 99.0 { "NEARLY" } else { "DIVERGENT" },
            "mismatch_details": mismatches.iter().take(10).map(|(n, r, c)| serde_json::json!({
                "input": n, "ref": r, "cand": c,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // 人类可读报告
    println!("📊 进化报告");
    println!("   {:<12} {}", "总输入:", total);
    println!("   {:<12} {} ({:.1}%)", "一致:", matches, consistency);
    if !mismatches.is_empty() {
        println!("   {:<12} {}", "不一致:", mismatches.len());
    }
    if ref_fails > 0 { println!("   {:<12} {} (参照崩,候选正常)", "参照失败:", ref_fails); }
    if cand_fails > 0 { println!("   {:<12} {} (候选崩,需修复)", "候选失败:", cand_fails); }
    if both_fail > 0 { println!("   {:<12} {}", "双失败:", both_fail); }

    println!("\n{}", if consistency == 100.0 {
        "🎉 一致率 100% — 进化成功! 该能力可从参照实现切换到候选实现。"
    } else if consistency >= 99.0 {
        "⚠ 一致率 >=99% — 接近完成, 修复少量不一致即可切换。"
    } else {
        "❌ 一致率低 — 候选实现与参照差异较大, 继续迭代。"
    });

    // 显示前几个不一致的 diff
    if !mismatches.is_empty() {
        println!("\n🔍 不一致详情 (前 {} 个):", mismatches.len().min(5));
        for (name, ref_out, cand_out) in mismatches.iter().take(5) {
            println!("\n  ── {} ──", name);
            print_diff(ref_out, cand_out);
        }
        if mismatches.len() > 5 {
            println!("\n  ... 还有 {} 个不一致", mismatches.len() - 5);
        }
    }

    // 退出码: 100% 一致返回 0, 否则返回 1(便于 CI/脚本判定)
    if consistency < 100.0 {
        std::process::exit(1);
    }
    Ok(())
}

enum Verdict {
    Match,
    Mismatch(String, String),
    RefOnlyFail,
    CandOnlyFail,
    BothFail,
}

fn judge(ref_out: &RunResult, cand_out: &RunResult, mode: &str) -> Verdict {
    // Windows 下 cmd /C 可能吞退出码, 改用"有 stdout 输出"作为成功启发式
    // 真正的成功 = 退出码 >=0 且 (有输出 或 明确成功)
    let ref_ok = ref_out.exit_ok || (!ref_out.stdout.is_empty() && ref_out.exit_code >= 0);
    let cand_ok = cand_out.exit_ok || (!cand_out.stdout.is_empty() && cand_out.exit_code >= 0);
    // 超时(exit_code=-999)强制失败
    let ref_ok = ref_ok && ref_out.exit_code != -999;
    let cand_ok = cand_ok && cand_out.exit_code != -999;
    if !ref_ok && !cand_ok { return Verdict::BothFail; }
    if !ref_ok { return Verdict::RefOnlyFail; }
    if !cand_ok { return Verdict::CandOnlyFail; }

    // 都成功, 比较输出
    match mode {
        "exitcode" => {
            if ref_out.exit_code == cand_out.exit_code { Verdict::Match }
            else { Verdict::Mismatch(format!("exit {}", ref_out.exit_code), format!("exit {}", cand_out.exit_code)) }
        }
        "json" => {
            // 语义对比: 解析 JSON 后比较(忽略空白/顺序)
            let ref_val = serde_json::from_str::<serde_json::Value>(&ref_out.stdout).ok();
            let cand_val = serde_json::from_str::<serde_json::Value>(&cand_out.stdout).ok();
            match (ref_val, cand_val) {
                (Some(r), Some(c)) => {
                    if json_equal(&r, &c) { Verdict::Match }
                    else { Verdict::Mismatch(ref_out.stdout.clone(), cand_out.stdout.clone()) }
                }
                _ => {
                    // 非JSON, fallback 到精确比较
                    if ref_out.stdout.trim() == cand_out.stdout.trim() { Verdict::Match }
                    else { Verdict::Mismatch(ref_out.stdout.clone(), cand_out.stdout.clone()) }
                }
            }
        }
        _ => {
            // exact: 精确比较(去尾部空白)
            if ref_out.stdout.trim_end() == cand_out.stdout.trim_end() {
                Verdict::Match
            } else {
                Verdict::Mismatch(ref_out.stdout.clone(), cand_out.stdout.clone())
            }
        }
    }
}

/// JSON 语义相等: 对象 key 顺序无关, 数组顺序有关
fn json_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Object(ma), serde_json::Value::Object(mb)) => {
            if ma.len() != mb.len() { return false; }
            ma.iter().all(|(k, va)| mb.get(k).map_or(false, |vb| json_equal(va, vb)))
        }
        (serde_json::Value::Array(aa), serde_json::Value::Array(ab)) => {
            aa.len() == ab.len() && aa.iter().zip(ab).all(|(x, y)| json_equal(x, y))
        }
        _ => a == b,
    }
}

struct RunResult {
    stdout: String,
    exit_code: i32,
    exit_ok: bool,
}

fn run_timed(_shell: &str, _flag: &str, cmd: &str, timeout_secs: u64) -> RunResult {
    // 直接拆分命令执行(不经 shell), 避免 cmd/sh 环境差异
    // 简单 shell 解析: 按空格分词, 保留引号内整体
    let parts = shell_split(cmd);
    if parts.is_empty() {
        return RunResult { stdout: String::new(), exit_code: -1, exit_ok: false };
    }
    let start = std::time::Instant::now();
    let child = Command::new(&parts[0])
        .args(&parts[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    match child {
        Ok(mut child) => {
            let deadline = start + Duration::from_secs(timeout_secs);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let stdout = child.stdout.take().map(|mut s| {
                            use std::io::Read;
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok();
                            buf
                        }).unwrap_or_default();
                        return RunResult {
                            stdout,
                            exit_code: status.code().unwrap_or(-1),
                            exit_ok: status.success(),
                        };
                    }
                    Ok(None) => {
                        if std::time::Instant::now() > deadline {
                            let _ = child.kill();
                            return RunResult { stdout: String::new(), exit_code: -999, exit_ok: false };
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        return RunResult { stdout: String::new(), exit_code: -1, exit_ok: false };
                    }
                }
            }
        }
        Err(e) => {
            RunResult { stdout: format!("启动失败: {}", e), exit_code: -1, exit_ok: false }
        }
    }
}

/// 简单命令分词: 按空格, 但保留双引号内整体
fn shell_split(cmd: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in cmd.chars() {
        match c {
            '"' => in_quote = !in_quote,
            ' ' | '\t' if !in_quote => {
                if !cur.is_empty() { parts.push(cur.clone()); cur.clear(); }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() { parts.push(cur); }
    parts
}

fn print_diff(ref_out: &str, cand_out: &str) {
    let ref_lines: Vec<&str> = ref_out.lines().collect();
    let cand_lines: Vec<&str> = cand_out.lines().collect();
    let max = ref_lines.len().max(cand_lines.len());
    let mut shown = 0;
    for i in 0..max {
        let r = ref_lines.get(i).copied().unwrap_or("<无>");
        let c = cand_lines.get(i).copied().unwrap_or("<无>");
        if r == c {
            if shown < 3 {
                println!("    {:>6} | {}", i + 1, r);
            }
        } else {
            println!("  - {:>6} | {}", i + 1, r);  // 参照
            println!("  + {:>6} | {}", i + 1, c);  // 候选
            shown += 1;
            if shown > 8 {
                println!("    ... (更多差异省略)");
                break;
            }
        }
    }
    if ref_out.is_empty() && !cand_out.is_empty() {
        println!("  参照无输出, 候选有输出 ({} 字符)", cand_out.len());
    }
    if cand_out.is_empty() && !ref_out.is_empty() {
        println!("  候选无输出, 参照有输出 ({} 字符)", ref_out.len());
    }
}

/// 展开输入: glob 目录 / 逗号列表 / 单文件
fn expand_inputs(spec: &str) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        let p = Path::new(part);
        if p.is_dir() {
            // 目录: 收集所有文件(递归一层, 常见测试目录)
            for entry in std::fs::read_dir(p)? {
                let entry = entry?;
                if entry.path().is_file() {
                    files.push(entry.path());
                }
            }
        } else if part.contains('*') {
            // glob: 用 shell 展开
            let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
            let out = Command::new(shell).arg(flag).arg(format!("echo {}", part))
                .output().map_err(|e| anyhow::anyhow!("glob 失败: {}", e))?;
            let expanded = String::from_utf8_lossy(&out.stdout);
            for f in expanded.split_whitespace() {
                let fp = Path::new(f);
                if fp.is_file() { files.push(fp.to_path_buf()); }
            }
        } else if p.is_file() {
            files.push(p.to_path_buf());
        }
        // 不存在的静默跳过
    }
    files.sort();
    Ok(files)
}
