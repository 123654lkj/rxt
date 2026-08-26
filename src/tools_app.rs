//! 标准库命令 — 独立二进制 rxt-tools（多路调用）。
//! 由 rxt 核心通过插件调度：rxt grep → ~/.rxt/plugins/grep → rxt-tools。

use crate::*;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "rxt",
    version,
    about = "rxt-tools — Run eXternal Tools 标准库（由 rxt 按插件调度）",
    allow_external_subcommands = true
)]
pub struct Cli {
    #[arg(long, global = true, help = "远程主机（从 ~/.rxt/hosts.toml 读取）")]
    host: Option<String>,
    #[arg(long, global = true, help = "远程主机组（批量执行）")]
    group: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
pub enum Command {
    #[command(about = "块替换")]
    Replace {
        target: PathBuf,
        #[arg(long)]
        old: PathBuf,
        #[arg(long)]
        new: Option<PathBuf>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        preview: bool,
        #[arg(num_args = 0..)]
        content: Vec<String>,
    },
    #[command(about = "读文件，自动检测编码/换行符/BOM，内部统一 UTF-8+LF")]
    Read {
        path: PathBuf,
        #[arg(short, long)]
        encoding: Option<String>,
        #[arg(short, long)]
        number: bool,
        #[arg(short = 'H', long)]
        head: Option<usize>,
        #[arg(short = 'T', long)]
        tail: Option<usize>,
        #[arg(short = 'L', long)]
        lines: Option<String>,
        #[arg(short, long, help = "token 预算, 超了自动截断")]
        budget: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "写文件，自动保持目标文件格式")]
    Write {
        path: PathBuf,
        #[arg(num_args = 0..)]
        content: Vec<String>,
        #[arg(short, long)]
        append: bool,
        #[arg(long, help = "从本地文件读取内容 (改远程文件时不用 base64 编码)")]
        file: Option<PathBuf>,
        #[arg(long)]
        b64: bool,
        #[arg(long, default_value_t = true)]
        preserve: bool,
        #[arg(
            long,
            value_name = "PATH",
            help = "从本地文件读取内容覆盖 content/file, 远程写入专用"
        )]
        from: Option<PathBuf>,
    },
    #[command(about = "打印文件内容")]
    Cat { path: PathBuf },
    #[command(about = "解析 Codex 会话 JSONL")]
    Jsonl {
        path: PathBuf,
        #[arg(short = 'L', long, default_value = "10")]
        last: usize,
        #[arg(short, long)]
        json: bool,
    },
    #[command(about = "补丁工具")]
    Patch {
        paths: Vec<String>,
        #[arg(short, long)]
        reverse: bool,
        #[arg(short, long)]
        check: bool,
        #[arg(short, long)]
        output: Option<String>,
    },
    #[command(about = "文件元信息 + 文件指纹")]
    Stat {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "智能搜索",
        after_help = "示例:\n  rxt find TODO -p src\n  rxt find /dir --name '*.rs'\n  rxt find /dir -name '*.md'   # GNU 风格 -name 也认\n  rxt --host huhu find /home/huhu --name '*.md'"
    )]
    Find {
        query: Option<String>,
        #[arg(short, long)]
        path: Option<PathBuf>,
        #[arg(short = 'n', long = "name")]
        name_pattern: Option<String>,
        #[arg(short = 't', long = "type")]
        file_type: Option<String>,
        #[arg(short = 'C', long = "context", default_value = "2")]
        context: usize,
        #[arg(short, long)]
        case_sensitive: bool,
        #[arg(long)]
        count: bool,
        #[arg(long)]
        stats: bool,
        #[arg(long)]
        replace: Option<String>,
        #[arg(long = "with")]
        replace_with: Option<String>,
        #[arg(long)]
        preview: bool,
        #[arg(long, help = "query 使用正则")]
        regex: bool,
        #[arg(long, help = "JSON 输出")]
        json: bool,
        #[arg(long, help = "最大匹配数")]
        max_results: Option<usize>,
        #[arg(long, help = "只返回前 N 条")]
        head: Option<usize>,
        #[arg(long, default_value = "0", help = "跳过前 N 条")]
        offset: usize,
    },
    #[command(about = "代码结构分析")]
    Struct {
        path: PathBuf,
        #[arg(short, long)]
        functions: bool,
        #[arg(short, long)]
        types: bool,
        #[arg(short, long)]
        deep: bool,
        #[arg(long)]
        extract: Option<String>,
        #[arg(long, help = "JSON 输出 [{file, kind, name, signature, line}]")]
        json: bool,
    },
    #[command(about = "差异对比")]
    Diff {
        first: PathBuf,
        second: Option<PathBuf>,
        #[arg(short = 'C', long = "context", default_value = "3")]
        context: usize,
        #[arg(short, long)]
        stat: bool,
        #[arg(long, help = "AI 模式: 输出结构化 JSON,包含每个变更的上下文和语义")]
        ai: bool,
        #[arg(long, help = "side-by-side 双栏对比")]
        side_by_side: bool,
        #[arg(long, help = "JSON 输出")]
        json: bool,
    },
    #[command(about = "依赖分析")]
    Dep {
        target: String,
        #[arg(short, long)]
        tree: bool,
        #[arg(short, long)]
        json: bool,
        #[arg(long)]
        check: bool,
    },
    #[command(about = "安全替换 — 格式保持")]
    Sed {
        path: PathBuf,
        #[arg(short, long)]
        pattern: String,
        #[arg(short, long)]
        replacement: String,
        #[arg(short, long)]
        preview: bool,
        #[arg(short, long)]
        line: Option<usize>,
        #[arg(long, help = "pattern 使用正则")]
        regex: bool,
    },
    #[command(about = "增强搜索 — 跨文件 grep")]
    Grep {
        pattern: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short = 'C', long = "context", default_value = "2")]
        context: usize,
        #[arg(short = 't', long = "type")]
        file_type: Option<String>,
        #[arg(short, long)]
        count: bool,
        #[arg(short, long)]
        invert: bool,
        #[arg(short, long)]
        json: bool,
        #[arg(long, help = "使用正则表达式")]
        regex: bool,
        #[arg(long, help = "最大匹配数,默认 1000,0 表示无限")]
        max_results: Option<usize>,
        #[arg(long, help = "只返回前 N 条")]
        head: Option<usize>,
        #[arg(long, default_value = "0", help = "跳过前 N 条")]
        offset: usize,
        #[arg(long, help = "JSONL 流式输出(每行一条 JSON)")]
        jsonl: bool,
        #[arg(
            long,
            help = "不忽略 .git / target / node_modules / vendor / .开头目录"
        )]
        no_ignore: bool,
    },
    #[command(
        about = "统一搜索 — glob 搜文件名，否则搜内容",
        after_help = "示例:\n  rxt search TODO\n  rxt search \"*.rs\"\n  rxt search \"fn main\" --type rs\n  rxt search --name \"*.toml\"\n  rxt search --content TODO"
    )]
    Search {
        query: String,
        #[arg(short, long, help = "搜索根目录")]
        path: Option<String>,
        #[arg(short = 't', long = "type", help = "扩展名，逗号分隔")]
        file_type: Option<String>,
        #[arg(long, help = "强制按文件名 glob")]
        name: bool,
        #[arg(long, help = "强制搜内容")]
        content: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 200, help = "最大结果数")]
        max_results: usize,
    },
    #[command(about = "执行内联 Python")]
    Py {
        code: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    #[command(about = "星枢记忆", disable_help_subcommand = true)]
    Mem {
        #[command(subcommand)]
        action: MemAction,
    },
    #[command(about = "目录树")]
    Tree {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short = 'L', long = "depth")]
        depth: Option<usize>,
        #[arg(short = 'I', long = "ignore")]
        ignore: Option<String>,
        #[arg(short = 'd', long = "dirs-only")]
        dirs_only: bool,
        #[arg(long, help = "JSON 输出")]
        json: bool,
    },
    #[command(about = "智能 Git 提交")]
    #[command(about = "JSON 查询/格式化")]
    Jq {
        query: Option<String>,
        #[arg(short = 'f', long = "file")]
        file: Option<PathBuf>,
        #[arg(long = "fmt", help = "pretty-print (default behavior)")]
        fmt: bool,
        #[arg(short = 'c', long = "compact")]
        compact: bool,
        #[arg(
            short = 'r',
            long = "raw",
            help = "raw output (strings/numbers without quotes)"
        )]
        raw: bool,
        #[arg(
            short = 's',
            long = "slurp",
            help = "slurp: read all inputs into array"
        )]
        slurp: bool,
    },
    #[command(about = "tail -f 替代 — 监控文件追加新行")]
    Tail {
        path: PathBuf,
        #[arg(short = 'f', long = "filter", help = "正则过滤")]
        filter: Option<String>,
        #[arg(
            short = 'n',
            long = "interval",
            default_value = "500",
            help = "轮询间隔 ms"
        )]
        interval: u64,
        #[arg(
            short = 'l',
            long = "lines",
            default_value = "10",
            help = "先打印最后 N 行"
        )]
        lines: usize,
        #[arg(long, help = "检查一次后退出")]
        once: bool,
    },
    #[command(about = "归档解压 — zip / tar / tar.gz / tgz / 3mf")]
    Unzip {
        archive: PathBuf,
        #[arg(short = 'o', long = "to")]
        target: Option<PathBuf>,
        #[arg(short = 'l', long = "list", help = "只列内容不解压")]
        list_only: bool,
        #[arg(long, help = "JSON 输出")]
        json: bool,
        #[arg(long, help = "去掉前 N 层目录前缀")]
        strip: Option<usize>,
    },
    #[command(about = "目录列表 — 类似 ls")]
    Ls {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(short = 'a', long = "all", help = "含隐藏文件")]
        all: bool,
        #[arg(short = 's', long = "sort", help = "排序: name | size | mtime")]
        sort: Option<String>,
        #[arg(short = 'd', long = "depth", help = "递归深度")]
        depth: Option<usize>,
        #[arg(long, help = "限制最大结果数")]
        max: Option<usize>,
    },
    #[command(about = "HTTP 客户端 — CLI 打开网页、读数据、点选填写；scan 从 JS 抽 API")]
    Http {
        #[arg(
            default_value = "GET",
            help = "GET|POST|…|open|snap|read|fill|click|eval|net|wait|storage|import|auth|sso|close|cookies|forms|cli|scan|session"
        )]
        method: String,
        #[arg(
            value_name = "URL",
            num_args = 0..,
            help = "URL，或 click/fill/read 的 @e1"
        )]
        urls: Vec<String>,
        #[arg(short = 'H', long = "header", help = "header: value")]
        headers: Vec<String>,
        #[arg(short = 'd', long = "data")]
        data: Option<String>,
        #[arg(
            short = 'j',
            long = "json",
            help = "request body is JSON；cookies/forms/cli/scan/多 URL 则输出 JSON"
        )]
        json_body: bool,
        #[arg(long, help = "basic auth: user:pass")]
        auth: Option<String>,
        #[arg(short = 't', long = "timeout", default_value = "30")]
        timeout: u64,
        #[arg(short = 'i', long = "headers", help = "show response headers")]
        show_headers: bool,
        #[arg(short = 'b', long = "body-only")]
        body_only: bool,
        #[arg(short = 'o', long = "output", help = "把响应体写到文件")]
        output: Option<PathBuf>,
        #[arg(
            long,
            help = "从本机浏览器导入 Cookie: chrome|edge|firefox|brave|opera|vivaldi|arc|tabbit|all|auto，或 Chromium User Data 目录。Chrome/Edge 127+ App-Bound 常失败，改 firefox / --cookie-json。环境变量 RXT_BROWSER"
        )]
        browser: Option<String>,
        #[arg(
            long = "cookie-jar",
            help = "Netscape Cookie 罐（请求前加载、响应后写回；环境变量 RXT_COOKIE_JAR）"
        )]
        cookie_jar: Option<PathBuf>,
        #[arg(
            long = "cookie",
            help = "额外 Cookie: name=value（可重复，或 a=1; b=2）"
        )]
        cookies: Vec<String>,
        #[arg(long = "ua", alias = "user-agent", help = "User-Agent（默认 Chrome）")]
        user_agent: Option<String>,
        #[arg(long, help = "HTML 抽成可读正文")]
        text: bool,
        #[arg(long, help = "提取页面链接")]
        links: bool,
        #[arg(long, help = "限制打印的正文字符数")]
        budget: Option<usize>,
        #[arg(long = "form", help = "表单字段 name=value（可重复，包装网页提交）")]
        form: Vec<String>,
        #[arg(long = "no-probe", help = "scan 时不探测接口，只抽 JS")]
        no_probe: bool,
        #[arg(
            long = "cookie-json",
            help = "DevTools Cookie JSON 数组或文件 [{name,value,domain,path}]（环境变量 RXT_COOKIE_JSON）"
        )]
        cookie_json: Option<String>,
        #[arg(
            long = "select",
            help = "从 HTML 抽数据：h1 / #id / .class / [name=q] / table"
        )]
        select: Option<String>,
        #[arg(
            long = "session",
            help = "页面会话名（默认 default，环境变量 RXT_HTTP_SESSION）"
        )]
        session: Option<String>,
        #[arg(
            long = "engine",
            help = "页面引擎: auto|js|static（默认 auto=有 Lightpanda 就跑 JS。环境变量 RXT_HTTP_ENGINE）"
        )]
        engine: Option<String>,
    },
    #[command(about = "结构化文件编辑 — 格式保持")]
    Edit {
        path: PathBuf,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        delete: Option<String>,
        #[arg(long)]
        replace: Option<String>,
        #[arg(num_args = 0..)]
        content: Vec<String>,
        #[arg(long)]
        preview: bool,
        #[arg(long)]
        script: Option<PathBuf>,
        #[arg(short = 'L', long, help = "替换指定行范围 (如 10-20, 15)")]
        line_range: Option<String>,
        #[arg(long, help = "pattern 使用正则")]
        regex: bool,
    },
    #[command(about = "文件哈希")]
    Hash {
        path: Option<PathBuf>,
        #[arg(short, long, default_value = "sha256")]
        algo: String,
        #[arg(short, long)]
        text: Option<String>,
    },
    #[command(about = "UUID 生成器")]
    Uuid {
        #[arg(short, long, default_value_t = 1)]
        count: usize,
    },
    #[command(about = "编码/解码")]
    Enc {
        mode: String,
        input: Option<String>,
        #[arg(short, long)]
        decode: bool,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    #[command(about = "解码")]
    Dec {
        mode: String,
        input: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    #[command(about = "文件监听")]
    Watch {
        patterns: Vec<String>,
        cmd: String,
        #[arg(short = 'p', long = "path")]
        path: Option<PathBuf>,
        #[arg(short = 'd', long = "debounce", default_value = "500")]
        debounce: u64,
    },
    #[command(about = "命令计时")]
    Time { cmd: String },
    #[command(about = "多语言代码执行")]
    Exec {
        code: Option<String>,
        #[arg(long)]
        b64: bool,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, help = "走 login shell, 加载完整 PATH/aliases")]
        login: bool,
        #[arg(long, help = "输出 JSON 包含 exit_code, stdout, stderr")]
        json: bool,
        #[arg(
            long,
            help = "在指定 docker 容器内执行 (--host 远程时为远程容器, 否则本地容器)"
        )]
        container: Option<String>,
        #[arg(
            long,
            help = "SQL 数据库名 (--lang sql 时配合 psql 使用, 默认 postgres)"
        )]
        db: Option<String>,
        #[arg(long, help = "SQL 用户名 (--lang sql 时配合 psql 使用, 默认 postgres)")]
        sql_user: Option<String>,
    },
    #[command(about = "行排序")]
    Sort {
        input: Option<String>,
        #[arg(short, long)]
        reverse: bool,
        #[arg(short = 'n', long)]
        numeric: bool,
        #[arg(short = 'k', long)]
        column: Option<usize>,
        #[arg(short = 't', long)]
        separator: Option<String>,
        #[arg(short = 'u', long)]
        unique: bool,
    },
    #[command(about = "行去重")]
    Uniq {
        input: Option<String>,
        #[arg(short = 'c', long)]
        count: bool,
        #[arg(short = 'd', long)]
        duplicates: bool,
        #[arg(short = 'i', long)]
        ignore_case: bool,
    },
    #[command(about = "列提取")]
    Cut {
        input: Option<String>,
        #[arg(short = 'd', long)]
        delimiter: Option<String>,
        #[arg(short = 'f', long, required = true)]
        fields: String,
        #[arg(short = 's', long)]
        only_delimited: bool,
    },
    #[command(about = "行/词/字符/字节统计")]
    Count {
        input: Option<String>,
        #[arg(short = 'l', long)]
        lines: bool,
        #[arg(short = 'w', long)]
        words: bool,
        #[arg(short = 'm', long)]
        chars: bool,
        #[arg(short = 'c', long)]
        bytes: bool,
        #[arg(short = 'L', long)]
        max_line: bool,
        #[arg(long, help = "JSON 输出")]
        json: bool,
    },
    #[command(about = "智能 Rust 构建")]
    Build {
        dir: Option<String>,
        #[arg(short = 't', long = "target")]
        target: Option<String>,
        #[arg(short = 'p', long = "profile")]
        profile: Option<String>,
        #[arg(short = 'b', long = "bin")]
        bin: Option<String>,
        #[arg(long)]
        features: Vec<String>,
        #[arg(long)]
        workspace: bool,
        #[arg(long)]
        list_targets: bool,
        #[arg(long)]
        no_config: bool,
    },
    #[command(about = "Rust 代码质量检查")]
    Check {
        dir: Option<String>,
        #[arg(long)]
        clippy: bool,
        #[arg(long)]
        fmt: bool,
        #[arg(long)]
        fix: bool,
    },
    #[command(about = "编译产物大小分析")]
    Size {
        dir: Option<String>,
        #[arg(short = 't', long = "target")]
        target: Option<String>,
        #[arg(short = 'p', long = "profile")]
        profile: Option<String>,
        #[arg(short = 'a', long)]
        all: bool,
        #[arg(short = 'H', long)]
        human: bool,
        #[arg(short = 's', long)]
        sort: bool,
    },
    #[command(about = "智能清理")]
    Clean {
        dir: Option<String>,
        #[arg(short = 't', long = "target")]
        target: Option<String>,
        #[arg(short = 'p', long = "profile")]
        profile: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(short = 'a', long)]
        all: bool,
    },
    #[command(about = "AI 上下文生成器 — 一次输出签名/imports/内容")]
    Ctx {
        path: PathBuf,
        #[arg(short = 'H', long, help = "限制总行数(超过自动截断)")]
        max_lines: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "文件格式统一 — 跨平台文本标准化")]
    Normalize {
        path: PathBuf,
        #[arg(short = 'e', long = "ending")]
        ending: Option<String>,
        #[arg(long)]
        remove_bom: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "rxt 自检 - 版本/配置/hosts 状态")]
    Info {
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Git 操作 - AI 友好的 git 包装 (status/diff/log/branch/add/commit/undo)")]
    Git {
        #[command(subcommand)]
        cmd: crate::git::GitSubCmd,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "v0.4.0 项目结构简报 + git HEAD 缓存引擎")]
    Map {
        dir: Option<PathBuf>,
        #[arg(short, long, default_value_t = 3, help = "结构树深度")]
        depth: usize,
        #[arg(long, help = "强制全量重算, 忽略缓存")]
        refresh: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "v0.4.0 文件骨架 — 函数体折叠, 省 token")]
    Digest {
        path: PathBuf,
        #[arg(short, long, default_value_t = 8, help = "函数体超过 N 行才折叠")]
        threshold: usize,
        #[arg(long, help = "token 预算")]
        budget: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "AI 一键简报 — map+优先digest+focus，硬预算省上下文/调用次数")]
    Pack {
        #[arg(help = "项目目录，默认 .")]
        dir: Option<PathBuf>,
        #[arg(
            short = 'b',
            long,
            default_value_t = 6000,
            help = "输出字符硬预算 (≈ budget/3.5 tokens)"
        )]
        budget: usize,
        #[arg(short = 'd', long, default_value_t = 2, help = "目录树深度")]
        depth: usize,
        #[arg(long, help = "关键词：附带紧凑 grep 命中")]
        focus: Option<String>,
        #[arg(long, help = "最多展开骨架的文件数 (默认按 budget 自适应)")]
        max_files: Option<usize>,
        #[arg(long, default_value_t = 10, help = "每文件最多符号行")]
        per_file: usize,
        #[arg(
            short = 't',
            long,
            default_value_t = 8,
            help = "函数体超过 N 行标记折叠"
        )]
        threshold: usize,
        #[arg(long, help = "不要目录树")]
        no_tree: bool,
        #[arg(long, help = "不要符号骨架")]
        no_digest: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "引用查找 — 默认列出符号所有出现(def/call); --callers 谁调用了它; --callees 它调用了谁"
    )]
    Refs {
        symbol: String,
        #[arg(short, long)]
        path: Option<PathBuf>,
        #[arg(long, help = "谁调用了 symbol(列出真实调用点)")]
        callers: bool,
        #[arg(long, help = "symbol 调用了谁(列出其函数体内的调用)")]
        callees: bool,
        #[arg(long)]
        json: bool,
    },
    // ===== v0.8.0 代码智能四件套 =====
    #[command(about = "git 历史热点 — 哪些文件改得最频繁(最易碎)")]
    Churn {
        #[arg(long, help = "时间范围 (如 '1 month', '2 weeks')")]
        since: Option<String>,
        #[arg(long, help = "按作者分组")]
        by_author: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "死代码检测 — 从入口做可达性分析, 找出不可达函数")]
    Dead {
        #[arg(long, help = "默认当前目录")]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "多跳调用链追踪 — refs 是单跳, trace 是 N 跳全图")]
    Trace {
        symbol: String,
        #[arg(long, help = "默认当前目录")]
        path: Option<PathBuf>,
        #[arg(short, long, default_value_t = 3, help = "追踪深度(跳数)")]
        depth: usize,
        #[arg(long, help = "向上追踪(谁调用了它), 默认向下(它调用了谁)")]
        up: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "改动爆炸半径 — 给定改动文件, 算出受影响的调用者链")]
    Impact {
        #[arg(num_args = 0..)]
        files: Vec<PathBuf>,
        #[arg(long, help = "自动取 git diff 改动的文件")]
        diff: bool,
        #[arg(long, help = "默认当前目录")]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "一键发布 — 编译两平台 + 装本地 + 部署远程 + git push")]
    Publish {
        #[arg(long, help = "仓库路径(默认自动探测)")]
        repo: Option<String>,
        #[arg(long, help = "不部署远程机器")]
        no_deploy: bool,
        #[arg(long, help = "不 git push")]
        no_push: bool,
        #[arg(short, long, help = "commit message")]
        message: Option<String>,
    },
    // ===== 系统命令族 (v0.4.0+) — 对标 PowerShell =====
    #[command(about = "系统信息 - OS/CPU/内存/磁盘/网络 (all|os|cpu|mem|disk|net)")]
    Sysinfo {
        section: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "进程列表/查杀 (对标 Get/Stop-Process)")]
    Ps {
        #[arg(long, help = "按名称过滤(支持 * 通配)")]
        name: Option<String>,
        #[arg(long = "kill", help = "终止进程(PID 或名称)")]
        kill: Option<String>,
        #[arg(short = 'n', long, default_value_t = 20, help = "显示前 N 条(0=全部)")]
        top: usize,
        #[arg(long = "sort", default_value = "mem", help = "排序: mem|cpu|pid|name")]
        sort: String,
        #[arg(long)]
        tree: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Windows 服务管理 (对标 Get/Start/Stop-Service)")]
    Service {
        #[arg(long, help = "按名称过滤(支持 * 通配)")]
        name: Option<String>,
        #[arg(long = "start")]
        start: Option<String>,
        #[arg(long = "stop")]
        stop: Option<String>,
        #[arg(long, help = "显示正在运行的")]
        running: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "注册表读写 (对标 Get/Set/Remove-ItemProperty)")]
    Reg {
        #[arg(long, help = "读: HKLM\\Software\\... 键路径")]
        get: Option<String>,
        #[arg(long, help = "写: 路径 --name X --value Y")]
        set: Option<String>,
        #[arg(long, help = "删除值")]
        delete: Option<String>,
        #[arg(long = "name", help = "值名(默认 = 默认值)")]
        value_name: Option<String>,
        #[arg(long, help = "写入的值")]
        value: Option<String>,
        #[arg(long = "list", help = "列出键下所有值和子键")]
        list: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "网络 - TCP连接/路由/DNS (对标 Get-Net*/Resolve-DnsName)")]
    Net {
        #[arg(long, help = "TCP 连接(可加状态: listen/established)")]
        conn: Option<String>,
        #[arg(long, help = "DNS 解析")]
        resolve: Option<String>,
        #[arg(long, help = "路由表")]
        route: bool,
        #[arg(long, help = "端口监听检查")]
        port: Option<String>,
        #[arg(long)]
        json: bool,
    },
    // ===== 高效工具族 (v0.4.0+) =====
    #[command(about = "自我更新 — git pull + 编译 + 热替换(自举封神)")]
    /// 一键部署二进制到远程机器 (自动处理进程占用 + 字节验证)
    Deploy {
        binary: PathBuf,
        #[arg(short = 't', long = "to")]
        to: Vec<String>,
        #[arg(long)]
        all: bool,
        #[arg(long = "remote-path")]
        remote_path: Option<String>,
    },
    /// 批量查询版本 + 一致性检测
    Version {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        host: Option<String>,
    },
    /// 跨机目录同步 (rsync 替代)
    Sync {
        local_dir: PathBuf,
        remote: String,
        #[arg(long)]
        delete: bool,
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    Upgrade {
        #[arg(long, help = "仓库路径(默认自动探测)")]
        repo: Option<String>,
        #[arg(long, help = "只检查不升级")]
        check: bool,
        #[arg(long, help = "指定 feature")]
        features: Option<String>,
        #[arg(long, help = "只 pull 不编译")]
        no_build: bool,
    },
    #[command(about = "HTTP 文件服务器 — 手机扫码秒访问")]
    Serve {
        dir: Option<String>,
        #[arg(short, long, default_value_t = 8000)]
        port: u16,
        #[arg(long, help = "不显示二维码")]
        no_qr: bool,
    },
    #[command(about = "文件/目录时光机 — 快照 + 回滚")]
    Snapshot {
        target: Option<String>,
        #[arg(short, long, help = "快照标签")]
        label: Option<String>,
        #[arg(long, help = "列出快照")]
        list: bool,
        #[arg(long, help = "回滚到某快照")]
        restore: Option<String>,
        #[arg(long, help = "对比快照与当前")]
        diff: Option<String>,
        #[arg(long, help = "清理 N 天前的")]
        clean: Option<u64>,
    },
    #[command(about = "终端二维码(扫码访问)")]
    Qr {
        text: Option<String>,
        #[arg(long)]
        invert: bool,
        #[arg(long)]
        compact: bool,
    },
    #[command(about = "系统剪贴板读写 (read/write/clear)")]
    Clip {
        action: String,
        content: Option<String>,
        #[arg(long, help = "从文件读取内容写入剪贴板")]
        file: Option<String>,
    },
    #[command(about = "轮询重试直到成功/超时 (等端口/文件/命令)")]
    Repeat {
        cmd: Option<String>,
        #[arg(long, help = "等文件出现")]
        file: Option<String>,
        #[arg(long, help = "等端口可连(如 5432 或 host:port)")]
        port: Option<String>,
        #[arg(long, help = "等主机 ping 通")]
        ping: Option<String>,
        #[arg(long, default_value_t = 60, help = "超时秒数")]
        timeout: u64,
        #[arg(long, default_value_t = 1000, help = "重试间隔毫秒")]
        interval: u64,
        #[arg(long, default_value_t = 0, help = "最大尝试次数(0=不限)")]
        tries: usize,
    },
    #[command(about = "桌面通知 (长任务完成提醒)")]
    Notify {
        message: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "info", help = "info|success|warn|error")]
        level: String,
    },
    #[command(about = "按内容哈希找重复文件 (清理重复照片/下载)")]
    Dup {
        dir: String,
        #[arg(long, default_value = "", help = "最小文件大小(如 1M/500K)")]
        min_size: String,
        #[arg(long, help = "扩展名过滤(逗号分隔 jpg,png)")]
        ext: Option<String>,
        #[arg(long, help = "删除重复(保留每组第一个)")]
        delete: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "安全删除 — 进回收站 + 恢复 (终结 rm 误删)")]
    Trash {
        paths: Vec<String>,
        #[arg(long, help = "列出回收站")]
        list: bool,
        #[arg(long, help = "恢复某项")]
        restore: Option<String>,
        #[arg(long, help = "恢复到指定目录")]
        to: Option<String>,
        #[arg(long, help = "清理 N 天前的")]
        clean: Option<u64>,
        #[arg(long, help = "清空回收站")]
        purge: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "命令宏 — 重复操作变一个词 (add/list/run/show/rm)")]
    Recipe {
        action: String,
        name: Option<String>,
        content: Option<String>,
        #[arg(num_args = 0.., allow_hyphen_values = true, help = "run 时的参数 $1 $2")]
        args: Vec<String>,
        #[arg(long, help = "只看不执行")]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "性能基准 — 跑 N 次取统计 + 对比")]
    Bench {
        cmds: Vec<String>,
        #[arg(short = 'n', long, default_value_t = 10, help = "运行次数")]
        runs: usize,
        #[arg(long, default_value_t = 1, help = "预热次数")]
        warmup: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "文件变化自动重跑 (替代 nodemon/entr)")]
    WatchRun {
        cmd: String,
        #[arg(num_args = 0.., help = "监控目录(默认当前)")]
        paths: Vec<String>,
        #[arg(long, default_value = "", help = "扩展名过滤(逗号分隔 rs,py)")]
        ext: String,
        #[arg(long, default_value_t = 500, help = "防抖毫秒")]
        debounce: u64,
        #[arg(long, help = "启动时先跑一次")]
        run_on_start: bool,
    },
    #[command(about = "差分测试 — 代码进化验证(自举/重构/移植,对比双实现输出一致率)")]
    Evolve {
        #[arg(long, help = "参照实现(含{input}占位符)")]
        reference: String,
        #[arg(long, help = "候选实现(含{input}占位符)")]
        candidate: String,
        #[arg(long, help = "输入集(目录/glob/逗号列表)")]
        inputs: String,
        #[arg(long, default_value = "exact", help = "对比模式: exact|json|exitcode")]
        mode: String,
        #[arg(long, default_value_t = 15, help = "单个超时秒数")]
        timeout: u64,
        #[arg(long, help = "首个失败即停,显示diff")]
        first_fail: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "MCP server 模式 (stdio JSON-RPC；默认 --slim 只暴露省 token 工具)")]
    Mcp {
        #[arg(long, help = "SSE 端口(暂未实现,本地用 stdio)")]
        sse: Option<u16>,
        #[arg(long, help = "暴露全部命令(肥 schema，一般别开)")]
        full: bool,
        #[arg(
            long,
            default_value_t = true,
            help = "精简工具集 pack/map/digest/refs/grep/read/write/cat/find/impact/ls/ctx"
        )]
        slim: bool,
    },
    #[command(about = "外挂插件 — Git 风格；new/add/install/edit/show/remove/which")]
    Plugin {
        #[arg(
            help = "list | new | add | install | edit | show | remove | which",
            default_value = "list"
        )]
        action: String,
        #[arg(help = "插件名，或 install 的路径")]
        target: Option<String>,
        #[arg(help = "new/add 时的脚本正文")]
        content: Option<String>,
        #[arg(long, help = "install 时指定名称；new 时覆盖名字")]
        name: Option<String>,
        #[arg(long, help = "覆盖同名内置，或覆盖已安装")]
        force: bool,
        #[arg(long)]
        json: bool,
        #[arg(
            long,
            help = "new 语言: sh|py|cmd|ps1（默认 Linux sh / Windows cmd，Git Bash 下 sh）"
        )]
        lang: Option<String>,
        #[arg(long, help = "脚本正文（也可用位置参数）")]
        body: Option<String>,
        #[arg(long, help = "从 stdin 读脚本正文")]
        stdin: bool,
        #[arg(long, help = "new 之后打开编辑器")]
        open: bool,
    },
    #[command(about = "Windows 代码签名（自签 rxt-codesign）")]
    Sign {
        #[arg(help = "要签的 exe（默认当前 rxt）")]
        exe: Option<PathBuf>,
        #[arg(long, help = "导入 TrustedPublisher（可能要管理员）")]
        trust: bool,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand, Clone)]
pub enum MemAction {
    /// 保存记忆 → POST /memory/add
    #[command(about = "保存记忆到星枢")]
    Save {
        content: String,
        #[arg(long, default_value = "fact")]
        category: String,
        #[arg(long, default_value_t = 0.6)]
        importance: f64,
    },
    /// 搜索 → POST /ask
    #[command(about = "星枢 /ask 搜索（contract+pack）", visible_alias = "ask")]
    Search {
        query: String,
        #[arg(short = 'k', long = "top-k", default_value = "5")]
        top_k: usize,
    },
    /// 统计
    #[command(about = "星枢 /v5/health + stats")]
    Stats,
    /// 会话抽取
    #[command(about = "会话抽取写回星枢")]
    Extract {
        /// 会话文本
        transcript: String,
        #[arg(long, default_value = "")]
        focus: String,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// 开场 bootstrap
    #[command(about = "会话开场 bootstrap 注入")]
    Bootstrap {
        focus: String,
        #[arg(long, default_value_t = 2000)]
        budget: u32,
    },
    /// 分层计划
    #[command(about = "Letta 分层调用计划")]
    Layers {
        #[arg(default_value = "")]
        focus: String,
    },
    /// 短帮助
    #[command(about = "mem 用法（短）")]
    Help,
}

/// GNU find 兼容：`-name`/`-type`/`-path` → clap long flag（仅 find 子命令上下文）
fn normalize_gnu_find_flags(args: &mut [String]) {
    let is_find = args.iter().any(|a| a == "find");
    if !is_find {
        return;
    }
    for a in args.iter_mut() {
        match a.as_str() {
            "-name" => *a = "--name".into(),
            "-type" => *a = "--type".into(),
            "-path" => *a = "--path".into(),
            _ => {}
        }
    }
}

/// clap 在 debug 构建里展开大量子命令会吃掉主线程默认栈（Windows 必现，Linux 加 plugin 字段后测试也会炸）。
/// 只把解析放到大栈线程，业务命令仍在主线程执行。
pub fn parse_cli(args: Vec<String>) -> anyhow::Result<Result<Cli, clap::Error>> {
    std::thread::Builder::new()
        .name("rxt-clap".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || Cli::try_parse_from(args))
        .map_err(|e| anyhow::anyhow!("启动 CLI 解析线程失败: {e}"))?
        .join()
        .map_err(|_| anyhow::anyhow!("CLI 解析线程异常退出"))
}

fn invocation_plugin_name(argv0: &str) -> Option<String> {
    if let Ok(n) = std::env::var("RXT_PLUGIN_NAME") {
        let n = n.trim().to_ascii_lowercase();
        if !n.is_empty() && n != "tools" && n != "rxt-tools" {
            return Some(n);
        }
    }
    let stem = std::path::Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = stem.strip_prefix("rxt-").unwrap_or(&stem);
    if name.is_empty() || name == "rxt" || name == "tools" || name == "rxt-tools" {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn run() -> anyhow::Result<()> {
    crate::common::setup_utf8_console();
    // password_env：从 ~/.rxt/env 注入（Agent/非登录壳也能用）
    let _ = crate::hosts::HostsFile::load_dotenv();

    // Handle --describe before clap parses (since --describe needs to be a top-level flag,
    // and adding it to Cli struct requires changes).
    // Use raw arg parsing for this.
    let mut args: Vec<String> = std::env::args().collect();
    // 少数包装会把 exe 路径再塞进 argv[1]
    if args.len() >= 2 && args[1] == args[0] {
        args.remove(1);
    }
    if let Some(p) = invocation_plugin_name(args.first().map(|s| s.as_str()).unwrap_or("")) {
        if args.get(1).map(|s| s.as_str()) != Some(p.as_str()) {
            args.insert(1, p);
        }
        if let Some(a0) = args.get_mut(0) {
            *a0 = "rxt".into();
        }
    }
    if args.iter().any(|a| a == "--describe") {
        return describe::run();
    }
    normalize_gnu_find_flags(&mut args);

    let raw_host = plugin::peek_flag(&args, "--host");
    let raw_group = plugin::peek_flag(&args, "--group");
    if let Some((name, rest)) = plugin::peek_subcommand(&args) {
        let rest = plugin::strip_global_flags(&rest);
        if plugin::run_forced_override(&name, &rest, raw_host.as_deref(), raw_group.as_deref())? {
            return Ok(());
        }
    }

    let cli = match parse_cli(args)? {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    // v0.4.2: Deploy/Version 有自己的 group 逻辑，不走全局批量拦截
    match &cli.command {
        Command::Deploy {
            binary,
            to,
            all,
            remote_path,
        } => {
            let (targets, is_group) = if *all {
                (vec!["all".to_string()], true)
            } else {
                (to.clone(), false)
            };
            return deploy::run(binary, &targets, is_group, remote_path.as_deref());
        }
        Command::Version { all, host } => {
            if *all {
                return version::run_remote("all", true);
            }
            if let Some(h) = host {
                return version::run_remote(h, false);
            }
            return version::run_local();
        }
        Command::Plugin {
            action,
            target,
            content,
            name,
            force,
            json,
            lang,
            body,
            stdin,
            open,
        } => {
            return plugin::run(plugin::PluginCli {
                action,
                target: target.as_deref(),
                name: name.as_deref(),
                content: content.as_deref(),
                body: body.as_deref(),
                force: *force,
                json: *json,
                lang: lang.as_deref(),
                stdin: *stdin,
                open: *open,
            });
        }
        Command::Sign { exe, trust } => {
            return sign::run(exe.as_deref(), *trust);
        }
        Command::External(args) => {
            let args = plugin::strip_global_flags(args);
            return plugin::run_external(
                &args,
                cli.host.as_deref().or(raw_host.as_deref()),
                cli.group.as_deref().or(raw_group.as_deref()),
            );
        }
        _ => {}
    }

    // mem 不需要 SFTP/远端 rxt 通道：--host 只映射到 RXT_NEBULA_SSH（星枢 ssh 跳板）
    if matches!(cli.command, Command::Mem { .. }) {
        if let Some(ref host) = cli.host {
            if std::env::var("RXT_NEBULA_SSH").is_err() {
                // SAFETY: 进程早期、单线程主路径，设置跳板主机名供 mem 使用
                std::env::set_var("RXT_NEBULA_SSH", host);
            }
        }
        return execute_command(cli.command, None);
    }

    // 如果有 --group,批量执行
    // v0.5.1: 统一执行路径 — group 和 host 都调用 execute_command
    if let Some(ref group_name) = cli.group {
        let hosts_config = crate::hosts::HostsFile::load()?;
        let members = hosts_config.get_group_members(group_name)?;
        for member in &members {
            eprintln!("\n=== [{}] ===", member);
            let mut rc = crate::remote::RemoteChannel::connect(member)?;
            execute_command(cli.command.clone(), Some(&mut rc))?;
        }
        return Ok(());
    }

    // --host: 建立远程连接
    let mut remote_channel = if let Some(ref host) = cli.host {
        Some(crate::remote::RemoteChannel::connect(host)?)
    } else {
        None
    };

    execute_command(cli.command, remote_channel.as_mut())?;
    Ok(())
}

/// v0.5.1: 统一命令分发 — 消灭 group/host 两套重复 dispatch
fn execute_command(
    cmd: Command,
    mut remote: Option<&mut crate::remote::RemoteChannel>,
) -> anyhow::Result<()> {
    match cmd {
        Command::Replace {
            target,
            old,
            new,
            all,
            preview,
            content,
        } => {
            let nc: Option<String> = if let Some(f) = new {
                Some(std::fs::read_to_string(f)?)
            } else if !content.is_empty() {
                Some(content.join("\n"))
            } else {
                None
            };
            replace::run(
                &target,
                &old,
                nc.as_deref(),
                all,
                preview,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Read {
            path,
            encoding,
            number,
            head,
            tail,
            lines,
            budget,
            json,
        } => {
            read::run(
                &path,
                encoding,
                number,
                head,
                tail,
                lines,
                budget,
                json,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Write {
            path,
            content,
            append,
            file,
            b64,
            preserve,
            from,
        } => {
            if let Some(f) = from {
                let data = std::fs::read(&f)?;
                write::run_bytes(
                    &path,
                    &data,
                    append,
                    preserve,
                    remote.as_ref().map(|r| &**r),
                )?;
            } else if let Some(f) = file {
                write::run_file(&path, &f, append, remote.as_ref().map(|r| &**r))?;
            } else if b64 {
                let j = content.join("");
                write::run_b64(&path, &j, append, remote.as_ref().map(|r| &**r))?;
            } else {
                let j = content.join("\n");
                write::run(
                    &path,
                    if content.is_empty() { None } else { Some(&j) },
                    append,
                    preserve,
                    remote.as_ref().map(|r| &**r),
                )?;
            }
        }
        Command::Cat { path } => {
            let storage = crate::storage::Storage::from_remote(remote.as_ref().map(|r| &**r));
            if storage.is_remote() {
                let (text, _) = storage.read_text(&path)?;
                print!("{}", text);
            } else {
                cat::run(&path)?;
            }
        }
        Command::Jsonl { path, last, json } => jsonl::run(&path, last, json)?,
        Command::Stat { path, json } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 远端有 rxt 时优先调原生 stat (格式统一)
                let mut args: Vec<String> =
                    vec!["stat".into(), path.to_string_lossy().into_owned()];
                if json {
                    args.push("--json".into());
                }
                let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&arg_refs) {
                    print!("{}", out);
                    return Ok(());
                }
                return stat::run(&path, json, Some(&**rc));
            }
            stat::run(&path, json, None)?;
        }
        Command::Find {
            query,
            path,
            name_pattern,
            file_type,
            context,
            case_sensitive,
            count,
            stats,
            replace,
            replace_with,
            preview,
            regex,
            json,
            max_results,
            head,
            offset,
        } => {
            find::run(
                query.as_deref(),
                path.as_deref(),
                name_pattern.as_deref(),
                file_type.as_deref(),
                context,
                case_sensitive,
                count,
                stats,
                replace.as_deref(),
                replace_with.as_deref(),
                preview,
                regex,
                json,
                max_results,
                head,
                offset,
                remote.as_mut().map(|r| &mut **r),
            )?;
        }
        Command::Struct {
            path,
            functions,
            types,
            deep,
            extract,
            json,
        } => {
            struct_mod::run(&path, functions, types, deep, extract.as_deref(), json)?;
        }
        Command::Diff {
            first,
            second,
            context,
            stat,
            ai,
            side_by_side,
            json,
        } => {
            diff::run(
                &first,
                second.as_deref(),
                context,
                stat,
                ai,
                side_by_side,
                json,
            )?;
        }
        Command::Dep {
            target,
            tree,
            json,
            check,
        } => dep::run(&target, tree, json, check)?,
        Command::Sed {
            path,
            pattern,
            replacement,
            preview,
            line,
            regex,
        } => {
            sed::run(
                &path,
                &pattern,
                &replacement,
                preview,
                line,
                regex,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Grep {
            pattern,
            path,
            context,
            file_type,
            count,
            invert,
            json,
            regex,
            max_results,
            head,
            offset,
            jsonl,
            no_ignore,
        } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 远端有 rxt 时优先调原生 grep (rayon 并行, 行为一致, 无版本差异)
                let mut args: Vec<String> = vec![
                    "grep".into(),
                    pattern.clone(),
                    path.to_string_lossy().into_owned(),
                ];
                args.push("--context".into());
                args.push(context.to_string());
                if let Some(t) = &file_type {
                    args.push("--type".into());
                    args.push(t.clone());
                }
                if count {
                    args.push("--count".into());
                }
                if invert {
                    args.push("--invert".into());
                }
                if json {
                    args.push("--json".into());
                }
                if regex {
                    args.push("--regex".into());
                }
                if let Some(m) = max_results {
                    args.push("--max-results".into());
                    args.push(m.to_string());
                }
                if let Some(h) = head {
                    args.push("--head".into());
                    args.push(h.to_string());
                }
                args.push("--offset".into());
                args.push(offset.to_string());
                if jsonl {
                    args.push("--jsonl".into());
                }
                if no_ignore {
                    args.push("--no-ignore".into());
                }
                let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&arg_refs) {
                    print!("{}", out);
                    return Ok(());
                }
                return grep::run(
                    &pattern,
                    &path,
                    context,
                    file_type.as_deref(),
                    count,
                    invert,
                    json,
                    regex,
                    max_results,
                    head,
                    offset,
                    jsonl,
                    no_ignore,
                    Some(&**rc),
                );
            }
            grep::run(
                &pattern,
                &path,
                context,
                file_type.as_deref(),
                count,
                invert,
                json,
                regex,
                max_results,
                head,
                offset,
                jsonl,
                no_ignore,
                None,
            )?;
        }
        Command::Search {
            query,
            path,
            file_type,
            name,
            content,
            json,
            max_results,
        } => {
            search::run(
                &query,
                path.as_deref(),
                file_type.as_deref(),
                name,
                content,
                json,
                max_results,
            )?;
        }
        Command::Patch {
            paths,
            reverse,
            check,
            output,
        } => {
            patch::run(&paths, reverse, check, output.as_deref())?;
        }
        Command::Py { code, file } => py::run(code.as_deref(), file.as_ref())?,
        Command::Mem { action } => match action {
            MemAction::Save {
                content,
                category,
                importance,
            } => mem::run_save(&content, &category, importance)?,
            MemAction::Search { query, top_k } => mem::run_search(&query, top_k)?,
            MemAction::Stats => mem::run_stats()?,
            MemAction::Extract {
                transcript,
                focus,
                dry_run,
            } => mem::run_extract(&transcript, &focus, dry_run)?,
            MemAction::Bootstrap { focus, budget } => mem::run_bootstrap(&focus, budget)?,
            MemAction::Layers { focus } => mem::run_layers(&focus)?,
            MemAction::Help => mem::run_help()?,
        },
        Command::Tree {
            path,
            depth,
            ignore,
            dirs_only,
            json,
        } => {
            let ignores: Vec<String> = ignore
                .as_deref()
                .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                .unwrap_or_default();
            if let Some(ref mut rc) = remote {
                // v0.7.5: 远端有 rxt 时优先调原生实现 (纯Rust, 不依赖远端有 tree 命令)
                let mut args: Vec<String> =
                    vec!["tree".into(), path.to_string_lossy().into_owned()];
                if let Some(d) = depth {
                    args.push("--depth".into());
                    args.push(d.to_string());
                }
                if !ignores.is_empty() {
                    args.push("--ignore".into());
                    args.push(ignores.join(","));
                }
                if dirs_only {
                    args.push("--dirs-only".into());
                }
                if json {
                    args.push("--json".into());
                }
                let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&arg_refs) {
                    print!("{}", out);
                    return Ok(());
                }
                return tree::run(&path, depth, &ignores, dirs_only, json, Some(&**rc));
            }
            tree::run(&path, depth, &ignores, dirs_only, json, None)?;
        }
        Command::Jq {
            query,
            file,
            fmt,
            compact,
            raw,
            slurp,
        } => jq::run(query.as_deref(), file.as_deref(), fmt, compact, raw, slurp)?,
        Command::Unzip {
            archive,
            target,
            list_only,
            json,
            strip,
        } => unzip::run(&archive, target.as_deref(), list_only, json, strip)?,
        Command::Ls {
            dir,
            json,
            all,
            sort,
            depth,
            max,
        } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 远端有 rxt 时优先调远端原生实现 (输出统一, 无 GBK/编码问题)
                let mut args: Vec<String> = vec!["ls".into(), dir.to_string_lossy().into_owned()];
                if json {
                    args.push("--json".into());
                }
                if all {
                    args.push("--all".into());
                }
                if let Some(s) = &sort {
                    args.push("--sort".into());
                    args.push(s.clone());
                }
                if let Some(d) = depth {
                    args.push("--depth".into());
                    args.push(d.to_string());
                }
                if let Some(m) = max {
                    args.push("--max".into());
                    args.push(m.to_string());
                }
                let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&arg_refs) {
                    print!("{}", out);
                    return Ok(());
                }
                // 降级: 远端无 rxt, 走原 shell 模式
                return ls::run(&dir, json, all, sort.as_deref(), depth, max, Some(&**rc));
            }
            ls::run(&dir, json, all, sort.as_deref(), depth, max, None)?
        }
        Command::Http {
            method,
            urls,
            headers,
            data,
            json_body,
            auth,
            timeout,
            show_headers,
            body_only,
            output,
            browser,
            cookie_jar,
            cookies,
            user_agent,
            text,
            links,
            budget,
            form,
            no_probe,
            cookie_json,
            select,
            session,
            engine,
        } => http::run(http::HttpOpts {
            method: &method,
            urls: &urls,
            headers: &headers,
            data: data.as_deref(),
            json_body,
            auth: auth.as_deref(),
            timeout,
            show_headers,
            body_only,
            output: output.as_deref(),
            browser: browser.as_deref(),
            cookie_jar: cookie_jar.as_deref(),
            cookies: &cookies,
            user_agent: user_agent.as_deref(),
            text,
            links,
            budget,
            form: &form,
            no_probe,
            cookie_json: cookie_json.as_deref(),
            select: select.as_deref(),
            session: session.as_deref(),
            engine: engine.as_deref(),
        })?,
        Command::Edit {
            path,
            after,
            before,
            delete,
            replace,
            content,
            preview,
            script,
            line_range,
            regex,
        } => {
            let rep = replace.as_deref().and_then(|s| {
                let mut p = s.splitn(2, ',');
                Some((p.next()?, p.next()?))
            });
            if let Some(sp) = script {
                edit::run_script(&path, &sp, preview, remote.as_ref().map(|r| &**r))?;
            } else if let Some(lr) = line_range {
                edit::run_line_range(
                    &path,
                    lr.as_str(),
                    &content,
                    preview,
                    remote.as_ref().map(|r| &**r),
                )?;
            } else {
                edit::run(
                    &path,
                    after.as_deref(),
                    before.as_deref(),
                    delete.as_deref(),
                    rep,
                    &content,
                    preview,
                    regex,
                    remote.as_ref().map(|r| &**r),
                )?;
            }
        }
        Command::Hash { path, algo, text } => hash::run(path.as_deref(), &algo, text.as_deref())?,
        Command::Uuid { count } => uuidgen::run(count)?,
        Command::Enc {
            mode,
            input,
            decode,
            file,
        } => {
            let fc;
            let is: Option<&str> = if let Some(f) = file {
                fc = std::fs::read_to_string(f)?;
                Some(&fc)
            } else {
                input.as_deref()
            };
            enc::run(&mode, is, decode)?;
        }
        Command::Dec { mode, input, file } => {
            let fc;
            let is: Option<&str> = if let Some(f) = file {
                fc = std::fs::read_to_string(f)?;
                Some(&fc)
            } else {
                input.as_deref()
            };
            enc::run(&mode, is, true)?;
        }
        Command::Watch {
            patterns,
            cmd,
            path,
            debounce,
        } => watch::run(&patterns, &cmd, path.as_deref(), debounce)?,
        Command::Tail {
            path,
            filter,
            interval,
            lines,
            once,
        } => tail::run(&path, filter.as_deref(), interval, lines, once)?,
        Command::Time { cmd } => timecmd::run(&cmd)?,
        Command::Exec {
            code,
            b64,
            lang,
            write,
            file,
            login,
            json,
            container,
            db,
            sql_user,
        } => {
            let cs = if let Some(f) = file {
                std::fs::read_to_string(f)?
            } else {
                code.unwrap_or_default()
            };
            exec::run(
                &cs,
                b64,
                lang.as_deref(),
                write.as_ref(),
                remote.as_ref().map(|r| &**r),
                login,
                json,
                container.as_deref(),
                db.as_deref(),
                sql_user.as_deref(),
            )?;
        }
        Command::Sort {
            input,
            reverse,
            numeric,
            column,
            separator,
            unique,
        } => {
            sort::run(
                input.as_deref(),
                reverse,
                numeric,
                column,
                separator,
                unique,
            )?;
        }
        Command::Uniq {
            input,
            count,
            duplicates,
            ignore_case,
        } => {
            uniq::run(input.as_deref(), count, duplicates, ignore_case)?;
        }
        Command::Cut {
            input,
            delimiter,
            fields,
            only_delimited,
        } => {
            cut::run(input.as_deref(), delimiter, &fields, only_delimited)?;
        }
        Command::Count {
            input,
            lines,
            words,
            chars,
            bytes,
            max_line,
            json,
        } => {
            count::run(input.as_deref(), lines, words, chars, bytes, max_line, json)?;
        }
        Command::Build {
            dir,
            target,
            profile,
            bin,
            features,
            workspace,
            list_targets,
            no_config,
        } => {
            build::run(
                dir.as_deref(),
                target.as_deref(),
                profile.as_deref(),
                bin.as_deref(),
                features,
                workspace,
                list_targets,
                no_config,
            )?;
        }
        Command::Check {
            dir,
            clippy,
            fmt,
            fix,
        } => check::run(dir.as_deref(), clippy, fmt, fix)?,
        Command::Size {
            dir,
            target,
            profile,
            all,
            human,
            sort,
        } => {
            size::run(
                dir.as_deref(),
                target.as_deref(),
                profile.as_deref(),
                all,
                human,
                sort,
            )?;
        }
        Command::Clean {
            dir,
            target,
            profile,
            dry_run,
            all,
        } => {
            clean::run(
                dir.as_deref(),
                target.as_deref(),
                profile.as_deref(),
                dry_run,
                all,
            )?;
        }
        Command::Ctx {
            path,
            max_lines,
            json,
        } => {
            ctx::run(&path, max_lines, json, remote.as_ref().map(|r| &**r))?;
        }
        Command::Normalize {
            path,
            ending,
            remove_bom,
            json,
        } => {
            normalize::run(&path, ending.as_deref(), remove_bom, json)?;
        }
        Command::Info { json } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 优先调远端 rxt info; 无 rxt 则降级提示
                if let Some(out) = rc.try_exec_rxt(&["info"]) {
                    print!("{}", out);
                } else {
                    println!("远端未安装 rxt, 无法获取 info (本地版: rxt info)");
                }
            } else {
                info::run(json)?;
            }
        }
        Command::Git { cmd, json } => {
            git::run(cmd, json)?;
        }
        Command::Map {
            dir,
            depth,
            refresh,
            json,
        } => {
            let d = dir.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            // 远程：优先远端 rxt map，一次回传（本地路径对远端无意义）
            if let Some(ref mut rc) = remote {
                let mut args = vec![
                    "map".to_string(),
                    d.display().to_string(),
                    "-d".to_string(),
                    depth.to_string(),
                ];
                if refresh {
                    args.push("--refresh".into());
                }
                if json {
                    args.push("--json".into());
                }
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&refs) {
                    print!("{}", out);
                    return Ok(());
                }
            }
            map::run(d, json, refresh, depth)?;
        }
        Command::Digest {
            path,
            threshold,
            budget,
            json,
        } => {
            if let Some(ref mut rc) = remote {
                let mut args = vec![
                    "digest".to_string(),
                    path.display().to_string(),
                    "-t".to_string(),
                    threshold.to_string(),
                ];
                if let Some(b) = budget {
                    args.push("--budget".into());
                    args.push(b.to_string());
                }
                if json {
                    args.push("--json".into());
                }
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&refs) {
                    print!("{}", out);
                    return Ok(());
                }
            }
            digest::run(&path, threshold, budget, json)?;
        }
        Command::Pack {
            dir,
            budget,
            depth,
            focus,
            max_files,
            per_file,
            threshold,
            no_tree,
            no_digest,
            json,
        } => {
            let d = dir.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            pack::run(
                d,
                budget,
                depth,
                focus.as_deref(),
                max_files,
                per_file,
                threshold,
                no_tree,
                no_digest,
                json,
                remote.as_deref_mut(),
            )?;
        }
        Command::Refs {
            symbol,
            path,
            callers,
            callees,
            json,
        } => {
            let p = path.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            if let Some(ref mut rc) = remote {
                let mut args = vec![
                    "refs".to_string(),
                    symbol.clone(),
                    "-p".to_string(),
                    p.display().to_string(),
                ];
                if callers {
                    args.push("--callers".into());
                }
                if callees {
                    args.push("--callees".into());
                }
                if json {
                    args.push("--json".into());
                }
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&refs) {
                    print!("{}", out);
                    return Ok(());
                }
            }
            refs::run(&symbol, p, callers, callees, json)?;
        }
        // v0.8.0 代码智能四件套
        Command::Churn {
            since,
            by_author,
            json,
        } => {
            churn::run(since.as_deref(), by_author, json)?;
        }
        Command::Dead { path, json } => {
            let p = path.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            dead::run(p, json)?;
        }
        Command::Trace {
            symbol,
            path,
            depth,
            up,
            json,
        } => {
            let p = path.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            trace::run(&symbol, p, depth, up, json)?;
        }
        Command::Impact {
            files,
            diff,
            path,
            json,
        } => {
            let p = path.as_deref().unwrap_or_else(|| std::path::Path::new("."));
            impact::run(&files, diff, p, json)?;
        }
        Command::Publish {
            repo,
            no_deploy,
            no_push,
            message,
        } => {
            publish::run(repo.as_deref(), no_deploy, no_push, message.as_deref())?;
        }
        Command::Sysinfo { section, json } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 优先调远端 rxt sysinfo; 无 rxt 则降级提示
                let sec = section.as_deref().unwrap_or("all");
                if let Some(out) = rc.try_exec_rxt(&["sysinfo", sec]) {
                    print!("{}", out);
                } else {
                    println!("远端未安装 rxt, 无法获取系统信息");
                }
            } else {
                sysinfo::run(section.as_deref().unwrap_or("all"), json)?;
            }
        }
        Command::Ps {
            name,
            kill,
            top,
            sort,
            tree,
            json,
        } => {
            if let Some(ref mut rc) = remote {
                // v0.7.5: 远端有 rxt 时优先调原生实现
                let mut args: Vec<String> = vec!["ps".into()];
                if let Some(n) = &name {
                    args.push("--name".into());
                    args.push(n.clone());
                }
                if let Some(k) = &kill {
                    args.push("--kill".into());
                    args.push(k.clone());
                }
                args.push("--top".into());
                args.push(top.to_string());
                args.push("--sort".into());
                args.push(sort.clone());
                if tree {
                    args.push("--tree".into());
                }
                if json {
                    args.push("--json".into());
                }
                let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                if let Some(out) = rc.try_exec_rxt(&arg_refs) {
                    print!("{}", out);
                    return Ok(());
                }
                return ps::run(
                    name.as_deref(),
                    kill.as_deref(),
                    top,
                    sort.as_str(),
                    tree,
                    json,
                    Some(&**rc),
                );
            }
            ps::run(
                name.as_deref(),
                kill.as_deref(),
                top,
                sort.as_str(),
                tree,
                json,
                None,
            )?;
        }
        Command::Service {
            name,
            start,
            stop,
            running,
            json,
        } => {
            service::run(
                name.as_deref(),
                start.as_deref(),
                stop.as_deref(),
                running,
                json,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Reg {
            get,
            set,
            delete,
            value_name,
            value,
            list,
            json,
        } => {
            reg::run(
                get.as_deref(),
                set.as_deref(),
                delete.as_deref(),
                value_name.as_deref(),
                value.as_deref(),
                list.as_deref(),
                json,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Net {
            conn,
            resolve,
            route,
            port,
            json,
        } => {
            net::run(
                conn.as_deref(),
                resolve.as_deref(),
                route,
                port.as_deref(),
                json,
                remote.as_ref().map(|r| &**r),
            )?;
        }
        Command::Upgrade {
            repo,
            check,
            features,
            no_build,
        } => {
            upgrade::run(repo.as_deref(), check, features.as_deref(), no_build)?;
        }
        // Deploy/Version/Sync/Plugin/Sign/External 在 main() 前置处理, 不会到达这里
        Command::Deploy { .. }
        | Command::Version { .. }
        | Command::Sync { .. }
        | Command::Plugin { .. }
        | Command::Sign { .. }
        | Command::External(_) => unreachable!(),
        Command::Serve { dir, port, no_qr } => {
            serve::run(dir.as_deref(), port, no_qr)?;
        }
        Command::Snapshot {
            target,
            label,
            list,
            restore,
            diff,
            clean,
        } => {
            snapshot::run(
                target.as_deref(),
                label.as_deref(),
                list,
                restore.as_deref(),
                diff.as_deref(),
                clean,
            )?;
        }
        Command::Qr {
            text,
            invert,
            compact,
        } => {
            let t = text.ok_or_else(|| anyhow::anyhow!("需要内容,如: rxt qr \"https://...\""))?;
            qr::run(&t, invert, compact)?;
        }
        Command::Clip {
            action,
            content,
            file,
        } => {
            clip::run(&action, content.as_deref(), file.as_deref())?;
        }
        Command::Repeat {
            cmd,
            file,
            port,
            ping,
            timeout,
            interval,
            tries,
        } => {
            repeat::run(
                cmd.as_deref(),
                file.as_deref(),
                port.as_deref(),
                ping.as_deref(),
                timeout,
                interval,
                tries,
            )?;
        }
        Command::Notify {
            message,
            title,
            level,
        } => {
            notify::run(&message, title.as_deref(), &level)?;
        }
        Command::Dup {
            dir,
            min_size,
            ext,
            delete,
            json,
        } => {
            dup::run(&dir, &min_size, ext.as_deref(), delete, json)?;
        }
        Command::Trash {
            paths,
            list,
            restore,
            to,
            clean,
            purge,
            json,
        } => {
            trash::run(
                &paths,
                list,
                restore.as_deref(),
                to.as_deref(),
                clean,
                purge,
                json,
            )?;
        }
        Command::Recipe {
            action,
            name,
            content,
            args,
            dry_run,
            json,
        } => {
            recipe::run(
                &action,
                name.as_deref(),
                content.as_deref(),
                &args,
                dry_run,
                json,
            )?;
        }
        Command::Bench {
            cmds,
            runs,
            warmup,
            json,
        } => {
            bench::run(&cmds, runs, warmup, json)?;
        }
        Command::WatchRun {
            cmd,
            paths,
            ext,
            debounce,
            run_on_start,
        } => {
            watch_run::run(&cmd, &paths, &ext, debounce, run_on_start)?;
        }
        Command::Evolve {
            reference,
            candidate,
            inputs,
            mode,
            timeout,
            first_fail,
            json,
        } => {
            evolve::run(
                &reference, &candidate, &inputs, &mode, timeout, first_fail, json,
            )?;
        }
        Command::Mcp { sse, full, slim } => {
            // full 优先；否则默认 slim
            let mode_slim = if full { false } else { slim };
            mcp::run(sse, mode_slim)?;
        }
    }
    Ok(())
}
