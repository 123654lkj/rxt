# rxt mem（0.10.2）

标准输出默认 UTF-8 无 BOM；Windows 标准库插件直接启动 rxt-tools.exe，避免 cmd.exe 拒绝多行参数。升级时必须同时更新 rxt 和 rxt-tools。已安装的标准 .cmd 启动器会被自动识别，自定义启动器仍按原样执行。

| CLI | HTTP |
|---|---|
| `mem save <正文>` / `mem save --file <UTF-8文件>` / `mem save --stdin` | POST /memory/add |
| `mem search <查询> --json`（别名 ask） | POST /ask，完整响应 JSON |
| `mem supersede <旧ID> --by <新ID> --reason <原因>` | POST /memory/supersede，old_id/new_id/note |
| `mem stats --json` | GET /v5/health 与 /stats，合为 `{ "health": ..., "stats": ... }` |
| `mem bootstrap <主题> --json` | POST /v5/bootstrap |
| `mem extract <文本> --dry-run` | POST /v5/session-extract |
| `mem layers <主题>` | GET /v5/layered-recall |

`save` 的正文、`--file`、`--stdin` 三选一；文件/stdin 保留换行，接受 UTF-8 BOM。分类默认为 fact，importance 默认为 0.6，范围 0–1。

`supersede` 不传 `--by` 时只废止旧记忆；传入时建立替代关系。HTTP 200 中的 `ok:false` 或 error 仍返回失败。此版本未提供物理删除命令。

所有 mem 操作接受 `--timeout <毫秒>`，必须为正整数。显式预算覆盖请求与响应读取，stats 两次请求共用预算；超时 stderr 含 `MEM_TIMEOUT`，退出码 124。显式设置预算时关闭 SSH 降级，避免第二次尝试突破预算。不设置时保留原有 HTTP 90 秒超时和可选 SSH 降级。

退出码：0 成功，1 请求或业务失败，2 参数错误，124 超时。标准库宿主保留插件退出码。

文本搜索保留 contract 裁决与 pack 证据，不再重复打印 hits 正文；缺少 contract/pack 时回退显示 hits。JSON 模式保留服务端全部字段。stats 任一接口失败都会报错，不返回伪装完整的结果。

```powershell
rxt mem save "第一行`n第二行" --category lesson
rxt mem save --file .\lesson.txt --category lesson
Get-Content -Raw .\lesson.txt | rxt mem save --stdin
rxt mem search "traceId 覆盖 判定" --json --timeout 12000
rxt mem supersede 2385 --by 2386 --reason "新记录已纠正旧记录"
rxt mem stats --json
```

`memory_layers`、category/trust 默认值、写入去重阈值及召回筛选均由星枢服务端决定；CLI 不回填、不伪造统计，也不合并近似记录。`is_duplicate`/`duplicate_of`/`similarity_score` 按原响应保留；是否写入与是否进入 top-k 是两个不同判断。具体阈值以目标服务端版本为准。

回归：`python3 tests/mem_cli.py <rxt或rxt-tools路径>` 使用临时 HTTP 服务，覆盖多行、特殊字符、文件/stdin、无 BOM、JSON、supersede 错误与超时，不写真实记忆。
