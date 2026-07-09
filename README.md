# rxt — Rust Codex Tools

> **AI 的跨平台 IDE** — 一个二进制，管所有机器。专为 AI agent 和开发者设计。

`rxt` 是一个用 Rust 编写的跨平台命令行工具集。核心理念是**透明远程**：用同一套命令管理本地和远程 Linux/Windows 机器，无需 SSH 进去再敲命令。同时还内置了代码结构分析、目录骨架、调用链追踪等 AI 友好的能力。

```bash
# 本地管理
rxt ls /home/huhu
rxt ps --top 5

# 远程管理 — 一条命令搞定, Windows/Linux 自动适配
rxt --host xian ls "C:\Users\xiantuer\Desktop"     # 远程 Windows
rxt --host tuanzi ps --top 5                        # 远程 Linux

# 走跳板机访问不可直连的目标
rxt --host xian tree "C:\temp"                      # 经 jump_host 中转
```

---

## ✨ 核心特性

### 🌐 透明远程 (SSH)

`--host` / `--group` 参数让所有命令无缝切换到远程机器，体验和本地完全一致。底层基于纯 Rust 的 [russh](https://crates.io/crates/russh)（不依赖 libssh2/OpenSSL），单文件部署。

```bash
rxt ls <dir>                         # 本地目录列表
rxt --host xian ls <dir>             # 远程目录列表（Windows/Linux 自动适配）
rxt --host tuanzi ps                 # 远程进程列表
rxt --group all version              # 批量查所有机器 rxt 版本 + 一致性检测
```

**跳板机访问（v0.7.3+）**：当目标机无法直连（防火墙隔离 / 仅内网可达 / 外网访问内网），可经跳板机 SSH 隧道中转：

```toml
# ~/.rxt/hosts.toml
[hosts.target]
host = "10.0.0.10"        # 目标机真实地址 (可能不可直连)
jump_host = "bastion"     # 先 SSH 到 bastion, 再 direct-tcpip 隧道转发到 target
```

```bash
rxt --host target exec "hostname"    # 自动走 bastion → target 两跳
```

实现细节见下方 [架构 - 跳板机](#-跳板机-jump_host) 章节。

### 🖥️ 跨平台 Windows 支持

远程 Windows 主机使用 PowerShell 命令，自动适配。Windows SSH 默认输出 GBK，rxt 会自动 UTF-8 优先、GBK 回退解码。

| 命令 | Windows 实现 | Linux 实现 |
|------|-------------|-----------|
| `ls` | Get-ChildItem | ls -la |
| `ps` | Get-Process | ps aux |
| `tree` | tree /F /A | tree |
| `net` | netstat -ano | ss / ip route |
| `service` | sc.exe | systemctl |
| `reg` | reg query/add/delete | 仅远程 Windows |
| `info` | 远端 rxt 信息 | 远端 rxt 信息 |

### 🧠 AI 友好的代码智能

不只是文件/系统工具，rxt 还内置了专为 AI agent 省 token、提效的能力：

- **`digest`** — 目录符号骨架，函数体折叠，省 70% token
- **`refs`** — 双向调用链（`--callers` 谁调用了它 / `--callees` 它调用了谁）
- **`struct`** — 代码结构分析（函数/类型/签名/行号）
- **`ctx`** — AI 上下文生成器（一次输出签名 + imports + 内容）
- **`map`** — 项目结构简报 + git HEAD 缓存引擎

### 🔧 60+ 命令，单二进制

文件操作、系统管理、开发工具、实用工具全覆盖，详见下方 [命令手册](#-命令手册)。

---

## 📦 安装

### 方式一：从源码编译

```bash
git clone https://github.com/123654lkj/rxt.git
cd rxt
cargo build --release
# Linux/macOS
cp target/release/rxt /usr/local/bin/
# 或用户级安装
cp target/release/rxt ~/.local/bin/
```

### 方式二：交叉编译 + 部署（自托管）

rxt 支持从一台 Linux 机器交叉编译出 Windows 版本并自动部署：

```bash
# 1. 安装 Windows 交叉编译工具链（一次性）
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64      # 提供 x86_64-w64-mingw32-gcc linker

# 2. 交叉编译
cargo build --release --target x86_64-pc-windows-gnu
# 产物: target/x86_64-pc-windows-gnu/release/rxt.exe (PE32+)

# 3. 用 rxt 自己部署到远程（自动检测目标 OS + 二进制格式校验）
rxt deploy target/release/rxt -t tuanzi                              # Linux → Linux (ELF)
rxt deploy target/x86_64-pc-windows-gnu/release/rxt.exe -t xian      # Linux → Windows (PE)
rxt deploy target/release/rxt --all                                  # 全部机器
```

> **Windows 部署注意**：若目标机有杀毒软件（如 Windows Defender）实时扫描，直接写入 `C:\rxt\` 的 `.exe` 可能被破坏。推荐部署到用户目录 `%USERPROFILE%\rxt.exe` 并把 `%USERPROFILE%` 加入 PATH。

### 远程主机配置

编辑 `~/.rxt/hosts.toml`：

```toml
# Linux 主机
[hosts.tuanzi]
host = "192.168.31.244"
user = "tuanzi"
password = "your_password"     # 或用 password_env 引用环境变量
port = 22

# Windows 主机
[hosts.xian]
os = "windows"                 # 可选, 避免每次探测
host = "192.168.31.169"
user = "xiantuer"
password = "your_password"
port = 22
jump_host = "huhu"             # 可选, 经跳板机访问 (v0.7.3+)

# 密钥认证
[hosts.osaka]
host = "64.176.43.4"
user = "root"
key = "~/.ssh/id_ed25519"
port = 22

# 主机组 (批量操作)
[group.all]
members = ["tuanzi", "xian", "osaka"]
```

**认证方式**（优先级）：密钥 (`key`) > 密码 (`password`) > 环境变量密码 (`password_env`)。

---

## 🚀 使用示例

### 透明远程管理

```bash
# 管理本地
rxt ls /home/huhu --depth 2
rxt ps --top 10 --sort cpu

# 管理远程 Linux
rxt --host tuanzi ls /home/tuanzi
rxt --host tuanzi exec "docker ps"
rxt --host tuanzi service --running

# 管理远程 Windows
rxt --host xian ls "C:\Users\xiantuer\Desktop"
rxt --host xian ps --top 10
rxt --host xian net --conn ESTABLISHED
rxt --host xian reg --list "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion"

# 远程执行 PowerShell
rxt exec --host xian "Get-Service | Where-Object Status -eq Running"

# 批量操作所有机器
rxt --group all version             # 版本一致性检测
rxt --group all exec "uptime"       # 全员执行
```

### AI 代码智能

```bash
rxt digest ./src/                   # 目录骨架 (省 token)
rxt refs connect_async --callers    # 谁调用了 connect_async
rxt refs connect_async --callees    # connect_async 调用了谁
rxt struct src/remote.rs --functions
rxt ctx src/main.rs --max-lines 200 # 生成 AI 上下文
rxt map . --depth 3                 # 项目结构简报
```

### 内联代码执行

```bash
rxt exec "docker ps"
rxt exec "SELECT count(*) FROM torrents" --lang sql --db postgres
rxt exec --host tuanzi "docker logs nginx --tail 50"
rxt py -c "print(sum(range(100)))"
```

### MCP Server 模式

rxt 可作为 MCP (Model Context Protocol) server 运行，把所有命令暴露给 AI agent：

```bash
rxt mcp          # stdio JSON-RPC 模式 (被 ZCode/Hermes/Codex 等调用)
rxt mcp --sse    # SSE 模式
```

---

## 📚 命令手册

### 文件操作

| 命令 | 说明 |
|------|------|
| `read` / `cat` | 读文件，自动检测编码/换行/BOM，统一 UTF-8+LF |
| `write` | 写文件，自动保持目标文件格式（支持 `--from` 远程大文件） |
| `edit` | 结构化编辑（`--after`/`--before`/`--replace`/`--line-range`） |
| `replace` | 块替换 |
| `sed` | 安全替换，格式保持（支持正则） |
| `ls` / `tree` / `find` | 目录浏览和搜索 |
| `grep` | 跨文件增强搜索（rayon 并行，`-i` 是反转匹配，注意！） |
| `stat` / `hash` / `diff` | 文件元信息、哈希、对比 |
| `normalize` | 文件格式统一（换行/BOM 标准化） |
| `tail` | tail -f 替代，监控文件追加 |
| `patch` | 补丁工具（apply/reverse/check） |
| `unzip` | 解压 zip/tar/tar.gz/3mf（支持 `--strip` 去前缀） |
| `trash` | 安全删除（回收站 + 恢复） |
| `snapshot` | 文件/目录时光机（快照 + 回滚） |

### 系统管理

| 命令 | 说明 |
|------|------|
| `ps` | 进程列表/查杀（`--name`/`--kill`/`--sort`/`--tree`） |
| `service` | 服务管理（Windows sc.exe / Linux systemctl） |
| `reg` | Windows 注册表读写（仅远程 Windows） |
| `net` | TCP 连接/路由/DNS/端口检查 |
| `sysinfo` | 系统信息（os/cpu/mem/disk/net） |
| `info` | rxt 自检（版本/配置/hosts 状态） |

### 开发工具

| 命令 | 说明 |
|------|------|
| `build` / `check` / `clean` / `size` | Rust 构建工具链 |
| `exec` | 多语言代码执行（支持 docker 容器 + SQL） |
| `py` | 执行内联 Python |
| `git` | AI 友好的 git 包装（status/diff/log/branch/add/commit/undo） |
| `jq` | JSON 查询/格式化 |
| `http` | HTTP 客户端 |
| `digest` | 目录符号骨架（省 token） |
| `refs` | 双向调用链（`--callers`/`--callees`） |
| `struct` | 代码结构分析 |
| `dep` | 依赖分析 |
| `ctx` | AI 上下文生成器 |
| `map` | 项目结构简报 + git HEAD 缓存 |

### 文本处理

| 命令 | 说明 |
|------|------|
| `sort` / `uniq` / `cut` / `count` | 行排序/去重/列提取/统计 |
| `enc` / `dec` | 编码/解码（base64/hex/url 等） |
| `hash` | 哈希计算（sha256/md5 等） |
| `uuid` | UUID 生成器 |

### 部署 & 运维

| 命令 | 说明 |
|------|------|
| `deploy` | 部署二进制到远程（自动 OS + 格式校验） |
| `version` | 批量查询版本 + 一致性检测 |
| `sync` | 跨机目录同步（rsync 替代） |
| `upgrade` | 自我更新（git pull + 编译 + 热替换） |

### 实用工具

| 命令 | 说明 |
|------|------|
| `serve` | HTTP 文件服务器（手机扫码访问） |
| `qr` | 终端二维码 |
| `clip` | 跨平台剪贴板读写 |
| `repeat` | 轮询重试（等端口/文件/命令） |
| `notify` | 桌面通知（长任务完成提醒） |
| `dup` | 按内容哈希找重复文件 |
| `watch` | 文件监听 + 触发命令 |
| `time` | 命令计时 |
| `jsonl` | 解析 Codex 会话 JSONL |

---

## 🏗️ 架构

```
rxt
├── 本地模式    → 直接调用 Rust 实现
├── 远程模式    → SSH 连接远端, 执行等效命令
│   ├── 直连      → russh client::connect
│   ├── 跳板机    → SSH-over-SSH (jump_host)  [v0.7.3+]
│   ├── Linux     → bash/sh 命令
│   └── Windows   → PowerShell 命令
└── MCP 模式    → JSON-RPC server, 供 AI agent 调用
```

### 远程 OS 检测

连接远程主机时，rxt 自动检测操作系统：

1. 执行 `uname -s`，包含 "linux" → Linux 模式
2. 执行 `echo WIN`，返回 "WIN" → Windows 模式
3. 也可在 `hosts.toml` 中手动指定 `os = "windows"` 跳过探测

### 跳板机 (jump_host)

**v0.7.3 引入**。当目标机无法直连时，经跳板机 SSH 隧道中转。底层基于 russh 0.61 的 SSH-over-SSH：

```
发起方 ──SSH①──→ 跳板机 (jump_host) ──direct-tcpip 隧道──→ 目标机
                                          │
                            在隧道上建第二层 SSH② 连接
```

实现链路（`src/remote.rs`）：

1. SSH 连接并认证跳板机（复用目标机的认证逻辑：密钥/密码）
2. `jump_handle.channel_open_direct_tcpip(target_host, target_port, ...)` 开启转发隧道
3. `channel.into_stream()` 把 Channel 转成 `ChannelStream`（实现 `AsyncRead + AsyncWrite`）
4. `client::connect_stream(config, stream, handler)` 在隧道上建立第二层 SSH
5. 认证目标机

关键 russh 0.61 API（源码核验）：

- `client::connect_stream(config, stream, handler)` — 接受任意 `AsyncRead + AsyncWrite + Unpin + Send` 的流
- `Handle::channel_open_direct_tcpip(host, port, originator_addr, originator_port)` → `Channel<Msg>`
- `Channel::into_stream()` → `ChannelStream`（已实现 `AsyncRead` + `AsyncWrite`）

无 `jump_host` 配置时走原直连逻辑，行为完全不变。

### 🔄 远端 rxt 感知（v0.7.5+）

当远端也安装了 rxt 时，`ls` / `ps` / `tree` / `grep` / `stat` / `sysinfo` / `info` 等命令会**自动优先调用远端 rxt 的原生实现**，而非 SSH 跑 shell 命令再解析文本：

- **输出统一**：跨 Linux/Windows 格式一致（`ls` 都是表格、`ps` 都是 `PID CPU% MEM` 格式）
- **无编码问题**：Windows 远程不再有 GBK 乱码（原生 Rust 直接输出 UTF-8）
- **无依赖**：`tree` / `grep` 不依赖远端系统工具的版本和行为差异
- **自动降级**：远端没装 rxt 时静默回退到 shell 模式，用户无感

探测采用三态缓存（`None`→首次尝试→`Some(true/false)`），首次成功后不再重复探测；"半路装上 rxt"只需重连一次即自动刷新。

### Windows 编码处理

Windows SSH 默认输出 GBK 编码，rxt 自动处理：

1. 先尝试 UTF-8 解码
2. 如果包含替换字符（`\u{fffd}`），回退到 GBK 解码（使用 `encoding_rs` crate）
3. 对于需要 UTF-8 输出的场景，使用 PowerShell 语法而非 `.exe` 程序

---

## 🛠️ 开发

```bash
# 编译 (默认启用 remote/net/xz features)
cargo build --release

# 交叉编译 Windows
cargo build --release --target x86_64-pc-windows-gnu

# 无 remote feature 编译 (纯本地, 无任何 C 依赖)
cargo build --release --no-default-features

# 运行
./target/release/rxt --help
```

### Feature flags

| Feature | 默认 | 说明 |
|---------|------|------|
| `remote` | ✅ | SSH/SFTP 远程能力（依赖 russh + tokio） |
| `net` | ✅ | `http` 命令（依赖 ureq） |
| `xz` | ✅ | `.tar.xz` 解压（依赖 xz2 → liblzma，需 gcc） |
| `shellexpand` | ✅* | `~` 路径展开（随 remote 启用） |

关闭默认 feature 可在无 C 工具链的环境编译纯本地版。

---

## 📖 版本历史

### v0.7.5 (2026-07-10)

- **🆕 远端 rxt 感知（exec_rxt 自动降级）**：连接远程主机时，自动探测远端是否安装了 rxt。有则优先调远端 `rxt` 原生实现（输出统一、无 GBK/编码问题、行为一致），无则静默降级到原有 shell 模式，用户无感
  - `remote.rs`: 新增 `probe_rxt_path()`（跨平台探测：Linux `command -v` / Windows `Get-Command`+多路径回退）+ `try_exec_rxt()`（三态缓存 `has_rxt: Option<bool>`，首次探测后缓存，"半路装上"重连即刷新）
  - `main.rs`: `sysinfo` / `info` / `ls` / `ps` / `tree` / `grep` / `stat` 7 个命令改造，远端有 rxt 时走原生实现
  - `version.rs`: 复用 `probe_rxt_path()`，消除重复的 PowerShell 探测逻辑
  - **收益**：Windows 远程不再有 GBK 乱码、输出格式跨平台统一、复杂命令（grep/tree）不依赖远端系统工具版本

### v0.7.3 (2026-07-09)

- **🆕 跳板机 (jump_host) 支持**：目标机配 `jump_host` 字段，经跳板机 SSH-over-SSH 隧道中转访问不可直连的目标
  - `hosts.rs`: `HostConfig` 新增 `jump_host: Option<String>`
  - `remote.rs`: 重构 `connect_async`，抽出 `authenticate()` 复用，加 jump_host 两跳分支
  - 基于 russh 0.61 `channel_open_direct_tcpip` + `into_stream` + `connect_stream`
- **修复 `version` 远程 Windows 路径硬编码**：不再只查 `C:\rxt\rxt.exe`，改为 PATH 优先 + 回退 `%USERPROFILE%\rxt.exe` 和 `C:\rxt\rxt.exe`

### v0.7.0 (2026-07-06)

- **`refs` 双向调用链**：`--callers`（谁调用了它）/ `--callees`（它调用了谁），对标 codeseek
- **`digest` 目录模式**：目录符号骨架，函数体折叠，省 70% token
- **`mem` 端点修复**：MCP 调用不再断连（stdout Mutex 包裹 + tools/call 子线程化）
- **`seek` 语义代码搜索**：调用星枢向量检索

### v0.6.0 ~ v0.6.1 (2026-07)

- 架构重构：远程逻辑下沉到各模块，统一 dispatch（`--group`/`--host` → `execute_command`）
- `Storage` trait/enum：本地/远程存储抽象，消除重复代码
- `deploy` 安全增强 + `dup` 重构 + `edit.rs` 远程分支去重
- `grep` 并行锁重构 + 去重

### v0.5.0 ~ v0.5.1 (2026-06)

- 统一命令分发（group + host → `execute_command`）
- `grep` 并行化（rayon）+ 锁重构
- 新增 `deploy` / `version` / `sync` + 交叉平台部署格式检查

### v0.4.x (2026-06 ~ 07)

- **v0.4.2**：`ssh2` 改用纯 Rust 的 `russh`，彻底摆脱 OpenSSL/perl/C 工具链依赖
- **v0.4.1**：远程 Windows 支持（ls/ps/tree/cat/net/service/reg/info）+ GBK 编码自动转换
- **v0.4.0**：系统命令族（ps/service/reg/net/sysinfo）、自举（rxt 用自己的 git 推送自己）、MCP server 模式、trash/recipe/clip/repeat/notify/dup/bench 等工具、Windows `rxt.exe` 部署

---

## 许可

MIT
