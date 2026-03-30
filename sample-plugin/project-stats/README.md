# project-stats 插件

项目统计分析插件，同时作为「Rust Agent 插件开发范例」，
完整覆盖**工具 (Tools)**、**技能 (Skills)**、**Hook** 三类组件。

## 安装

```bash
cp -r sample/project-stats .agent/plugins/project-stats
```

重启 Agent 或执行 `/plugins reload` 即可生效。

## 目录结构

```
project-stats/
├── plugin.toml                  # 插件元数据（必须）
├── README.md                    # 本文件
│
├── tools/                       # 工具定义
│   ├── git_log.json             # 工具参数 Schema（JSON Schema）
│   ├── git_log.sh               # 工具实现脚本
│   ├── word_count.json
│   └── word_count.sh
│
├── skills/                      # 技能文档（注入 LLM system prompt）
│   └── git-workflow.md
│
└── hooks/                       # Hook 定义 + 脚本
    ├── on_agent_start.toml      # 事件: agent.start   模式: fire_and_forget
    ├── on_tool_after.toml       # 事件: tool.after    模式: fire_and_forget
    ├── on_router_decision.toml  # 事件: router.decision 模式: intercepting
    └── scripts/
        ├── on_agent_start.sh
        ├── on_tool_after.sh
        └── on_router_decision.sh
```

## 提供的工具

### `git_log`
查询 Git 提交历史，返回结构化 JSON。

| 参数 | 类型 | 说明 |
|------|------|------|
| `limit` | int | 返回条数，默认 20 |
| `author` | string | 按作者过滤（可选） |
| `path` | string | 按文件路径过滤（可选） |
| `since` | string | 起始日期 YYYY-MM-DD（可选） |

### `word_count`
统计代码规模（行数/单词数/字符数）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `path` | string | 文件或目录（必填） |
| `ext` | string | 只统计指定扩展名（可选） |
| `exclude` | string | 排除路径模式（可选） |

## Hook 说明

### `on_agent_start` — fire_and_forget
每次 Agent 启动时写入会话日志（`/tmp/project-stats.log`）。

不干预任何流程，适合监控、指标上报等场景。

### `on_tool_after` — fire_and_forget
每次工具执行后写入审计日志（`/tmp/project-stats-audit.log`）。

可修改为发送 Webhook、写数据库等。

### `on_router_decision` — intercepting
检测任务描述中的高风险关键词（`生产`、`deploy`、`rm -rf` 等），
自动将执行模式升级为 `full_pipeline`，确保有 Checker 验证。

**脚本返回协议：**

```bash
# 不干预（接受内置决策）
exit 0

# 覆盖为指定模式
echo '{"override_mode": "full_pipeline"}'
exit 0

# 阻止执行
echo '{"cancel": "原因说明"}'
exit 0
```

**可用 override_mode 值：**
- `basic_loop` — 单模型直接回复
- `plan_and_execute` — 两阶段（计划+执行）
- `full_pipeline` — 三阶段（计划+执行+校验）

## 开发规范

### 工具脚本约定
- 参数以 `--key value` 方式接收
- stdout **必须**输出合法 JSON
- 错误时输出 `{"error": "msg"}` 并 `exit 0`（不要 exit 非零）

### Hook 脚本约定
- 事件 payload 通过环境变量 `AGENT_EVENT` 传入（JSON 字符串）
- `[env]` 节中的键值对也会注入为环境变量
- 超时后脚本会被强制终止，等同于 Continue
- intercepting 模式：stdout 输出 JSON 对象才会被处理，空输出 = Continue
