# rxt - Rust Codex Tools

> AI 的跨平台 IDE — 一个二进制，管所有机器。

## 简介

rxt 是一个用 Rust 编写的跨平台命令行工具集，专为 AI agent 和开发者设计。核心特性是**透明远程**：用同一套命令管理本地和远程 Linux/Windows 机器，无需 SSH 进去再敲命令。

```bash
# 本地管理
rxt ls /home/huhu
rxt ps --top 5

# 远程管理 — 一条命令搞定
rxt --host xian ls "C:\Users\xiantuer\Desktop"
rxt --host xian ps --top 5
rxt --host tuanzi ls /home/tuanzi
```

## 特性

### 🌐 透明远程

`--host` 参数让所有命令无缝切换到远程机器，体验和本地完全一致：

```bash
rxt ls <dir>                    # 本地目录列表
rxt --host xian ls <dir>        # 远程目录列表（Windows/Linux 自动适配）
rxt --host tuanzi ps            # 远程进程列表
rxt --host xian tree "C:\temp"  # 远程目录树
```

### 🖥️ 跨平台 Windows 支持

远程 Windows 主机使用 PowerShell 命令，自动适配：

| 命令 | Windows 实现 | Linux 实现 |
|------|-------------|-----------|
| `ls` | Get-ChildItem | ls -la |
| `ps` | Get-Process | ps aux |
| `tree` | tree /F /A | tree |
| `cat` | 直接读取 | read_file |
| `net` | netstat -ano | ss / ip route |
| `service` | sc.exe | systemctl |
| `reg` | reg query/add/delete | 仅远程 Windows |
| `info` | 远端 rxt 信息 | 远端 rxt 信息 |

### 🔧 60+ 命令

**文件操作：**
- `read` / `write` / `cat` — 读写文件，自动检测编码/换行/BOM
- `ls` / `tree` / `find` — 目录浏览和搜索
- `grep` / `sed` / `replace` / `edit` — 内容搜索和编辑
- `stat` / `hash` / `diff` — 文件元信息和对比

**系统管理：**
- `ps` — 进程管理（列表/查杀/排序）
- `service` — 服务管理（Windows sc.exe / Linux systemctl）
- `reg` — Windows 注册表读写
- `net` — TCP 连接/路由/DNS/端口检查
- `sysinfo` — 系统信息（OS/CPU/内存/磁盘/网络）

**开发工具：**
- `exec` — 执行代码（Python/PowerShell/Shell）
- `build` / `check` / `clean` — Rust 构建工具链
- `git` — AI 友好的 git 包装
- `ctx` — AI 上下文生成器

**实用工具：**
- `jq` — JSON 查询/格式化
- `clip` — 跨平台剪贴板
- `trash` — 安全删除（回收站）
- `serve` — HTTP 文件服务器
- `qr` — 终端二维码
- `enc` / `dec` — 编码/解码

## 安装

### 从源码编译

```bash
git clone https://github.com/123654lkj/rxt.git
cd rxt
cargo build --release
cp target/release/rxt /usr/local/bin/
```

### 远程主机配置

编辑 `~/.rxt/hosts.toml`：

```toml
# Linux 主机
[hosts.tuanzi]
host = "192.168.31.244"
user = "tuanzi"
password = "your_password"
port = 22

# Windows 主机
[hosts.xian]
os = "windows"
host = "192.168.31.169"
user = "xiantuer"
password = "your_password"
port = 22

# 主机组
[group.all]
members = ["tuanzi", "xian"]
```

## 使用示例

### 三跳管理

```bash
# 管理本地 Linux
rxt ls /home/huhu --depth 2

# 管理远程 Linux
rxt --host tuanzi ls /home/tuanzi

# 管理远程 Windows
rxt --host xian ls "C:\Users\xiantuer\Desktop"
rxt --host xian ps --top 10
rxt --host xian net --conn ESTABLISHED
rxt --host xian service --running
rxt --host xian reg --list "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion"
```

### 远程执行

```bash
# 在远程 Windows 上执行 PowerShell
rxt exec --host xian "Get-Service | Where-Object Status -eq Running"

# 在远程 Linux 上执行命令
rxt exec --host tuanzi "docker ps"
```

### MCP Server 模式

rxt 可以作为 MCP server 运行，暴露所有命令给 AI agent：

```bash
rxt mcp          # stdio JSON-RPC 模式
rxt mcp --sse    # SSE 模式
```

## 架构

```
rxt
├── 本地模式    → 直接调用 Rust 实现
├── 远程模式    → SSH 连接远端，执行等效命令
│   ├── Linux   → bash/sh 命令
│   └── Windows → PowerShell 命令
└── MCP 模式    → JSON-RPC server，供 AI agent 调用
```

### 远程 OS 检测

连接远程主机时，rxt 自动检测操作系统：

1. 执行 `uname -s`，包含 "linux" → Linux 模式
2. 执行 `echo WIN`，返回 "WIN" → Windows 模式
3. 也可在 `hosts.toml` 中手动指定 `os = "windows"`

### Windows 编码处理

Windows SSH 默认输出 GBK 编码，rxt 自动处理：
- 先尝试 UTF-8 解码
- 如果包含替换字符（`\u{fffd}`），回退到 GBK 解码（使用 `encoding_rs` crate）
- 对于需要 UTF-8 输出的场景，使用 PowerShell 语法而非 `.exe` 程序

## 开发

```bash
# 编译
cargo build --release

# 测试
cargo test

# 运行
./target/release/rxt --help
```

## 版本历史

### v0.4.1 (2026-07-03)

- **新增远程 Windows 支持**：ls/ps/tree/cat/net/service/reg/info 全部支持远程模式
- **Windows 命令适配**：Get-ChildItem, Get-Process, tree /F, netstat, sc.exe, reg
- **Linux 命令适配**：ls -la, ps aux, tree, ss/ip, systemctl
- **remote.rs 增强**：添加 `is_windows()` / `is_linux()` 公开方法
- **GBK 编码修复**：远程 Windows 输出自动 GBK→UTF-8 转换

### v0.4.0 (2026-06-28)

- 自举：rxt 用自己的 git 命令推送自己的代码
- 新增 trash/recipe/clip/repeat/notify/dup/bench 等工具
- MCP server 模式（stdio + SSE）
- Windows rxt.exe 部署

## 许可

MIT
