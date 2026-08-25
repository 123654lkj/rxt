# rxt 插件开发指南

> 对照 **rxt 0.9.2** 源码 `src/plugin.rs` + `src/main.rs`。  
> 写插件只看这一页：注册、契约、最小例子、管理命令。

rxt 的插件是 **Git 风格外挂**：未知子命令不会报「没有这个命令」就结束，而是去找一个叫 `rxt-<name>` 的可执行文件来跑。

```bash
rxt hello --flag a     # 等价于执行 rxt-hello --flag a
```

---

## 30 秒上手

```bash
# 1. 写一个可执行文件（Linux 任意 shebang；Windows 必须是 .exe）
cat > rxt-hello <<'EOF'
#!/usr/bin/env bash
echo "hello from plugin"
echo "argv: $*"
echo "RXT_HOST=${RXT_HOST:-} RXT_GROUP=${RXT_GROUP:-}"
EOF
chmod +x rxt-hello

# 2. 注册到本机
rxt plugin install ./rxt-hello
# 装到 ~/.rxt/plugins/hello/rxt-hello + manifest.toml

# 3. 当 rxt 子命令用
rxt hello
rxt --host huhu hello --any --flag
rxt plugin which hello
rxt plugin remove hello
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
         b. PATH 上的 rxt-<name>（Windows 是 rxt-<name>.exe）
         c. 都没有 → 报错：未知命令，提示 rxt plugin install
```

要点：

- **默认盖不住内置命令。** `rxt ls` 永远是内置 `ls`，除非 `rxt plugin install ./rxt-ls --force`。
- **`--force` 在 clap 之前拦截**，所以能劫持 `read`/`http` 这类名字。没把握别 force。
- 插件 **不会自动远程执行**。`--host` / `--group` 只从 argv 里剥掉，塞进环境变量。要远程，插件自己读 `RXT_HOST` 再去连。

---

## 目录与清单

安装后的形状：

```
~/.rxt/plugins/
  <name>/
    manifest.toml
    rxt-<name>          # Linux
    rxt-<name>.exe      # Windows
```

`manifest.toml` 三个字段，全是 TOML 标量：

```toml
name = "hello"           # 子命令名，小写
exe = "rxt-hello"        # 目录里的文件名（Windows 为 rxt-hello.exe）
force = false            # true = 覆盖同名内置
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 出现在 `rxt <name>` 里的名字 |
| `exe` | string | 相对该插件目录的可执行文件名 |
| `force` | bool | 缺省 `false`。`true` 才能覆盖内置 |

手写目录也能用：保证 `manifest.toml` + exe 在，`rxt <name>` 就能找到。`rxt plugin install` 只是帮你拷文件、写清单、chmod、Windows 签名。

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

不要和内置撞名。当前内置（0.9.2，`plugin.rs` 的 `BUILTINS`）：

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
| 可执行文件 | `~/.rxt/plugins/<name>/rxt-<name>` 或 PATH 上的 `rxt-<name>` |
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

### install 只拷一个 exe

`rxt plugin install <dir>` **不会**把目录里的资源文件一起拷走，只拷那个可执行文件 + 生成 `manifest.toml`。

插件如果还要配置、模板、模型文件：

- 做成单一二进制（embed），或
- **不要 install**，把 `rxt-<name>` 放 PATH，旁边放资源，用 `$0` / `argv[0]` 定位自身目录。

---

## 管理命令

```bash
rxt plugin                         # 默认 list
rxt plugin list
rxt plugin list --json             # Agent 用这个
rxt plugin install <exe|dir> [--name foo] [--force]
rxt plugin remove <name>           # 别名 rm / uninstall
rxt plugin which <name>
```

`install` 行为：

| 源 | 怎么解析名字和 exe |
|----|--------------------|
| 文件 | stem → 名字；该文件就是 exe |
| 目录且有 `manifest.toml` | 用清单里的 `name` + `exe` |
| 目录无清单 | 找第一个文件名以 `rxt-` 开头的文件 |

然后：

1. 写到临时目录 `~/.rxt/plugins/.<name>.install-<pid>/`
2. Linux `chmod 0755`；Windows 调用 `rxt sign` 签这个 exe
3. 若目标已存在，先改名为 `.<name>.backup-<pid>`
4. rename 成正式目录；失败则把 backup 改回去
5. 打印 `# 已安装 <name> -> <exe路径>`

`--name` 覆盖从文件名/清单推出来的名字。  
撞内置且没 `--force` → 直接失败：`'<n>' 是内置命令。覆盖请加 --force`。

`which` 输出：

| 情况 | 打印 |
|------|------|
| force 覆盖 | `<path> (force)` |
| 内置 | `builtin` |
| 已安装或 PATH | 绝对路径 |
| 没有 | 退出非 0：`找不到: <name>` |

`list --json` 形状：

```json
{
  "builtins": ["replace", "read", "..."],
  "installed": [{ "name": "hello", "path": "/home/you/.rxt/plugins/hello/rxt-hello", "force": false }],
  "path": [{ "name": "http", "path": "/home/you/.local/bin/rxt-http" }]
}
```

`path` 是扫 PATH 时发现的 `rxt-*`，**不等于已经在用**。现网有时会看到 `rxt-http` 其实是整份 rxt 二进制的拷贝，只要没 `--force`，`rxt http` 仍走内置。

---

## 最小例子

### Bash（Linux / macOS）

```bash
#!/usr/bin/env bash
# rxt-hello — 保存为 rxt-hello 后 chmod +x
set -euo pipefail
echo "rxt-hello argv=$*"
if [[ -n "${RXT_HOST:-}" ]]; then
  echo "remote host=$RXT_HOST"
fi
# 退出码会传回 rxt
```

```bash
rxt plugin install ./rxt-hello --name hello
rxt hello world
```

### Python

```python
#!/usr/bin/env python3
# rxt-hello
import os, sys
print("argv", sys.argv[1:])
print("host", os.environ.get("RXT_HOST", ""))
print("group", os.environ.get("RXT_GROUP", ""))
sys.exit(0)
```

Linux 可直接 `rxt plugin install ./rxt-hello`。Windows 需要先打成 `.exe`（PyInstaller 等），再 install。

### Rust

```rust
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = std::env::var("RXT_HOST").unwrap_or_default();
    println!("hello args={args:?} host={host}");
}
```

```bash
cargo build --release
rxt plugin install ./target/release/rxt-hello --name hello
```

Cargo 包名用 `rxt-hello`，产物文件名就会对上。

---

## Windows 额外规则

- 源文件 **必须是 `.exe`**，否则 install 失败。
- install / `rxt build` / `upgrade` 都会签 Authenticode（CN=`rxt-codesign`）。
- 证书导出在 `~/.rxt/rxt-codesign.cer`。
- 新 exe 若已被 WDAC 4551 拦住，**被拦的程序没法先启动再给自己签名**。用还能跑的旧 `rxt.exe` 执行：

```text
rxt sign <新exe> --trust
```

仍拦就把 cer 加进代码完整性签名者规则（策略级，再编一遍解决不了）。

---

## 给 Agent 的调用卡

```text
装：    rxt plugin install <exe|dir> [--name <n>] [--force]
卸：    rxt plugin remove <n>
查：    rxt plugin which <n>
列表：  rxt plugin list --json
跑：    rxt <n> [插件自己的参数…]
远程：  rxt --host <alias> <n> …     → 插件读 $RXT_HOST
覆盖：  只有 --force（manifest.force=true）才能盖内置
名字：  [a-z0-9_-]+ ，会剥 rxt- 和 .exe
契约：  argv 不含子命令名；stdin/out 直通；不自动 SSH
资源：  install 只拷一个 exe；多文件插件走 PATH
源码：  src/plugin.rs
```

---

## 排错

| 现象 | 原因 | 处理 |
|------|------|------|
| `未知命令 'foo'` | 没装、不在 PATH、名字没过 sanitize | `rxt plugin list`；确认文件名 `rxt-foo` |
| `rxt foo` 跑成了内置 | `foo` 在 BUILTINS 里 | 换名，或 `--force`（慎用） |
| 插件没收到 `--host` | 全局 flag 被剥掉 | 读 `RXT_HOST` |
| 装了但没带上 `.py` / 配置 | install 只拷 exe | embed，或改 PATH 布局 |
| Windows `必须是 .exe` | 源不是 exe | 先编/打包再装 |
| `插件 … 退出 1` | 插件自己非 0 | 先直接跑那个 exe 看 stderr |
| PATH 列出 `http` 但 `rxt http` 不像插件 | PATH 上有一份 `rxt-http` 整包拷贝 | 正常：无 force 时内置优先 |

直接验证解析（不靠猜）：

```bash
rxt plugin which hello
rxt plugin list --json
# 然后不经过 rxt，直接跑：
~/.rxt/plugins/hello/rxt-hello --flag
```

---

## 源码锚点

| 行为 | 位置 |
|------|------|
| 清单 / 安装 / 解析 | `src/plugin.rs` |
| clap `allow_external_subcommands` + `External` | `src/main.rs` `Cli` / `Command::External` |
| force 覆盖（clap 之前） | `main()` → `plugin::run_forced_override` |
| 未知命令入口 | `Command::External` → `plugin::run_external` |
| Windows 签名 | `src/sign.rs`（install 成功前调用） |

改注册结构时先改 `plugin.rs`，再同步这一页。
