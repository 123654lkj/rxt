//! 自描述协议 — `rxt --describe` 输出所有子命令 schema
//!
//! 输出 JSON 描述 rxt 的全部 subcommand,args,options
//! 便于 AI agent 动态发现 CLI 能力

use serde_json::json;

pub fn run() -> anyhow::Result<()> {
    let schema = build_schema();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}

fn build_schema() -> serde_json::Value {
    let mut commands = vec![];

    commands.push(json!({
        "name": "read",
        "about": "读文件,自动检测编码/换行符/BOM",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "encoding", "short": null, "long": "encoding", "type": "Option<String>"},
            {"name": "number", "short": "n", "long": "number", "type": "bool"},
            {"name": "head", "short": null, "long": "head", "type": "Option<usize>"},
            {"name": "tail", "short": null, "long": "tail", "type": "Option<usize>"},
            {"name": "lines", "short": null, "long": "lines", "type": "Option<String>"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "write",
        "about": "写文件(覆盖),UTF-8/LF",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "content", "type": "Vec<String>"},
            {"name": "append", "short": null, "long": "append", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "edit",
        "about": "结构化文件编辑 — 格式保持",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "after", "type": "Option<String>"},
            {"name": "before", "type": "Option<String>"},
            {"name": "delete", "short": null, "long": "delete", "type": "bool"},
            {"name": "replace", "short": null, "long": "replace", "type": "Option<String>"},
            {"name": "content", "type": "Vec<String>"},
            {"name": "preview", "short": null, "long": "preview", "type": "bool"},
            {"name": "script", "short": null, "long": "script", "type": "Option<String>"},
        ]
    }));

    commands.push(json!({
        "name": "find",
        "about": "查找文件",
        "args": [
            {"name": "pattern", "type": "Option<String>"},
            {"name": "dir", "type": "PathBuf", "default": "."},
            {"name": "type", "short": "t", "long": "type", "type": "Option<String>", "values": ["f", "d", "l"]},
            {"name": "max_depth", "short": "d", "long": "max-depth", "type": "Option<usize>"},
            {"name": "max_results", "short": null, "long": "max-results", "type": "Option<usize>"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
            {"name": "no_ignore", "short": null, "long": "no-ignore", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "grep",
        "about": "正则搜索文件内容",
        "args": [
            {"name": "pattern", "type": "String", "required": true},
            {"name": "path", "type": "PathBuf", "default": "."},
            {"name": "context", "short": "C", "long": "context", "type": "usize", "default": 2},
            {"name": "file_type", "short": null, "long": "type", "type": "Option<String>"},
            {"name": "count", "short": null, "long": "count", "type": "bool"},
            {"name": "invert", "short": "v", "long": "invert-match", "type": "bool"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
            {"name": "jsonl", "short": null, "long": "jsonl", "type": "bool"},
            {"name": "regex", "short": null, "long": "regex", "type": "bool", "default": true},
            {"name": "max_results", "short": null, "long": "max-results", "type": "Option<usize>"},
            {"name": "head", "short": null, "long": "head", "type": "Option<usize>"},
            {"name": "offset", "short": null, "long": "offset", "type": "Option<usize>"},
            {"name": "no_ignore", "short": null, "long": "no-ignore", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "diff",
        "about": "差异对比",
        "args": [
            {"name": "first", "type": "PathBuf", "required": true},
            {"name": "second", "type": "Option<PathBuf>"},
            {"name": "context", "short": "C", "long": "context", "type": "usize", "default": 3},
            {"name": "stat", "short": "s", "long": "stat", "type": "bool"},
            {"name": "ai", "short": null, "long": "ai", "type": "bool"},
            {"name": "side_by_side", "short": null, "long": "side-by-side", "type": "bool"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "jq",
        "about": "JSON 查询/格式化 — 迷你 jq",
        "args": [
            {"name": "query", "type": "Option<String>"},
            {"name": "file", "short": "f", "long": "file", "type": "Option<PathBuf>"},
            {"name": "fmt", "short": null, "long": "fmt", "type": "bool"},
            {"name": "compact", "short": "c", "long": "compact", "type": "bool"},
            {"name": "raw", "short": "r", "long": "raw", "type": "bool"},
            {"name": "slurp", "short": "s", "long": "slurp", "type": "bool"},
        ],
        "syntax_hints": {
            "examples": [
                ".foo[0].name",
                ".users[] | select(.active) | .name",
                "[.items[].x] | @csv",
                ".users | sort_by(.age) | map(.name)",
            ],
            "builtins": ["length", "keys", "values", "type", "select", "map", "has", "contains",
                         "unique", "sort", "sort_by", "reverse", "first", "last", "nth",
                         "min", "max", "min_by", "max_by", "group_by", "flatten",
                         "ascii_downcase", "ascii_upcase", "tostring", "tonumber",
                         "to_entries", "from_entries", "with_entries",
                         "add", "empty", "not", "recurse", "walk"],
            "formats": ["@csv", "@json", "@text", "@tsv", "@uri", "@base64", "@html"],
            "operators": ["==", "!=", "<", "<=", ">", ">=", "and", "or", "not", "+", "-", "*", "/", "%"],
        }
    }));

    commands.push(json!({
        "name": "unzip",
        "about": "归档解压 — zip / tar / tar.gz / tgz / 3mf",
        "args": [
            {"name": "archive", "type": "PathBuf", "required": true},
            {"name": "target", "short": "o", "long": "to", "type": "Option<PathBuf>"},
            {"name": "list_only", "short": "l", "long": "list", "type": "bool"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
            {"name": "strip", "short": null, "long": "strip", "type": "Option<usize>"},
        ]
    }));

    commands.push(json!({
        "name": "ls",
        "about": "目录列表 — 类似 ls",
        "args": [
            {"name": "dir", "type": "PathBuf", "default": "."},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
            {"name": "all", "short": "a", "long": "all", "type": "bool"},
            {"name": "sort", "short": "s", "long": "sort", "type": "Option<String>", "values": ["name", "size", "mtime"]},
            {"name": "depth", "short": "d", "long": "depth", "type": "Option<usize>"},
            {"name": "max", "short": null, "long": "max", "type": "Option<usize>"},
        ]
    }));

    commands.push(json!({
        "name": "http",
        "about": "HTTP 客户端",
        "args": [
            {"name": "method", "type": "String", "default": "GET", "values": ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS", "PATCH"]},
            {"name": "url", "type": "String", "required": true},
            {"name": "headers", "short": "H", "long": "header", "type": "Vec<String>"},
            {"name": "data", "short": "d", "long": "data", "type": "Option<String>"},
            {"name": "json_body", "short": "j", "long": "json", "type": "bool"},
            {"name": "auth", "short": null, "long": "auth", "type": "Option<String>"},
            {"name": "show_headers", "short": "i", "long": "headers", "type": "bool"},
            {"name": "body_only", "short": "b", "long": "body-only", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "tail",
        "about": "tail -f 替代 — 监控文件追加新行",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "filter", "short": "f", "long": "filter", "type": "Option<String>"},
            {"name": "interval", "short": "n", "long": "interval", "type": "u64", "default": 500},
            {"name": "lines", "short": "l", "long": "lines", "type": "usize", "default": 10},
            {"name": "once", "short": null, "long": "once", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "watch",
        "about": "文件变化触发命令",
        "args": [
            {"name": "patterns", "type": "Vec<String>", "required": true},
            {"name": "cmd", "type": "String", "required": true},
            {"name": "path", "short": "p", "long": "path", "type": "Option<PathBuf>"},
            {"name": "debounce", "short": "d", "long": "debounce", "type": "u64", "default": 500},
        ]
    }));

    commands.push(json!({
        "name": "stat",
        "about": "文件元数据",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "cat",
        "about": "连接文件输出(stdin/cat 替代)",
        "args": [
            {"name": "files", "type": "Vec<PathBuf>"},
            {"name": "number", "short": "n", "long": "number", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "jsonl",
        "about": "JSONL 处理",
        "args": [
            {"name": "file", "type": "Option<PathBuf>"},
        ]
    }));

    commands.push(json!({
        "name": "sed",
        "about": "流式替换",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "pattern", "type": "String", "required": true},
            {"name": "replacement", "type": "String", "required": true},
            {"name": "preview", "short": null, "long": "preview", "type": "bool"},
            {"name": "line", "short": null, "long": "line", "type": "Option<usize>"},
            {"name": "regex", "short": null, "long": "regex", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "patch",
        "about": "应用 unified diff 补丁(git apply)",
        "args": [
            {"name": "paths", "type": "Vec<String>"},
            {"name": "reverse", "short": "R", "long": "reverse", "type": "bool"},
            {"name": "check", "short": null, "long": "check", "type": "bool"},
            {"name": "output", "short": "o", "long": "output", "type": "Option<String>"},
        ]
    }));

    commands.push(json!({
        "name": "hash",
        "about": "计算哈希",
        "args": [
            {"name": "path", "type": "Option<PathBuf>"},
            {"name": "algo", "short": null, "long": "algo", "type": "String", "default": "sha256", "values": ["md5", "sha1", "sha256", "sha512"]},
            {"name": "text", "short": null, "long": "text", "type": "Option<String>"},
        ]
    }));

    commands.push(json!({
        "name": "enc",
        "about": "编码转换",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "from", "short": null, "long": "from", "type": "Option<String>"},
            {"name": "to", "short": null, "long": "to", "type": "String", "default": "utf8"},
        ]
    }));

    commands.push(json!({
        "name": "uuidgen",
        "about": "生成 UUID",
        "args": [
            {"name": "count", "short": "n", "long": "count", "type": "usize", "default": 1},
        ]
    }));

    commands.push(json!({
        "name": "tree",
        "about": "目录树",
        "args": [
            {"name": "dir", "type": "PathBuf", "default": "."},
            {"name": "depth", "short": "d", "long": "depth", "type": "Option<usize>"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "count",
        "about": "统计行/词/字节",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "struct",
        "about": "Rust 结构体/函数提取",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
            {"name": "functions", "short": "f", "long": "functions", "type": "bool"},
            {"name": "types", "short": "t", "long": "types", "type": "bool"},
            {"name": "deep", "short": "d", "long": "deep", "type": "bool"},
            {"name": "extract", "short": null, "long": "extract", "type": "Option<String>"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "sort",
        "about": "排序",
        "args": [
            {"name": "file", "type": "Option<PathBuf>"},
            {"name": "reverse", "short": "r", "long": "reverse", "type": "bool"},
            {"name": "unique", "short": "u", "long": "unique", "type": "bool"},
            {"name": "numeric", "short": "n", "long": "numeric", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "uniq",
        "about": "去重(相邻)",
        "args": [
            {"name": "file", "type": "Option<PathBuf>"},
            {"name": "count", "short": "c", "long": "count", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "cut",
        "about": "按列切分",
        "args": [
            {"name": "file", "type": "Option<PathBuf>"},
            {"name": "delimiter", "short": "d", "long": "delimiter", "type": "String", "default": "\\t"},
            {"name": "fields", "short": "f", "long": "fields", "type": "String"},
        ]
    }));

    commands.push(json!({
        "name": "size",
        "about": "文件/目录大小",
        "args": [
            {"name": "path", "type": "PathBuf", "required": true},
        ]
    }));

    commands.push(json!({
        "name": "ctx",
        "about": "上下文提取",
        "args": [
            {"name": "file", "type": "String", "required": true},
            {"name": "line", "short": "l", "long": "line", "type": "usize"},
            {"name": "radius", "short": "r", "long": "radius", "type": "usize", "default": 5},
        ]
    }));

    commands.push(json!({
        "name": "mem",
        "about": "内存状态",
        "args": [
            {"name": "key", "type": "Option<String>"},
            {"name": "value", "type": "Option<String>"},
        ]
    }));

    commands.push(json!({
        "name": "build",
        "about": "Rust 项目构建",
        "args": [
            {"name": "release", "short": "r", "long": "release", "type": "bool"},
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "check",
        "about": "cargo check",
        "args": [
            {"name": "json", "short": null, "long": "json", "type": "bool"},
        ]
    }));

    commands.push(json!({
        "name": "clean",
        "about": "清理 Rust target",
        "args": []
    }));

    commands.push(json!({
        "name": "timecmd",
        "about": "测量命令执行时间",
        "args": [
            {"name": "cmd", "type": "Vec<String>", "required": true},
        ]
    }));

    commands.push(json!({
        "name": "exec",
        "about": "执行系统命令",
        "args": [
            {"name": "cmd", "type": "Vec<String>", "required": true},
        ]
    }));

    commands.push(json!({
        "name": "py",
        "about": "运行 Python 代码",
        "args": [
            {"name": "code", "type": "Option<String>"},
            {"name": "file", "short": "f", "long": "file", "type": "Option<String>"},
        ]
    }));

    json!({
        "name": "rxt",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Rust Codex Tools - AI's Cross-Platform IDE",
        "global_flags": [
            {"name": "host", "long": "host", "type": "Option<String>", "help": "远程主机(~/.rxt/hosts.toml)"},
            {"name": "group", "long": "group", "type": "Option<String>", "help": "远程主机组(批量执行)"},
        ],
        "commands": commands,
        "describe_flags": [
            {"name": "describe", "long": "describe", "type": "bool", "help": "输出所有子命令 schema(JSON)"},
        ],
    })
}