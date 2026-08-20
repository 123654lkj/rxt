# rxt 使用手册 — AI Agent 操作指南

> **rxt** (Rust Codex Tools) — 仙兔儿的跨平台远程执行工具，版本 0.4.0
> 
> 核心能力：本地/SSH 远程文件读写、代码执行、搜索、Git 操作、系统管理，统一 CLI + MCP 双模式。
>
> 部署在三台机器：本地 Windows、huhu (192.168.31.252)、tuanzi (192.168.31.244)

---

## 一、全局参数

所有命令都支持以下全局参数：

| 参数 | 作用 | 示例 |
|------|------|------|
| `--host <HOST>` | SSH 远程执行 | `rxt exec --host huhu "hostname"` |
| `--group <GROUP>` | 批量执行多个主机 | `rxt exec --group all "uptime"` |
| `--json` | JSON 输出（部分命令支持） | `rxt ls --json` |

**主机配置**在 `~/.rxt/hosts.toml`，当前配置：

```toml
[hosts.huhu]
host = "192.168.31.252"
user = "huhu"
password = "Xiantuer123.."
port = 22

[hosts.tuanzi]
host = "192.168.31.244"
user = "tuanzi"
password = "Xiantuer123.."
port = 22

[hosts.xian]
os = "windows"
host = "192.168.31.169"
user = "xiantuer"
password = "Xiantuer123.."
port = 22
```

---

## 二、命令速查表

### 🔹 文件操作

| 命令 | 作用 | 常用示例 |
|------|------|----------|
| `read` | 读文件（自动检测编码/BOM/换行） | `rxt read /etc/hosts` |
| `write` | 写文件（自动保持目标格式） | `rxt write /tmp/x.txt "hello"` |
| `cat` | 打印文件内容（原始输出） | `rxt cat /var/log/syslog` |
| `stat` | 文件元信息 + 指纹 | `rxt stat --json file.txt` |
| `ls` | 目录列表 | `rxt ls -a --sort mtime /tmp` |
| `tree` | 目录树 | `rxt tree -L 2 -I "target\|.git"` |
| `diff` | 差异对比 | `rxt diff a.txt b.txt --side-by-side` |
| `patch` | 补丁工具 | `rxt patch --check file.txt` |
| `replace` | 块替换 | `rxt replace target --old "foo" --new "bar"` |
| `sed` | 安全替换（格式保持） | `rxt sed file.txt -p "old" -r "new"` |
| `edit` | 结构化编辑 | `rxt edit file.txt --before "xxx" --replace "yyy"` |
| `normalize` | 文件格式统一 | `rxt normalize --ending lf file.txt` |

#### read 详解

```bash
rxt read <PATH>                    # 读全文
rxt read -H 20 file.txt            # 只读前20行
rxt read -T 10 file.txt            # 只读后10行
rxt read -L 5-15 file.txt          # 读第5~15行
rxt read -n file.txt               # 带行号
rxt read -b 3000 file.txt          # token预算3000，超了截断
rxt read -e gbk file.txt           # 指定编码
rxt read --json file.txt           # JSON输出
rxt read --host huhu /etc/hosts    # 远程读取
```

#### write 详解

```bash
rxt write /tmp/x.txt "hello"                    # 写入内容
rxt write /tmp/x.txt --append "追加"             # 追加
rxt write /tmp/x.txt --file local.txt            # 从本地文件读取内容写远程
rxt write /tmp/x.txt --from local.txt            # 同上（远程写入专用）
rxt write /tmp/x.txt --b64 "base64内容"           # base64解码写入
rxt write /tmp/x.txt --preserve "保持原格式"       # 保持目标文件格式
```

#### sed / replace / edit 区别

| 命令 | 适用场景 | 特点 |
|------|----------|------|
| `sed` | 简单模式替换 | 格式保持，正则支持 |
| `replace` | 大块文本替换 | `--old`/`--new` 整段匹配 |
| `edit` | 结构化编辑 | 基于前后文定位，支持行范围 |

---

### 🔹 搜索

| 命令 | 作用 | 常用示例 |
|------|------|----------|
| `find` | 智能搜索（文件名+内容） | `rxt find "function main"` |
| `grep` | 跨文件 grep | `rxt grep "TODO" --type rs` |
| `search` | 统一搜索（自动判断搜名还是搜内容） | `rxt search "config.toml"` |
| `struct` | 代码结构分析 | `rxt struct --functions --json src/` |
| `refs` | 引用查找（谁调用谁） | `rxt refs "my_func"` |
| `digest` | 文件骨架（函数体折叠省token） | `rxt digest --budget 5000 large.rs` |
| `ctx` | AI上下文生成器 | `rxt ctx --max-lines 200 file.rs` |

#### find 详解

```bash
rxt find "search_text"                              # 搜内容
rxt find -n "*.rs"                                  # 搜文件名
rxt find -t rs "fn main"                            # 按类型过滤+搜内容
rxt find -p /path/to/dir "keyword"                  # 指定路径
rxt find --regex "fn\s+\w+"                         # 正则搜索
rxt find --replace "old" --with "new" --preview     # 替换预览
rxt find --json --max-results 50                    # JSON输出，限50条
rxt find --count                                    # 只输出匹配数
```

#### grep 详解

```bash
rxt grep "pattern"                                  # 当前目录递归
rxt grep "pattern" /path/to/dir                     # 指定目录
rxt grep "pattern" -t rs,py                         # 按文件类型过滤
rxt grep "pattern" -C 5                             # 上下文5行
rxt grep "pattern" --regex                          # 正则模式
rxt grep "pattern" --no-ignore                      # 不忽略.git等
rxt grep "pattern" --jsonl                          # 流式JSON输出
```

#### search（智能搜索）

```bash
rxt search "config.toml"              # 自动判断：像文件名→搜文件名，像代码→搜内容
rxt search "fn main" --content        # 强制搜内容
rxt search "*.rs" --name              # 强制搜文件名
rxt search "TODO" -t rs,py            # 按文件类型过滤
rxt search "bug" --json               # JSON输出
```

---

### 🔹 执行

| 命令 | 作用 | 常用示例 |
|------|------|----------|
| `exec` | 多语言代码/命令执行 | `rxt exec "echo hello"` |
| `build` | Rust智能构建 | `rxt build --release` |
| `check` | Rust代码质量检查 | `rxt check --clippy --fix` |
| `watch` | 文件监听 | `rxt watch "*.rs" "cargo build"` |
| `watch-run` | 文件变化自动重跑 | `rxt watch-run "cargo test" --ext rs` |
| `bench` | 性能基准 | `rxt bench --runs 10 "cmd1" "cmd2"` |
| `recipe` | 命令宏 | `rxt recipe add deploy "./deploy.sh $1"` |

#### exec 详解

```bash
rxt exec "echo hello"                               # shell命令
rxt exec --lang python "print(1+1)"                 # Python代码
rxt exec --lang rust --file main.rs                 # 运行Rust文件
rxt exec --lang sql --db mydb "SELECT 1"            # SQL
rxt exec --container nginx "ls /etc/nginx"          # Docker容器内执行
rxt exec --login "echo $PATH"                       # login shell加载完整环境
rxt exec --json "exit 1"                            # JSON输出(exit_code+stdout+stderr)
rxt exec --host huhu "hostname"                     # SSH远程执行
rxt exec --b64 "base64编码的脚本"                     # base64解码执行
```

**exec 是最高频命令**，几乎所有远程操作都通过它完成。

---

### 🔹 数据处理

| 命令 | 作用 | 示例 |
|------|------|------|
| `jq` | JSON查询/格式化 | `rxt jq ".data[]" file.json` |
| `jsonl` | 解析Codex会话JSONL | `rxt jsonl --last 5 session.jsonl` |
| `sort` | 行排序 | `rxt sort -n -r file.txt` |
| `uniq` | 行去重 | `rxt uniq -c file.txt` |
| `cut` | 列提取 | `rxt cut -f 1,3 -d "," file.csv` |
| `count` | 行/词/字符统计 | `rxt count -l -w file.txt` |

---

### 🔹 Git 操作

```bash
rxt git status                    # 查看改动
rxt git diff                      # 查看diff
rxt git log                       # 最近提交
rxt git branch                    # 列出分支
rxt git add .                     # Stage
rxt git commit -m "msg"           # 提交
rxt git undo                      # 撤销上次commit（保留改动）
rxt git push                      # 推送
rxt git pull                      # 拉取
rxt git fetch                     # fetch不合并
rxt git remote list               # 远程仓库
rxt git remote add origin URL     # 添加远程
```

---

### 🔹 系统管理

| 命令 | 作用 | 示例 |
|------|------|------|
| `sysinfo` | 系统信息 | `rxt sysinfo all` / `rxt sysinfo mem` |
| `ps` | 进程列表/查杀 | `rxt ps --sort cpu --top 10` |
| `service` | Windows服务管理 | `rxt service --name "xray*"` |
| `reg` | 注册表读写 | `rxt reg --get "HKLM\Software\..."` |
| `net` | 网络（TCP/DNS/路由） | `rxt net --port 8080` |
| `info` | rxt自检 | `rxt info --json` |

#### sysinfo 分区

```bash
rxt sysinfo all        # 全部
rxt sysinfo os         # 操作系统
rxt sysinfo cpu        # CPU
rxt sysinfo mem        # 内存
rxt sysinfo disk       # 磁盘
rxt sysinfo net        # 网络
```

#### ps 详解

```bash
rxt ps                              # 默认按内存排序，前20条
rxt ps --sort cpu --top 10          # 按CPU排序，前10条
rxt ps --name "python*"             # 按名称过滤
rxt ps --kill 12345                 # 终止PID
rxt ps --kill "chrome"              # 按名称终止
rxt ps --tree                       # 树形显示
rxt ps --json                       # JSON输出
rxt ps --host huhu --top 5          # 远程查看
```

---

### 🔹 实用工具

| 命令 | 作用 | 示例 |
|------|------|------|
| `http` | HTTP客户端（浏览器 Cookie / 抽正文） | `rxt http GET https://api.example.com --browser chrome` |
| `tail` | 监控文件追加 | `rxt tail -f "ERROR" /var/log/app.log` |
| `unzip` | 解压（zip/tar/tgz） | `rxt unzip archive.zip --to /tmp` |
| `dup` | 找重复文件 | `rxt dup ~/Downloads --ext jpg,png` |
| `trash` | 安全删除（回收站） | `rxt trash file.txt` / `rxt trash --list` |
| `snapshot` | 文件快照+回滚 | `rxt snapshot --label "before-fix"` |
| `repeat` | 轮询重试 | `rxt repeat --port 5432 --timeout 30` |
| `upgrade` | 自我更新 | `rxt upgrade` / `rxt upgrade --check` |
| `map` | 项目结构简报 | `rxt map --depth 2` |

#### http 详解

```bash
rxt http GET https://api.example.com              # GET（默认 Chrome UA，超时生效）
rxt http POST https://api.example.com -d '{"key":"val"}' -j   # POST JSON
rxt http GET https://api.example.com -i           # 显示响应头
rxt http GET https://api.example.com -b           # 只显示body
rxt http GET url --auth user:pass                 # Basic Auth
rxt http GET url -H "Authorization: Bearer token" # 自定义Header
rxt http GET url --timeout 60                     # 超时60秒
rxt http GET url --text --budget 4000             # HTML 抽正文，截断
rxt http GET url --links                          # 列出 href/src
rxt http GET url -o page.html                     # 落盘
rxt http GET url --browser chrome                 # 带上本机 Chrome Cookie（需 --features cookies）
rxt http GET url --cookie-jar cookies.txt         # Netscape 罐（读+写 Set-Cookie）
rxt http GET url --cookie "sid=abc; theme=dark"   # 额外 Cookie
rxt http cookies --browser chrome                 # 按域统计
rxt http cookies --browser chrome github.com      # 导出某域 Cookie（含值）
rxt http cookies --browser firefox github.com -j --cookie-jar cookies.txt
rxt http --host huhu GET http://localhost:8650/sse  # 远程HTTP
```

Chrome 127+ 的 App-Bound Encryption 可能需要**管理员**运行 rxt；失败就换 `--browser firefox` 或浏览器导出 Netscape 到 `--cookie-jar`。Cookie 是登录态，别写进笔记/星枢。

#### tail 详解

```bash
rxt tail /var/log/app.log                         # 看最后10行
rxt tail -l 50 /var/log/app.log                   # 看最后50行
rxt tail -f "ERROR" /var/log/app.log              # 监控并过滤ERROR
rxt tail -n 1000 /var/log/app.log                 # 轮询间隔1秒
rxt tail --once /var/log/app.log                  # 检查一次退出
```

#### repeat 详解

```bash
rxt repeat --port 5432 --timeout 30               # 等端口可连
rxt repeat --file /tmp/ready.txt                  # 等文件出现
rxt repeat --ping 192.168.1.1                     # 等主机ping通
rxt repeat "curl -s http://localhost:8080/health" # 等命令成功
rxt repeat --tries 10 --interval 2000 "cmd"       # 最多10次，间隔2秒
```

---

### 🔹 星枢记忆

```bash
rxt mem save "关键决策：采用HY2多VPS方案"              # 保存记忆
rxt mem search "虎虎的网络架构"                        # 语义搜索
rxt mem stats                                          # 查看统计
```

---

## 三、远程执行模式

### 3.1 SSH 远程执行

```bash
# 在远程机器执行命令
rxt exec --host huhu "hostname && uptime"
rxt exec --host tuanzi "df -h"

# 远程读文件
rxt read --host huhu /etc/hosts

# 远程写文件
rxt write --host tuanzi /tmp/test.txt "hello"

# 远程搜索
rxt grep "TODO" --host huhu -t rs

# 远程Git
rxt git --host huhu status

# 远程系统信息
rxt sysinfo --host huhu mem
```

### 3.2 批量执行（Group）

```bash
# group.all 定义在 hosts.toml 里
rxt exec --group all "uptime"
rxt exec --group all "df -h /"

# 批量读
rxt read --group all /etc/hostname
```

### 3.3 跨机器链式调用

```bash
# 在huhu上执行命令，该命令又调用tuanzi的rxt
rxt exec --host huhu "/usr/local/bin/rxt exec --host tuanzi hostname"

# 跨机器文件传输（先读到本地再写过去）
rxt read --host huhu /tmp/file.txt > /tmp/file.txt
rxt write --host tuanzi /tmp/file.txt --file /tmp/file.txt
```

### 3.4 Docker 容器内执行

```bash
rxt exec --container bitmagnet-postgres "psql -U postgres -d bitmagnet -c 'SELECT count(*) FROM torrents'"
rxt exec --host huhu --container torrent-panel "ls /app"
```

---

## 四、MCP 模式

rxt 可以作为 MCP server 运行，暴露全部命令给 AI Agent：

```bash
# stdio 模式（本地）
rxt mcp

# 被 ZCode 通过 stdio 调用
# 配置示例 (.mcp.json)：
{
  "rxt": {
    "type": "stdio",
    "command": "C:\\rxt\\rxt.exe",
    "args": ["mcp"]
  }
}

# SSE 模式（远程，部署在huhu :8652）
# ZCode 配置：
{
  "rxt": {
    "type": "sse",
    "url": "http://192.168.31.252:8652/sse"
  }
}
```

---

## 五、实用工作流

### 5.1 快速排查远程问题

```bash
# 1. 看系统状态
rxt sysinfo --host huhu all

# 2. 看进程
rxt ps --host huhu --sort cpu --top 10

# 3. 看日志
rxt tail -f "ERROR" --host huhu /var/log/app.log

# 4. 看端口
rxt net --host huhu --conn listen
```

### 5.2 远程代码编辑

```bash
# 1. 先看结构
rxt struct --functions --json --host huhu /home/huhu/app.py

# 2. 读关键部分
rxt read -H 50 --host huhu /home/huhu/app.py

# 3. 搜索目标
rxt grep "def main" --host huhu -t py

# 4. 替换
rxt sed --host huhu /home/huhu/app.py -p "old_func" -r "new_func"

# 5. 验证
rxt exec --host huhu "python3 /home/huhu/app.py"
```

### 5.3 等服务就绪

```bash
# 等端口可用
rxt repeat --port 8080 --timeout 60

# 等文件出现（如编译产物）
rxt repeat --file target/release/rxt --timeout 120

# 等服务健康
rxt repeat "curl -sf http://localhost:8080/health" --timeout 30
```

### 5.4 批量部署

```bash
# 给所有机器发命令
rxt exec --group all "uptime && free -h"

# 批量读配置
rxt read --group all /etc/hostname

# 批量写配置
rxt write --group all /tmp/deploy-marker "deployed-$(date +%s)"
```

### 5.5 安全删除

```bash
rxt trash /tmp/old_file.txt        # 进回收站
rxt trash --list                   # 看回收站
rxt trash --restore "old_file"     # 恢复
rxt trash --clean 30               # 清理30天前的
rxt trash --purge                  # 清空回收站
```

---

## 六、注意事项

1. **路径**：远程路径用 Linux 风格（`/home/user/file`），本地 Windows 路径用反斜杠（`G:\xxx`）
2. **编码**：`read` 自动检测编码并统一为 UTF-8+LF；`write` 自动保持目标文件格式
3. **权限**：远程写 `/usr/local/bin/` 等系统目录需要 sudo，先传到 home 再 `sudo cp`
4. **大文件**：`read` 支持 `--budget` 限制 token 预算，超出自动截断
5. **SSH 密码**：hosts.toml 里明文存储密码，仅供 LAN 内使用
6. **exec 超时**：默认 30s，长命令用 `rxt repeat` 或后台执行
7. **rxt 路径**：Linux 机器上 `rxt` 在 `/usr/local/bin/rxt`，Windows 在 `C:\rxt\rxt.exe`

## 0.8.7 封神升级（2026-07-15）

- **全网**：xian Win + huhu + tuanzi + osaka → 0.8.7；tuntun aarch64 另编
- **0.8.6**：`mem ask`、find 路径/`-name`、远程 find、shell_quote
- **0.8.7**：`deploy` Windows 原生 OpenSSH（不依赖 bash+sshpass）
- 星枢：`rxt mem bootstrap|ask|save`；进仓：`rxt pack -b 5000`
- 部署：在 **对应架构** 机上 `cargo build --release`，再 `rxt deploy <ELF> -t huhu` 或 scp

## 0.9.0（2026-08-20）

- 版本号 0.9.0；`rxt search` 可用
- feature：默认 `http`（ureq）；`--browser` 需 `--features cookies` 或 `--features net`
- grep 去掉双重全文读；find 内容搜索并行
- Cookie 值不进日志；Chrome 127+ 读 Cookie 可能要管理员
