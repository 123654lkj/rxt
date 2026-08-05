# rxt 使用速查

> **rxt** (Rust Codex Tools) — 跨平台远程执行 + AI 友好代码工具。  
> 完整说明见 [README.md](./README.md)。

## 配置

### 主机 `~/.rxt/hosts.toml`

```toml
[hosts.lab]
host = "10.0.0.10"
user = "deploy"
password_env = "RXT_PASS_LAB"
port = 22

[hosts.winbox]
os = "windows"
host = "10.0.0.20"
user = "you"
key = "~/.ssh/id_ed25519"
jump_host = "bastion"   # 可选

[hosts.bastion]
host = "10.0.0.1"
user = "jump"
key = "~/.ssh/id_ed25519"

[group.all]
members = ["lab", "winbox"]
```

### 密钥与环境 `~/.rxt/env`（chmod 600）

```bash
RXT_PASS_LAB=...
# 可选
# RXT_NEBULA_URL=http://127.0.0.1:26670
# RXT_NEBULA_SSH=lab
# RXT_UPDATE_URL=http://10.0.0.10:26780
# RXT_PUBLISH_LINUX_HOSTS=lab
# RXT_PUBLISH_WINDOWS_HOSTS=winbox
```

全局选项：

| 选项 | 说明 | 示例 |
|------|------|------|
| `--host <HOST>` | SSH 远程执行 | `rxt --host lab exec "hostname"` |
| `--group <GROUP>` | 批量 | `rxt --group all version` |

---

## 常用命令

### 文件

```bash
rxt ls /var/log
rxt --host lab ls /etc
rxt read Cargo.toml
rxt write /tmp/hi.txt "hello"
rxt grep "TODO" src -t rs
rxt find --name "*.md" .
rxt pack . -b 5000
rxt digest src/
rxt trash old.log
```

### 系统

```bash
rxt ps --top 10
rxt --host lab ps --top 5
rxt sysinfo
rxt net --conn listen
rxt service --running          # Linux systemctl / Windows sc
```

### 执行

```bash
rxt exec "hostname && uptime"
rxt --host lab exec "df -h"
rxt --host winbox exec "Get-Process | Select -First 5"
rxt py -c "print(1+1)"
```

### 代码智能

```bash
rxt map .
rxt struct src/main.rs --functions
rxt refs main --callers
rxt trace main --depth 3
rxt impact --diff
rxt dead
rxt churn --since 30d
```

### 记忆 API（`rxt mem`）

默认 `http://127.0.0.1:26670`，用 `RXT_NEBULA_URL` 覆盖。

```bash
rxt mem help
rxt mem search "某主题"
rxt mem bootstrap 会话焦点
rxt mem save "要记住的内容"
```

### 更新频道（`rxt update`）

需自建 HTTP 目录并设置 `RXT_UPDATE_URL`：

```
manifest.json
rxt-x86_64-unknown-linux-gnu
rxt-aarch64-unknown-linux-gnu   # 可选
```

```bash
export RXT_UPDATE_URL=http://your-server:26780
rxt update --check
rxt update
```

### 一键发布（`rxt publish`）

```bash
export RXT_PUBLISH_LINUX_HOSTS=lab
export RXT_PUBLISH_WINDOWS_HOSTS=winbox
export RXT_PUBLISH_GIT_REMOTES=origin
rxt publish --message "v0.x.y: ..."
# 或
rxt publish --no-deploy --no-push   # 只编译+装本地
```

### MCP

```bash
rxt mcp                 # stdio JSON-RPC
```

Agent 配置示例：

```json
{
  "mcpServers": {
    "rxt": {
      "command": "rxt",
      "args": ["mcp"]
    }
  }
}
```

---

## 典型工作流

```bash
# 进仓一次看清
rxt pack . -b 5000

# 远程排查
rxt --host lab sysinfo
rxt --host lab ps --sort cpu --top 10
rxt --host lab tail /var/log/app.log

# 跨机同步后执行
rxt sync ./app lab:/opt/app
rxt --host lab exec "systemctl restart myapp"
```

---

## 版本与构建

```bash
rxt --version
cargo build --release
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --no-default-features   # 无 C 依赖纯本地
```
