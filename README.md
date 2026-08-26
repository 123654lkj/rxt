# rxt — Run eXternal Tools

> **跑外部工具**。小核心，全插件。同一条命令管本地和远程。

`rxt` 不再是「把 70 个命令焊死在一个二进制里」。  
**0.10** 起：宿主只留 8 个核心命令；`pack` / `grep` / `mem` / `http` / `read` … 全部是可单独安装、卸载的插件。加功能不必重编 rxt。

```
rxt          核心宿主（~7MB）
rxt-tools    官方标准库（一份多路调用）
~/.rxt/plugins/<name>/   每个命令一个插件，可装卸
```

```bash
rxt pack . -b 5000                 # 标准库插件
rxt --host server pack /opt/app    # 核心把命令转到远端 rxt
rxt plugin remove http             # 卸掉，rxt http 立刻消失
rxt plugin add http                # 装回来，不用重编
rxt plugin new hello --body 'echo hi'
rxt hello
```

---

## 为什么叫这个名字

| 字母 | 单词 | 意思 |
|------|------|------|
| **R** | Run | 宿主只负责把命令跑起来 |
| **X** | eXternal | 真正干活的是外面的插件，不是焊死在 rxt 里 |
| **T** | Tools | `pack` / `grep` / 你自己写的 `rxt-foo` |

`rxt pack` = 跑名为 pack 的外部工具。`--host` 也一样，只是跑到另一台机器上。

旧称 “Rust Codex Tools” / “Remote eXtension Toolkit” 已弃用。命令名仍是 `rxt`。

---

## 核心 vs 插件

**焊在 rxt 里的只有：**

```
plugin   exec   info   version   upgrade   deploy   publish   sign
```

其余 67 个官方命令（`pack` `grep` `read` `write` `mem` `http` `git` `map` …）走 `rxt-tools`，按名字注册成插件。

```bash
rxt plugin seed              # 安装全套官方标准库
rxt plugin list              # core / installed / path / recipes
rxt plugin seed pack         # 只装一个
rxt plugin remove grep       # 卸载某一个
rxt plugin add grep          # 官方名：装回标准库
rxt plugin new foo --lang py # 自己的插件
rxt plugin install ./rxt-foo # 现成脚本或 exe
rxt plugin remove foo
```

用户插件和官方插件同一套调度，互不影响。官方实现是一份 `~/.rxt/lib/rxt-tools`，每个名字一个符号链接（Windows 为 `.cmd` 启动器），所以卸 `http` 不会复制 16MB 七十份。

---

## 安装

```bash
git clone https://github.com/123654lkj/rxt.git
cd rxt
cargo build --release --bin rxt --bin rxt-tools

# 核心
cp target/release/rxt ~/.local/bin/
# 标准库二进制（plugin seed 会再拷一份到 ~/.rxt/lib/）
cp target/release/rxt-tools ~/.local/bin/

rxt plugin seed              # 第一次必做：挂上 pack/grep/mem/…
```

交叉编译 Windows：

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu --bin rxt --bin rxt-tools
# target/x86_64-pc-windows-gnu/release/rxt.exe
# target/x86_64-pc-windows-gnu/release/rxt-tools.exe
```

一键发版（编两平台 + 装本地 + seed + 部署）：

```bash
rxt publish --no-push
```

---

## 远程主机

`~/.rxt/hosts.toml`（密码请用 `password_env`，不要写进仓库）：

```toml
[hosts.alpha]
host = "10.0.0.10"
user = "deploy"
password_env = "RXT_PASS_ALPHA"
port = 22

[hosts.win]
os = "windows"
host = "10.0.0.20"
user = "alice"
key = "~/.ssh/id_ed25519"

[hosts.isolated]
host = "10.1.0.8"
jump_host = "alpha"          # 先 SSH 到 alpha，再隧道到 isolated

[group.all]
members = ["alpha", "win"]
```

认证优先级：密钥 `key` > 明文 `password` > `password_env`。

```bash
rxt --host alpha exec "hostname"
rxt --host alpha pack /opt/app -b 4000    # 远端执行 rxt pack（远端也要 seed）
rxt --group all version
```

底层 SSH 是纯 Rust [russh](https://crates.io/crates/russh)，不依赖 OpenSSL。

---

## 调度顺序

```
rxt [--host H] <cmd> args
        │
        ├─ 核心 8 个命令 → 本进程
        └─ 其它
              ├─ --host → SSH 到 H 再跑 `rxt <cmd> args`
              ├─ ~/.rxt/plugins/<cmd>/   已安装插件
              ├─ PATH 上的 rxt-<cmd>
              ├─ ~/.rxt-recipes/<cmd>    一行宏
              └─ 提示：plugin seed / plugin new / recipe add
```

插件进程：argv **不含**子命令名；`--host`/`--group` 剥掉后写入 `RXT_HOST` / `RXT_GROUP`。官方标准库由核心做远程转发，用户脚本插件读环境变量即可。

写插件正本：[docs/PLUGIN.md](docs/PLUGIN.md)

---

## 常用命令（均为插件，除非标了核心）

| 命令 | 位置 | 作用 |
|------|------|------|
| `plugin` | 核心 | 创建 / 安装 / seed / 卸载 |
| `exec` | 核心 | 本机或远程跑代码/shell |
| `pack` | 插件 | 项目一键简报，硬预算省 token |
| `map` / `digest` / `ctx` / `refs` | 插件 | 结构 / 骨架 / 上下文 / 调用链 |
| `grep` / `find` / `search` | 插件 | 搜内容 / 搜文件 |
| `read` / `write` / `ls` / `tree` | 插件 | 文件与目录 |
| `mem` | 插件 | 星枢记忆 |
| `http` | 插件 | HTTP 客户端 |
| `git` | 插件 | AI 友好 git |
| `recipe` | 插件 | 一行宏；也可 `rxt <name>` |
| `mcp` | 插件 | MCP stdio（默认 slim） |

完整列表：`rxt plugin seed` 之后 `rxt plugin list`，或 `rxt --help`（核心）/ `rxt-tools --help`（标准库）。

```bash
rxt mcp                # AI 用：默认 --slim
rxt mcp --full         # 暴露已安装插件的 schema
```

---

## 架构

```
src/main.rs        → 二进制 rxt         核心宿主
src/core_cli.rs    → 8 个核心子命令
src/bin/rxt-tools.rs
src/tools_app.rs   → 二进制 rxt-tools   标准库多路调用
src/plugin.rs      → seed / install / 远程转发
src/lib.rs         → 共享实现
```

`--host` 对核心 `exec` 仍走原来的 RemoteChannel（本地脑子、远端 IO）。  
对插件则转发成远端 `rxt <cmd>`，所以 **每台机器都要装核心 + 需要的插件**。

---

## Feature flags

| feature | 默认 | 作用 |
|---------|------|------|
| `remote` | ✅ | SSH（russh） |
| `http` | ✅ | `http` / `mem`（ureq + Lightpanda CDP） |
| `xz` | ✅ | `.tar.xz` |
| `cookies` | ❌ | 读浏览器 Cookie（Windows 常编不过） |

---

## 版本

### v0.10.1

- `rxt http`：CLI 网页会话（open/snap/read/fill/click/eval/net/wait）
- Lightpanda JS 引擎（不跑 Chrome），拦截 XHR/fetch，会话登录态
- Cookie 从 Chrome/Edge/Firefox/Tabbit 等主流浏览器导入，GET/POST 自动带 Cookie + SSO Bearer/CSRF

### v0.10.0

- **主体重写**：核心只留 `plugin/exec/info/version/upgrade/deploy/publish/sign`
- 67 个业务命令改为标准库插件，`rxt plugin seed/remove/add` 单独装卸，不必重编 rxt
- 二进制拆成 `rxt` + `rxt-tools`（多路调用，不是 70 份拷贝）
- `--host` 对插件改为远端执行 `rxt <cmd>`
- 品牌：Run eXternal Tools（跑外部工具），不再使用 “Rust Codex Tools”

### v0.9.4

- `plugin new` / 智能 `add` / 脚本插件 / 目录整树拷贝 / recipe 回退为子命令

更早记录见 git log。
