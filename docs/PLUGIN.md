# rxt 插件开发指南

> **rxt** = **Run eXternal Tools**（跑外部工具）。  
> 对照 **rxt 0.10.0**。核心宿主 `src/core_cli.rs`；标准库 `src/tools_app.rs` + `rxt-tools`。  
> 写插件只看这一页：创建、安装、契约、调度、管理命令。

## 0.10 主体：核心 8 个，其余全是可装卸插件

rxt 二进制只留宿主：

```
plugin  exec  info  version  upgrade  deploy  publish  sign
```

`pack` / `grep` / `mem` / `http` / `read` … **67 个**官方命令是标准库插件（一份 `~/.rxt/lib/rxt-tools`，每个名字一个目录）。不必重编 rxt。

```bash
rxt plugin seed              # 装全套标准库
rxt plugin seed pack         # 只装一个
rxt plugin remove http       # 卸掉，rxt http 立刻消失
rxt plugin add http          # 再装回来（官方名走 seed，不必给路径）
rxt plugin new foo --body 'echo hi'   # 自己的插件
```

`--host`：核心把 `rxt --host huhu pack …` 转发成远端 `rxt pack …`（远端也要装着对应插件）。

rxt 的插件是 **Git 风格外挂**：未知子命令不会报「没有这个命令」就结束，而是按顺序找插件 / PATH / recipe。

```bash
rxt hello --flag a     # 已安装插件 / PATH 上的 rxt-hello / 同名 recipe
```

---

## 30 秒上手（创建，不必先有 exe）

```bash
# 1. 一键建插件（Linux 默认 bash；Windows 默认 cmd；Git Bash 下默认 sh）
rxt plugin new hello
rxt hello

# 2. 带正文 / 指定语言
rxt plugin new hello --body 'echo argv: $*'
rxt plugin new hello --lang py --body 'import sys; print(sys.argv[1:])'
printf '%s\n' 'echo from-stdin' | rxt plugin new hello --stdin

# 3. 智能 add：名字 → 创建；已有文件/目录 → 安装
rxt plugin add hello --body 'echo hi'
rxt plugin add ./rxt-hello.sh
rxt plugin add ./my-plugin-dir

# 4. 改 / 看 / 卸
rxt plugin edit hello
rxt plugin show hello
rxt plugin which hello
rxt plugin remove hello
```

一行宏（不是插件，但也能 `rxt <name>`）：

```bash
rxt recipe add hello "echo hi \$1"
rxt hello world          # 0.9.4 起，未知子命令会回退到 recipe
rxt recipe run hello world
```

不想 install、只想随 PATH 生效：把可执行文件命名为 `rxt-<name>` 丢进 `PATH` 即可。

---

## 调度顺序（写插件必须知道）

`rxt` 启动后按这个顺序决定跑谁：

```
argv
  │
  ├─ 1. 读 ~/.rxt/env
  ├─ 2. --describe / --help / --version → 内置，不走插件
  ├─ 3. 若 ~/.rxt/plugins/<name>/manifest.toml 里 force=true
  │      → 立刻 spawn 插件（可覆盖同名内置命令）
  ├─ 4. clap 解析
  │      ├─ 认识的内置子命令 → 走内置（插件到不了）
  │      └─ 不认识 → Command::External
  └─ 5. External 解析：
         a. ~/.rxt/plugins/<name>/ 已安装（manifest + exe 都在）
         b. PATH 上的 rxt-<name>（Windows 是 rxt-<name>.exe / .cmd）
         c. ~/.rxt-recipes/<name>.sh（Windows .cmd）— 0.9.4
         d. 都没有 → 报错，提示 plugin new / recipe add / plugin install
```

要点：

- **默认盖不住内置命令。** `rxt ls` 永远是内置 `ls`，除非 `rxt plugin new ls --force`（或 install `--force`）。
- **`--force` 在 clap 之前拦截**，所以能劫持 `read`/`http` 这类名字。没把握别 force。
- **插件优先于 recipe。** 同名时 `rxt hello` 走插件，recipe 还在，用 `rxt recipe run hello`。
- 插件 **不会自动远程执行**。`--host` / `--group` 只从 argv 里剥掉，塞进环境变量。要远程，插件自己读 `RXT_HOST` 再去连。
- recipe 当子命令跑时 **不打印** `▶ 执行 recipe` 横幅（安静，像插件）；`rxt recipe run` 仍有横幅。

---

## 目录与清单

创建 / 安装后的形状：

```
~/.rxt/plugins/
  <name>/
    manifest.toml
    rxt-<name>.sh       # --lang sh（Linux 直接 spawn）
    rxt-<name>.py       # --lang py
    rxt-<name>.cmd      # --lang cmd；Windows 上 sh/py/ps1 的启动器
    rxt-<name>.ps1      # --lang ps1
    rxt-<name>.exe      # 原生二进制（Windows）
    …其余资源文件…      # install 目录时整树拷贝
```

可用环境变量改位置：

| 变量 | 作用 |
|------|------|
| `RXT_HOME` | 默认 `~/.rxt`，插件目录为其下 `plugins/` |
| `RXT_PLUGINS_DIR` | 直接指定插件根目录 |
| `RXT_RECIPES_DIR` | recipe 目录（默认 `~/.rxt-recipes`） |

`manifest.toml` 三个字段，全是 TOML 标量：

```toml
name = "hello"           # 子命令名，小写
exe = "rxt-hello.sh"     # 目录里真正 spawn 的文件名
force = false            # true = 覆盖同名内置
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 出现在 `rxt <name>` 里的名字 |
| `exe` | string | 相对该插件目录的可执行文件名 |
| `force` | bool | 缺省 `false`。`true` 才能覆盖内置 |

手写目录也能用：保证 `manifest.toml` + exe 在，`rxt <name>` 就能找到。

---

## 名字规则

`sanitize()` 会做这些事：

1. 去掉前缀 `rxt-`（大小写不敏感）
2. 去掉后缀 `.exe`
3. 转成小写
4. 只允许 `A-Za-z0-9_-`

非法例子：`../x`、空名、带 `/` `\` 空格点号。

```
rxt-foo.exe  →  foo
Bar_1        →  bar_1
rxt-hello    →  hello
```

不要和内置撞名。当前内置（0.9.4，`plugin.rs` 的 `BUILTINS`）：

```
replace read write cat jsonl patch stat find struct diff dep sed grep search
py mem tree jq unzip ls http edit hash uuid enc dec watch tail time exec
sort uniq cut count build check size clean normalize info git ctx map digest
pack refs churn dead trace impact publish sysinfo ps service reg net upgrade
deploy version sync serve snapshot qr clip repeat notify dup trash recipe
bench watch-run evolve mcp plugin sign
```

`rxt plugin list` 会打印完整 builtin 列表。

---

## 运行契约（插件进程看到什么）

rxt 用 `std::process::Command` 直接 spawn，**不包一层 shell**。

| 项 | 行为 |
|----|------|
| 可执行文件 | `~/.rxt/plugins/<name>/` 里 manifest.exe，或 PATH 上的 `rxt-<name>` |
| argv | **不含** 子命令名。`rxt hello --flag a` → 插件收到 `--flag a` |
| 工作目录 | 继承调用方 cwd |
| stdin / stdout / stderr | 原样继承。rxt 不截获、不包 JSON |
| 退出码 | `0` = 成功；非 0 → rxt 报 `插件 <path> 退出 <code>` 并以失败返回 |
| 环境 | 继承全部现有环境 |

额外注入（有才设）：

| 环境变量 | 来源 | 插件该怎么用 |
|----------|------|----------------|
| `RXT_HOST` | 全局 `--host` / `--host=` | 远程主机别名（`~/.rxt/hosts.toml`） |
| `RXT_GROUP` | 全局 `--group` / `--group=` | 主机组，批量 |

`--host` / `--group` **不会**出现在插件 argv 里，无论写在子命令前还是后：

```bash
rxt --host huhu hello --flag
rxt hello --host huhu --flag
# 两种都是：exe --flag ，且 RXT_HOST=huhu
```

插件里自己的 `--host` 如果也想当参数用，会被剥掉。需要主机参数时 **只读 `RXT_HOST`**，不要在插件 CLI 里再定义 `--host`。

### 远程是插件自己的事

rxt **不会**把插件二进制传到远端再执行。典型做法：

```bash
# 插件内部
if [ -n "$RXT_HOST" ]; then
  exec rxt --host "$RXT_HOST" exec "你的远程命令"
fi
# 否则跑本地
```

或者插件直接读 `~/.rxt/hosts.toml` 自己 SSH。没有 `RXT_HOST` 就当本地。

---

## 创建 vs 安装

| 你有什么 | 命令 |
|----------|------|
| 还没有文件，想要 `rxt foo` | `rxt plugin new foo` |
| 一段脚本正文 | `rxt plugin new foo --body '…'` / `--stdin` / 位置参数 |
| 本地已有脚本或 exe | `rxt plugin add ./file` 或 `rxt plugin install ./file` |
| 一个目录（脚本+资源） | `rxt plugin install ./dir`（**整树拷贝**，跳过 `.git` / `node_modules` / `target`） |
| 只要一行 shell，不想维护插件目录 | `rxt recipe add foo "命令"`，然后 `rxt foo` |

### `new` / `create` / `init`

```bash
rxt plugin new <name> [--lang sh|py|cmd|ps1] [--body TEXT | --stdin] [--open] [--force] [--json]
rxt plugin new hello "echo argv: \$*"          # 位置参数 = 正文
rxt plugin new hello --lang py
rxt plugin new hello --open                    # 写完打开 $VISUAL/$EDITOR
```

`--lang` 别名：`bash|shell|zsh` → sh；`python|python3` → py；`bat|batch` → cmd；`ps|powershell|pwsh` → ps1。

没写 `--lang` 时：看正文 shebang / `@echo`；再不行 Linux=`sh`，Windows=`cmd`，**Git Bash（`MSYSTEM` 或 `$SHELL` 含 bash）=`sh`**。

`--force` 两件事：覆盖已安装的同名插件；名字是内置时写入 `manifest.force=true`。

已有同名 recipe 时会警告：`rxt hello` 优先走插件。

**不要**只因 stdin 不是 tty 就自动读入（Agent 会把后续输入吃掉）。必须 `--stdin` 或 `--body -` 或 target `-`。

### `add`（智能）

```bash
rxt plugin add hello                 # 名字 → 等同 new（写 stub）
rxt plugin add hello --body 'echo 1'
rxt plugin add ./rxt-hello           # 路径存在 → 等同 install
rxt plugin add ./rxt-hello.py --name hi
```

路径长得像路径但不存在（`./x`、`foo.py`、`.exe`）→ 报「不存在」，不会当成插件名。

`add` 仍是 install 的别名 **仅当 target 是已存在路径**。这是 0.9.3 的坑：`rxt plugin add hello` 以前会去找文件 `hello`。

### `install`

| 源 | 行为 |
|----|------|
| 单个 `.exe` | 拷成 `rxt-<name>.exe`（Windows 自动签名） |
| 单个 `.sh/.py/.cmd/.ps1` 或带 shebang 的文本 | 按语言落盘 + 需要时写启动器（Windows `.cmd` 调 Git Bash / python / pwsh） |
| 目录且有 `manifest.toml` | **整树拷贝**（保留相对资源路径），更新 name/force |
| 目录无清单 | 找第一个 `rxt-*`，整树拷贝，生成清单 |

不再「只拷一个 exe」。多文件插件请用目录 install，不要只靠 PATH。

Windows **不再强制必须是 .exe**。脚本用 `.cmd` 启动器 spawn，不走 Authenticode。只有真正的 `.exe` 才签名。

`--name` 覆盖从文件名/清单推出来的名字。  
撞内置且没 `--force` → 直接失败：`'<n>' 是内置命令。覆盖请加 --force`。

安装是 staging + rename，失败会把旧目录改回去。

---

## 语言与跨平台启动器

| lang | Linux spawn | Windows spawn | 源文件 |
|------|-------------|---------------|--------|
| sh | `rxt-name.sh`（shebang） | `rxt-name.cmd` → Git Bash `bash rxt-name.sh` | `.sh` |
| py | `rxt-name.py`（shebang） | `rxt-name.cmd` → `py -3` / `python` / `python3` | `.py` |
| cmd | `rxt-name` 占位脚本（有 `cmd.exe` 时转过去，否则报错） | `rxt-name.cmd` | `.cmd` |
| ps1 | `rxt-name.ps1`（`#!/usr/bin/env pwsh`） | `rxt-name.cmd` → `pwsh` / `powershell` | `.ps1` |

Windows 找 bash 的顺序：PATH 上的 `bash` → `%ProgramFiles%\Git\bin\bash.exe` → x86 → `%LOCALAPPDATA%\Programs\Git\bin\bash.exe`。

`rxt plugin edit` 打开**源文件**（`.py/.sh/.ps1`），不是 `.cmd` 启动器。

---

## 管理命令

```bash
rxt plugin                         # 默认 list（builtin / installed / path / recipes）
rxt plugin list
rxt plugin list --json             # Agent 用这个
rxt plugin new <name> …
rxt plugin add <name|path> …
rxt plugin install <exe|dir> [--name foo] [--force]
rxt plugin show <name> [--json]    # 找不到插件则回退 recipe
rxt plugin edit <name>             # $VISUAL 或 $EDITOR；回退 recipe
rxt plugin which <name> [--json]   # force / builtin / installed / path / recipe
rxt plugin remove <name>           # 别名 rm / uninstall；不删 recipe
```

`which` 输出：

| 情况 | 打印 |
|------|------|
| force 覆盖 | `<path> (force)` |
| 内置 | `builtin` |
| 已安装或 PATH | 绝对路径 |
| recipe | `<path> (recipe)` |
| 没有 | 退出非 0：`找不到: <name>` |

`list --json` 形状：

```json
{
  "builtins": ["replace", "read", "..."],
  "installed": [{ "name": "hello", "path": "/home/you/.rxt/plugins/hello/rxt-hello.sh", "force": false }],
  "path": [{ "name": "http", "path": "/home/you/.local/bin/rxt-http" }],
  "recipes": [{ "name": "hello", "path": "/home/you/.rxt-recipes/hello.sh" }]
}
```

`new --json`：

```json
{
  "ok": true,
  "name": "hello",
  "lang": "sh",
  "dir": ".../hello",
  "exe": ".../rxt-hello.sh",
  "source": ".../rxt-hello.sh",
  "force": false,
  "run": "rxt hello",
  "recipe_shadowed": null
}
```

`path` 是扫 PATH 时发现的 `rxt-*`，**不等于已经在用**。现网有时会看到 `rxt-http` 其实是整份 rxt 二进制的拷贝，只要没 `--force`，`rxt http` 仍走内置。

`remove` 只删插件目录。若只有 recipe：提示 `rxt recipe rm <name>`。

---

## 最小例子

### 一键（推荐）

```bash
rxt plugin new hello --body 'echo hello argv=$*; echo host=${RXT_HOST:-}'
rxt hello world
rxt --host huhu hello
```

### Bash 手写再安装

```bash
#!/usr/bin/env bash
set -euo pipefail
echo "rxt-hello argv=$*"
if [[ -n "${RXT_HOST:-}" ]]; then
  echo "remote host=$RXT_HOST"
fi
```

```bash
chmod +x rxt-hello
rxt plugin install ./rxt-hello --name hello
# 或: rxt plugin add ./rxt-hello
```

### Python

```python
#!/usr/bin/env python3
import os, sys
print("argv", sys.argv[1:])
print("host", os.environ.get("RXT_HOST", ""))
```

```bash
rxt plugin new hello --lang py
# 或 Windows：rxt plugin add .\hello.py
```

Linux 可直接 install 带 shebang 的 `.py`。Windows 会再写一份 `.cmd` 启动器。

### Rust 二进制

```bash
cargo build --release
rxt plugin install ./target/release/rxt-hello --name hello
```

Cargo 包名用 `rxt-hello`，产物文件名就会对上。

### 多文件插件

```
myplug/
  manifest.toml
  rxt-myplug.sh
  lib/helper.sh
```

```bash
rxt plugin install ./myplug
# ~/.rxt/plugins/myplug/lib/helper.sh 还在，脚本里用 "$0" 定位即可
```

---

## Windows 额外规则

- **脚本可以当插件**（0.9.4）：`.sh` / `.py` / `.cmd` / `.ps1` 会生成 `.cmd` 启动器。
- 原生二进制仍必须是 `.exe`，install 后走 `rxt sign`（CN=`rxt-codesign`）。
- 证书导出在 `~/.rxt/rxt-codesign.cer`。
- 新 exe 若已被 WDAC 4551 拦住，**被拦的程序没法先启动再给自己签名**。用还能跑的旧 `rxt.exe` 执行：

```text
rxt sign <新exe> --trust
```

仍拦就把 cer 加进代码完整性签名者规则（策略级，再编一遍解决不了）。

仙兔儿 Windows 侧常用 Git Bash：`rxt plugin new foo` 在 `MSYSTEM`/`SHELL=bash` 下默认 `--lang sh`，启动器会找 Git 的 bash。

---

## 给 Agent 的调用卡

```text
建：    rxt plugin new <n> [--lang sh|py|cmd|ps1] [--body <脚本>] [--stdin] [--force] [--json]
        rxt plugin add <n> --body '…'          # 无文件时创建
        rxt plugin add <exe|dir> [--name <n>]  # 有文件时安装
装：    rxt plugin install <exe|dir> [--name <n>] [--force]
卸：    rxt plugin remove <n>
查：    rxt plugin which <n> [--json]
看：    rxt plugin show <n> [--json]
改：    rxt plugin edit <n>
列表：  rxt plugin list --json
跑：    rxt <n> [插件自己的参数…]
宏：    rxt recipe add <n> "命令"   →  也可 rxt <n>
远程：  rxt --host <alias> <n> …     → 插件读 $RXT_HOST
覆盖：  只有 --force（manifest.force=true）才能盖内置
名字：  [a-z0-9_-]+ ，会剥 rxt- 和 .exe
契约：  argv 不含子命令名；stdin/out 直通；不自动 SSH
资源：  install 目录整树拷贝（跳过 .git/node_modules/target）
stdin： 必须 --stdin / --body - ，不会因 !tty 自动读
源码：  src/plugin.rs
```

---

## 排错

| 现象 | 原因 | 处理 |
|------|------|------|
| `未知命令 'foo'` | 没装、不在 PATH、没有 recipe、名字没过 sanitize | `rxt plugin new foo` 或 `rxt recipe add foo "…"` |
| `rxt foo` 跑成了内置 | `foo` 在 BUILTINS 里 | 换名，或 `--force`（慎用） |
| `rxt foo` 跑成了插件而不是 recipe | 插件优先 | `rxt recipe run foo` 或 `plugin remove foo` |
| 插件没收到 `--host` | 全局 flag 被剥掉 | 读 `RXT_HOST` |
| 装了但没带上资源 | 旧版只拷 exe | **0.9.4 目录 install 整树拷贝**；或 embed |
| Windows `必须是 .exe` | 旧版限制 | **0.9.4 脚本可装**；真二进制仍要 .exe |
| `插件 … 退出 1` | 插件自己非 0 | 先直接跑那个 exe/脚本看 stderr |
| PATH 列出 `http` 但 `rxt http` 不像插件 | PATH 上有一份 `rxt-http` 整包拷贝 | 正常：无 force 时内置优先 |
| `rxt plugin add hello` 报不存在 | 0.9.3 add=install | 0.9.4 起无文件则创建 |
| `--stdin` 把 Agent 输入吃掉 | 你开了 `--stdin` | 不要对非交互 Agent 开 `--stdin`；用 `--body` |
| Windows sh 插件报找不到 bash | 没装 Git Bash / bash 不在 PATH | 装 Git for Windows，或 `--lang cmd` |

直接验证解析（不靠猜）：

```bash
rxt plugin which hello
rxt plugin list --json
rxt plugin show hello
# 然后不经过 rxt，直接跑：
~/.rxt/plugins/hello/rxt-hello.sh --flag
```

---

## 源码锚点

| 行为 | 位置 |
|------|------|
| 清单 / 创建 / 安装 / 解析 | `src/plugin.rs` |
| clap `allow_external_subcommands` + `External` | `src/main.rs` `Cli` / `Command::External` |
| force 覆盖（clap 之前） | `main()` → `plugin::run_forced_override` |
| 未知命令入口 | `Command::External` → `plugin::run_external` → recipe 回退 |
| recipe 当子命令 | `src/recipe.rs` `try_run_as_command` |
| Windows 签名 | `src/sign.rs`（原生 .exe install 成功前调用） |

改注册结构时先改 `plugin.rs`，再同步这一页。
