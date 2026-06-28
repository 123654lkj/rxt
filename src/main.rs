use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rxt", version, about = "Rust Codex Tools - AI's Cross-Platform IDE")]
struct Cli {
    #[arg(long, global = true, help = "远程主机（从 ~/.rxt/hosts.toml 读取）")]
    host: Option<String>,
    #[arg(long, global = true, help = "远程主机组（批量执行）")]
    group: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "块替换")]
    Replace {
        target: PathBuf,
        #[arg(long)] old: PathBuf,
        #[arg(long)] new: Option<PathBuf>,
        #[arg(long)] all: bool,
        #[arg(long)] preview: bool,
        #[arg(num_args = 0..)] content: Vec<String>,
    },
    #[command(about = "读文件，自动检测编码/换行符/BOM，内部统一 UTF-8+LF")]
    Read {
        path: PathBuf,
        #[arg(short, long)] encoding: Option<String>,
        #[arg(short, long)] number: bool,
        #[arg(short = 'H', long)] head: Option<usize>,
        #[arg(short = 'T', long)] tail: Option<usize>,
        #[arg(short = 'L', long)] lines: Option<String>,
        #[arg(long)] json: bool,
    },
    #[command(about = "写文件，自动保持目标文件格式")]
    Write {
        path: PathBuf,
        #[arg(num_args = 0..)] content: Vec<String>,
        #[arg(short, long)] append: bool,
        #[arg(long, help = "从本地文件读取内容 (改远程文件时不用 base64 编码)")] file: Option<PathBuf>,
        #[arg(long)] b64: bool,
        #[arg(long, default_value_t = true)] preserve: bool,
        #[arg(long, value_name = "PATH", help = "从本地文件读取内容覆盖 content/file, 远程写入专用")] from: Option<PathBuf>,
    },
    #[command(about = "打印文件内容")]
    Cat { path: PathBuf },
    #[command(about = "解析 Codex 会话 JSONL")]
    Jsonl {
        path: PathBuf,
        #[arg(short = 'L', long, default_value = "10")] last: usize,
        #[arg(short, long)] json: bool,
    },
    #[command(about = "补丁工具")]
    Patch {
        paths: Vec<String>,
        #[arg(short, long)] reverse: bool,
        #[arg(short, long)] check: bool,
        #[arg(short, long)] output: Option<String>,
    },
    #[command(about = "文件元信息 + 文件指纹")]
    Stat {
        path: PathBuf,
        #[arg(long)] json: bool,
    },
    #[command(about = "智能搜索")]
    Find {
        query: Option<String>,
        #[arg(short, long)] path: Option<PathBuf>,
        #[arg(short = 'n', long = "name")] name_pattern: Option<String>,
        #[arg(short = 't', long = "type")] file_type: Option<String>,
        #[arg(short = 'C', long = "context", default_value = "2")] context: usize,
        #[arg(short, long)] case_sensitive: bool,
        #[arg(long)] count: bool,
        #[arg(long)] stats: bool,
        #[arg(long)] replace: Option<String>,
        #[arg(long = "with")] replace_with: Option<String>,
        #[arg(long)] preview: bool,
        #[arg(long, help = "query 使用正则")] regex: bool,
        #[arg(long, help = "JSON 输出")] json: bool,
        #[arg(long, help = "最大匹配数")] max_results: Option<usize>,
        #[arg(long, help = "只返回前 N 条")] head: Option<usize>,
        #[arg(long, default_value = "0", help = "跳过前 N 条")] offset: usize,
    },
    #[command(about = "代码结构分析")]
    Struct {
        path: PathBuf,
        #[arg(short, long)] functions: bool,
        #[arg(short, long)] types: bool,
        #[arg(short, long)] deep: bool,
        #[arg(long)] extract: Option<String>,
        #[arg(long, help = "JSON 输出 [{file, kind, name, signature, line}]")] json: bool,
    },
    #[command(about = "差异对比")]
    Diff {
        first: PathBuf,
        second: Option<PathBuf>,
        #[arg(short = 'C', long = "context", default_value = "3")] context: usize,
        #[arg(short, long)] stat: bool,
        #[arg(long, help = "AI 模式: 输出结构化 JSON,包含每个变更的上下文和语义")]
        ai: bool,
        #[arg(long, help = "side-by-side 双栏对比")] side_by_side: bool,
        #[arg(long, help = "JSON 输出")] json: bool,
    },
    #[command(about = "依赖分析")]
    Dep {
        target: String,
        #[arg(short, long)] tree: bool,
        #[arg(short, long)] json: bool,
        #[arg(long)] check: bool,
    },
    #[command(about = "安全替换 — 格式保持")]
    Sed {
        path: PathBuf,
        #[arg(short, long)] pattern: String,
        #[arg(short, long)] replacement: String,
        #[arg(short, long)] preview: bool,
        #[arg(short, long)] line: Option<usize>,
        #[arg(long, help = "pattern 使用正则")]
        regex: bool,
    },
    #[command(about = "增强搜索 — 跨文件 grep")]
    Grep {
        pattern: String,
        #[arg(default_value = ".")] path: PathBuf,
        #[arg(short = 'C', long = "context", default_value = "2")] context: usize,
        #[arg(short = 't', long = "type")] file_type: Option<String>,
        #[arg(short, long)] count: bool,
        #[arg(short, long)] invert: bool,
        #[arg(short, long)] json: bool,
        #[arg(long, help = "使用正则表达式")] regex: bool,
        #[arg(long, help = "最大匹配数,默认 1000,0 表示无限")] max_results: Option<usize>,
        #[arg(long, help = "只返回前 N 条")] head: Option<usize>,
        #[arg(long, default_value = "0", help = "跳过前 N 条")] offset: usize,
        #[arg(long, help = "JSONL 流式输出(每行一条 JSON)")] jsonl: bool,
        #[arg(long, help = "不忽略 .git / target / node_modules / vendor / .开头目录")] no_ignore: bool,
    },
    #[command(about = "执行内联 Python")]
    Py { code: Option<String>, #[arg(short, long)] file: Option<PathBuf> },
    #[command(about = "星枢记忆")]
    Mem { #[command(subcommand)] action: MemAction },
    #[command(about = "目录树")]
    Tree {
        #[arg(default_value = ".")] path: PathBuf,
        #[arg(short = 'L', long = "depth")] depth: Option<usize>,
        #[arg(short = 'I', long = "ignore")] ignore: Option<String>,
        #[arg(short = 'd', long = "dirs-only")] dirs_only: bool,
        #[arg(long, help = "JSON 输出")] json: bool,
    },
    #[command(about = "智能 Git 提交")]
    #[command(about = "JSON 查询/格式化")]
    Jq {
        query: Option<String>,
        #[arg(short = 'f', long = "file")] file: Option<PathBuf>,
        #[arg(long = "fmt", help = "pretty-print (default behavior)")] fmt: bool,
        #[arg(short = 'c', long = "compact")] compact: bool,
        #[arg(short = 'r', long = "raw", help = "raw output (strings/numbers without quotes)")] raw: bool,
        #[arg(short = 's', long = "slurp", help = "slurp: read all inputs into array")] slurp: bool,
    },
    #[command(about = "tail -f 替代 — 监控文件追加新行")]
    Tail {
        path: PathBuf,
        #[arg(short = 'f', long = "filter", help = "正则过滤")] filter: Option<String>,
        #[arg(short = 'n', long = "interval", default_value = "500", help = "轮询间隔 ms")] interval: u64,
        #[arg(short = 'l', long = "lines", default_value = "10", help = "先打印最后 N 行")] lines: usize,
        #[arg(long, help = "检查一次后退出")] once: bool,
    },
        #[command(about = "归档解压 — zip / tar / tar.gz / tgz / 3mf")]
    Unzip {
        archive: PathBuf,
        #[arg(short = 'o', long = "to")] target: Option<PathBuf>,
        #[arg(short = 'l', long = "list", help = "只列内容不解压")] list_only: bool,
        #[arg(long, help = "JSON 输出")] json: bool,
        #[arg(long, help = "去掉前 N 层目录前缀")] strip: Option<usize>,
    },
    #[command(about = "目录列表 — 类似 ls")]
    Ls {
        #[arg(default_value = ".")] dir: PathBuf,
        #[arg(long)] json: bool,
        #[arg(short = 'a', long = "all", help = "含隐藏文件")] all: bool,
        #[arg(short = 's', long = "sort", help = "排序: name | size | mtime")] sort: Option<String>,
        #[arg(short = 'd', long = "depth", help = "递归深度")] depth: Option<usize>,
        #[arg(long, help = "限制最大结果数")] max: Option<usize>,
    },
    #[command(about = "HTTP 客户端")]
    Http {
        #[arg(default_value = "GET")] method: String,
        url: String,
        #[arg(short = 'H', long = "header", help = "header: value")] headers: Vec<String>,
        #[arg(short = 'd', long = "data")] data: Option<String>,
        #[arg(short = 'j', long = "json", help = "request body is JSON")] json_body: bool,
        #[arg(long, help = "basic auth: user:pass")] auth: Option<String>,
        #[arg(short = 't', long = "timeout", default_value = "30")] timeout: u64,
        #[arg(short = 'i', long = "headers", help = "show response headers")] show_headers: bool,
        #[arg(short = 'b', long = "body-only")] body_only: bool,
    },
    #[command(about = "结构化文件编辑 — 格式保持")]
    Edit {
        path: PathBuf,
        #[arg(long)] after: Option<String>,
        #[arg(long)] before: Option<String>,
        #[arg(long)] delete: Option<String>,
        #[arg(long)] replace: Option<String>,
        #[arg(num_args = 0..)] content: Vec<String>,
        #[arg(long)] preview: bool,
        #[arg(long)] script: Option<PathBuf>,
        #[arg(short = 'L', long, help = "替换指定行范围 (如 10-20, 15)")]
        line_range: Option<String>,
        #[arg(long, help = "pattern 使用正则")]
        regex: bool,
    },
    #[command(about = "文件哈希")]
    Hash { path: Option<PathBuf>, #[arg(short, long, default_value = "sha256")] algo: String, #[arg(short, long)] text: Option<String> },
    #[command(about = "UUID 生成器")]
    Uuid { #[arg(short, long, default_value_t = 1)] count: usize },
    #[command(about = "编码/解码")]
    Enc { mode: String, input: Option<String>, #[arg(short, long)] decode: bool, #[arg(long)] file: Option<PathBuf> },
    #[command(about = "解码")]
    Dec { mode: String, input: Option<String>, #[arg(long)] file: Option<PathBuf> },
    #[command(about = "文件监听")]
    Watch { patterns: Vec<String>, cmd: String, #[arg(short = 'p', long = "path")] path: Option<PathBuf>, #[arg(short = 'd', long = "debounce", default_value = "500")] debounce: u64 },
    #[command(about = "命令计时")]
    Time { cmd: String },
    #[command(about = "多语言代码执行")]
    Exec {
        code: Option<String>,
        #[arg(long)] b64: bool,
        #[arg(long)] lang: Option<String>,
        #[arg(long)] write: Option<PathBuf>,
        #[arg(long)] file: Option<PathBuf>,
        #[arg(long, help = "走 login shell, 加载完整 PATH/aliases")] login: bool,
        #[arg(long, help = "输出 JSON 包含 exit_code, stdout, stderr")] json: bool,
    },
    #[command(about = "行排序")]
    Sort { input: Option<String>, #[arg(short, long)] reverse: bool, #[arg(short = 'n', long)] numeric: bool, #[arg(short = 'k', long)] column: Option<usize>, #[arg(short = 't', long)] separator: Option<String>, #[arg(short = 'u', long)] unique: bool },
    #[command(about = "行去重")]
    Uniq { input: Option<String>, #[arg(short = 'c', long)] count: bool, #[arg(short = 'd', long)] duplicates: bool, #[arg(short = 'i', long)] ignore_case: bool },
    #[command(about = "列提取")]
    Cut { input: Option<String>, #[arg(short = 'd', long)] delimiter: Option<String>, #[arg(short = 'f', long, required = true)] fields: String, #[arg(short = 's', long)] only_delimited: bool },
    #[command(about = "行/词/字符/字节统计")]
    Count { input: Option<String>, #[arg(short = 'l', long)] lines: bool, #[arg(short = 'w', long)] words: bool, #[arg(short = 'm', long)] chars: bool, #[arg(short = 'c', long)] bytes: bool, #[arg(short = 'L', long)] max_line: bool, #[arg(long, help = "JSON 输出")] json: bool },
    #[command(about = "智能 Rust 构建")]
    Build { dir: Option<String>, #[arg(short = 't', long = "target")] target: Option<String>, #[arg(short = 'p', long = "profile")] profile: Option<String>, #[arg(short = 'b', long = "bin")] bin: Option<String>, #[arg(long)] features: Vec<String>, #[arg(long)] workspace: bool, #[arg(long)] list_targets: bool, #[arg(long)] no_config: bool },
    #[command(about = "Rust 代码质量检查")]
    Check { dir: Option<String>, #[arg(long)] clippy: bool, #[arg(long)] fmt: bool, #[arg(long)] fix: bool },
    #[command(about = "编译产物大小分析")]
    Size { dir: Option<String>, #[arg(short = 't', long = "target")] target: Option<String>, #[arg(short = 'p', long = "profile")] profile: Option<String>, #[arg(short = 'a', long)] all: bool, #[arg(short = 'H', long)] human: bool, #[arg(short = 's', long)] sort: bool },
    #[command(about = "智能清理")]
    Clean { dir: Option<String>, #[arg(short = 't', long = "target")] target: Option<String>, #[arg(short = 'p', long = "profile")] profile: Option<String>, #[arg(long)] dry_run: bool, #[arg(short = 'a', long)] all: bool },
    #[command(about = "AI 上下文生成器 — 一次输出签名/imports/内容")]
    Ctx {
        path: PathBuf,
        #[arg(short = 'H', long, help = "限制总行数(超过自动截断)")]
        max_lines: Option<usize>,
        #[arg(long)] json: bool,
    },
    #[command(about = "文件格式统一 — 跨平台文本标准化")]
    Normalize {
        path: PathBuf,
        #[arg(short = 'e', long = "ending")] ending: Option<String>,
        #[arg(long)] remove_bom: bool,
        #[arg(long)] json: bool,
    },
    #[command(about = "rxt 自检 - 版本/配置/hosts 状态")]
    Info {
        #[arg(long)] json: bool,
    },
    #[command(about = "Git 操作 - AI 友好的 git 包装 (status/diff/log/branch/add/commit/undo)")]
    Git {
        #[command(subcommand)]
        cmd: crate::git::GitSubCmd,
        #[arg(long)] json: bool,
    },
}

#[derive(Subcommand)]
enum MemAction {
    #[command(about = "保存记忆")]
    Save { content: String, #[arg(long, default_value = "code")] category: String, #[arg(long, default_value_t = 0.6)] importance: f64 },
    #[command(about = "搜索记忆")]
    Search { query: String, #[arg(short = 'k', long = "top-k", default_value = "5")] top_k: usize },
    #[command(about = "星枢统计")]
    Stats,
}

mod common;
mod signature;
mod replace; mod read; mod write; mod cat; mod jsonl; mod stat;
mod find;
#[path = "struct.rs"]
mod struct_mod;
mod diff; mod dep; mod sed; mod grep; mod patch; mod tree;
mod py; mod mem;  mod jq;
mod unzip;
mod ls;
mod http; mod edit; mod hash;
mod uuidgen; mod enc; mod watch;
mod tail; mod timecmd; mod exec;
mod sort; mod uniq; mod cut; mod count;
mod build; mod check; mod size; mod clean;
mod normalize;
mod info;
mod git;
mod ctx;
mod hosts;
mod remote;

fn main() -> anyhow::Result<()> {
    crate::common::setup_utf8_console();
    let cli = Cli::parse();
    
    // 如果有 --group,批量执行
    if let Some(ref group_name) = cli.group {
        let hosts_config = crate::hosts::HostsFile::load()?;
        let members = hosts_config.get_group_members(group_name)?;
        
        for member in &members {
            eprintln!("
=== [{}] ===", member);
            let remote_channel = crate::remote::RemoteChannel::connect(member)?;
            
            match &cli.command {
                Command::Read { path, encoding, number, head, tail, lines, json } => {
                    read::run(path, encoding.clone(), *number, *head, *tail, lines.clone(), *json, Some(&remote_channel))?;
                }
                Command::Stat { path, json } => {
                    stat::run(path, *json, Some(&remote_channel))?;
                }
                Command::Sed { path, pattern, replacement, preview, line, regex: _ } => {
                    sed::run(path, pattern, replacement, *preview, *line, false, Some(&remote_channel))?;
                }
                Command::Write { path, content, append, file, b64, preserve, from: _ } => {
                    if let Some(f) = file {
                        write::run_file(path, f, *append)?;
                    } else if *b64 {
                        let j: Vec<String> = content.iter().cloned().collect();
                        write::run_b64(path, &j.join(""), *append)?;
                    } else {
                        let j: Vec<String> = content.iter().cloned().collect();
                        let joined = j.join("
");
                        let opt: Option<&str> = if j.is_empty() { None } else { Some(&joined) };
                        write::run(path, opt, *append, *preserve, Some(&remote_channel))?;
                    }
                }
                Command::Cat { path } => {
                    cat::run(path)?;
                }
                Command::Grep { pattern, path, context, file_type, count, invert, json, regex: _, max_results, head, offset, jsonl, no_ignore } => {
                    grep::run(pattern, path, *context, file_type.as_deref(), *count, *invert, *json, false, *max_results, *head, *offset, *jsonl, *no_ignore, Some(&remote_channel))?;
                }
                Command::Find { query, path, name_pattern, file_type, context, case_sensitive, count, stats, replace, replace_with, preview, regex: _, json, max_results, head, offset } => {
                    find::run(query.as_deref(), path.as_deref(), name_pattern.as_deref(), file_type.as_deref(), *context, *case_sensitive, *count, *stats, replace.as_deref(), replace_with.as_deref(), *preview, false, *json, *max_results, *head, *offset, Some(&remote_channel))?;
                }
                Command::Replace { target, old, new, all, preview, content } => {
                    let nc: Option<String> = if let Some(f) = new {
                        Some(std::fs::read_to_string(f)?)
                    } else if !content.is_empty() {
                        Some(content.join("
"))
                    } else { None };
                    replace::run(target, old, nc.as_deref(), *all, *preview, Some(&remote_channel))?;
                }
                Command::Edit { path, after, before, delete, replace, content, preview, script, line_range, regex: _ } => {
                    let rep = replace.as_deref().and_then(|s| {
                        let mut p = s.splitn(2, ',');
                        Some((p.next()?, p.next()?))
                    });
                    if let Some(sp) = script {
                        edit::run_script(path, sp, *preview, Some(&remote_channel))?;
                    } else {
                        edit::run(path, after.as_deref(), before.as_deref(), delete.as_deref(), rep, content, *preview, false, Some(&remote_channel))?;
                    }
                }
                Command::Exec { code, b64, lang, write, file, login: _, json } => {
                    let cs = if let Some(f) = file { std::fs::read_to_string(f)? } else { code.clone().unwrap_or_default() };
                    let ec = exec::run(&cs, *b64, lang.as_deref(), write.as_ref(), Some(&remote_channel), false, *json)?;
                    if ec != 0 { std::process::exit(ec); }
                }
                Command::Ctx { path, max_lines, json } => {
                    ctx::run(path, *max_lines, *json, None)?;
                }
                _ => {
                    eprintln!("  Group execution not supported for this command yet");
                }
            }
        }
        return Ok(());
    }
    
    // 如果有 --host,建立远程连接
    let remote_channel = if let Some(ref host) = cli.host {
        Some(crate::remote::RemoteChannel::connect(host)?)
    } else {
        None
    };
    
    match cli.command {
        Command::Replace { target, old, new, all, preview, content } => {
            let nc: Option<String> = if let Some(f) = new {
                Some(std::fs::read_to_string(f)?)
            } else if !content.is_empty() {
                Some(content.join("\n"))
            } else { None };
            replace::run(&target, &old, nc.as_deref(), all, preview, remote_channel.as_ref())?;
        }
        Command::Read { path, encoding, number, head, tail, lines, json } => {
            read::run(&path, encoding, number, head, tail, lines, json, remote_channel.as_ref())?;
        }
        Command::Write { path, content, append, file, b64, preserve, from } => {
            if let Some(f) = from {
                let data = std::fs::read(&f)?;
                write::run_bytes(&path, &data, append, preserve, remote_channel.as_ref())?;
            } else if let Some(f) = file {
                write::run_file(&path, &f, append)?;
            } else if b64 {
                let j = content.join("");
                write::run_b64(&path, &j, append)?;
            } else {
                let j = content.join("\n");
                write::run(&path, if content.is_empty() { None } else { Some(&j) }, append, preserve, remote_channel.as_ref())?;
            }
        }
        Command::Cat { path } => cat::run(&path)?,
        Command::Jsonl { path, last, json } => jsonl::run(&path, last, json)?,
        Command::Stat { path, json } => stat::run(&path, json, remote_channel.as_ref())?,
        Command::Find { query, path, name_pattern, file_type, context, case_sensitive, count, stats, replace, replace_with, preview, regex, json, max_results, head, offset } => {
            find::run(query.as_deref(), path.as_deref(), name_pattern.as_deref(), file_type.as_deref(), context, case_sensitive, count, stats, replace.as_deref(), replace_with.as_deref(), preview, regex, json, max_results, head, offset, remote_channel.as_ref())?;
        }
        Command::Struct { path, functions, types, deep, extract, json } => {
            struct_mod::run(&path, functions, types, deep, extract.as_deref(), json)?;
        }
        Command::Diff { first, second, context, stat, ai, side_by_side, json } => {
            diff::run(&first, second.as_deref(), context, stat, ai, side_by_side, json)?;
        }
        Command::Dep { target, tree, json, check } => dep::run(&target, tree, json, check)?,
        Command::Sed { path, pattern, replacement, preview, line, regex } => {
            sed::run(&path, &pattern, &replacement, preview, line, regex, remote_channel.as_ref())?;
        }
        Command::Grep { pattern, path, context, file_type, count, invert, json, regex, max_results, head, offset, jsonl, no_ignore } => {
            grep::run(&pattern, &path, context, file_type.as_deref(), count, invert, json, regex, max_results, head, offset, jsonl, no_ignore, remote_channel.as_ref())?;
        }
        Command::Patch { paths, reverse, check, output } => {
            patch::run(&paths, reverse, check, output.as_deref())?;
        }
        Command::Py { code, file } => py::run(code.as_deref(), file.as_ref())?,
        Command::Mem { action } => match action {
            MemAction::Save { content, category, importance } => mem::run_save(&content, &category, importance)?,
            MemAction::Search { query, top_k } => mem::run_search(&query, top_k)?,
            MemAction::Stats => mem::run_stats()?,
        },
        Command::Tree { path, depth, ignore, dirs_only, json } => {
            let ignores: Vec<String> = ignore.as_deref()
                .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                .unwrap_or_default();
            tree::run(&path, depth, &ignores, dirs_only, json)?;
        }
        Command::Jq { query, file, fmt, compact, raw, slurp } => jq::run(query.as_deref(), file.as_deref(), fmt, compact, raw, slurp)?,
        Command::Unzip { archive, target, list_only, json, strip } => unzip::run(&archive, target.as_deref(), list_only, json, strip)?,
        Command::Ls { dir, json, all, sort, depth, max } => ls::run(&dir, json, all, sort.as_deref(), depth, max)?,
        Command::Http { method, url, headers, data, json_body, auth, timeout: _, show_headers, body_only } => http::run(&method, &url, &headers, data.as_deref(), json_body, auth.as_deref(), show_headers, body_only)?,
        Command::Edit { path, after, before, delete, replace, content, preview, script, line_range, regex } => {
            let rep = replace.as_deref().and_then(|s| {
                let mut p = s.splitn(2, ',');
                Some((p.next()?, p.next()?))
            });
            if let Some(sp) = script {
                edit::run_script(&path, &sp, preview, remote_channel.as_ref())?;
            } else if let Some(lr) = line_range {
                edit::run_line_range(&path, lr.as_str(), &content, preview, remote_channel.as_ref())?;
            } else {
                edit::run(&path, after.as_deref(), before.as_deref(), delete.as_deref(), rep, &content, preview, regex, remote_channel.as_ref())?;
            }
        }
        Command::Hash { path, algo, text } => hash::run(path.as_deref(), &algo, text.as_deref())?,
        Command::Uuid { count } => uuidgen::run(count)?,
        Command::Enc { mode, input, decode, file } => {
            let fc;
            let is: Option<&str> = if let Some(f) = file { fc = std::fs::read_to_string(f)?; Some(&fc) } else { input.as_deref() };
            enc::run(&mode, is, decode)?;
        }
        Command::Dec { mode, input, file } => {
            let fc;
            let is: Option<&str> = if let Some(f) = file { fc = std::fs::read_to_string(f)?; Some(&fc) } else { input.as_deref() };
            enc::run(&mode, is, true)?;
        }
        Command::Watch { patterns, cmd, path, debounce } => watch::run(&patterns, &cmd, path.as_deref(), debounce)?,
        Command::Tail { path, filter, interval, lines, once } => tail::run(&path, filter.as_deref(), interval, lines, once)?,
        Command::Time { cmd } => timecmd::run(&cmd)?,
        Command::Exec { code, b64, lang, write, file, login, json } => {
            let cs = if let Some(f) = file { std::fs::read_to_string(f)? } else { code.unwrap_or_default() };
            exec::run(&cs, b64, lang.as_deref(), write.as_ref(), remote_channel.as_ref(), login, json)?;
        }
        Command::Sort { input, reverse, numeric, column, separator, unique } => {
            sort::run(input.as_deref(), reverse, numeric, column, separator, unique)?;
        }
        Command::Uniq { input, count, duplicates, ignore_case } => {
            uniq::run(input.as_deref(), count, duplicates, ignore_case)?;
        }
        Command::Cut { input, delimiter, fields, only_delimited } => {
            cut::run(input.as_deref(), delimiter, &fields, only_delimited)?;
        }
        Command::Count { input, lines, words, chars, bytes, max_line, json } => {
            count::run(input.as_deref(), lines, words, chars, bytes, max_line, json)?;
        }
        Command::Build { dir, target, profile, bin, features, workspace, list_targets, no_config } => {
            build::run(dir.as_deref(), target.as_deref(), profile.as_deref(), bin.as_deref(), features, workspace, list_targets, no_config)?;
        }
        Command::Check { dir, clippy, fmt, fix } => check::run(dir.as_deref(), clippy, fmt, fix)?,
        Command::Size { dir, target, profile, all, human, sort } => {
            size::run(dir.as_deref(), target.as_deref(), profile.as_deref(), all, human, sort)?;
        }
        Command::Clean { dir, target, profile, dry_run, all } => {
            clean::run(dir.as_deref(), target.as_deref(), profile.as_deref(), dry_run, all)?;
        }
        Command::Ctx { path, max_lines, json } => {
            ctx::run(&path, max_lines, json, remote_channel.as_ref())?;
        }
        Command::Normalize { path, ending, remove_bom, json } => {
            normalize::run(&path, ending.as_deref(), remove_bom, json)?;
        }
        Command::Info { json } => {
            info::run(json)?;
        }
        Command::Git { cmd, json } => {
            git::run(cmd, json)?;
        }
    }
    Ok(())
}