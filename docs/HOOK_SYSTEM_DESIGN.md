# Hook 系统设计文档

## 1. 设计理念

### 1.1 核心定位

Hook 系统是 Rust Agent 的**外部可观测事件系统**。Agent 在执行过程中产生各类事件，外部代码（以插件脚本的形式）可以订阅并响应这些事件，实现不侵入核心代码的扩展能力。

```
Agent 内核事件源
       │
       ▼
  HookBus（事件总线）
       │
  ┌────┴────┐
  │  匹配   │── 按 event name + filter 条件筛选
  └────┬────┘
       │
  ┌────┴────────────────────────────┐
  │  按 priority 排序执行           │
  │  hook1 → hook2 → hook3 ...     │
  └────────────────────────────────┘
       │
  三种执行模式：
  • Fire-and-forget  → 异步不阻塞，用于通知/日志
  • Blocking         → 等待完成，可获取输出，不影响主流程
  • Intercepting     → 可返回 cancel=true 中止当前操作
```

### 1.2 设计原则

1. **零侵入**：主程序只负责在关键点 `emit` 事件，具体行为完全由插件的 `hooks/` 声明
2. **脚本为一等公民**：Hook 脚本与 Plugin Tool 脚本地位相同，sh/py/js 均支持
3. **隔离性**：Hook 脚本的失败不能崩溃主流程，顶多打印警告
4. **可组合**：多个插件可以订阅同一事件，按 priority 顺序执行
5. **超时保护**：所有 Hook 执行均有超时限制，防止阻塞

---

## 2. 事件分类与完整列表

### 2.1 生命周期事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `agent.start` | 进程启动，项目目录和配置已确定 | Blocking |
| `agent.session_start` | 新会话开始（新的 session_id） | Blocking |
| `agent.session_end` | 会话正常结束 | Blocking |
| `plugin.load` | 某个插件加载完成 | Fire-and-forget |
| `plugin.unload` | 某个插件被卸载/禁用 | Fire-and-forget |

### 2.2 Agent 循环事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `turn.start` | 新一轮 LLM 调用开始 | Fire-and-forget |
| `turn.end` | LLM 返回完成 | Fire-and-forget |
| `turn.thinking` | LLM 输出 thinking block（extended thinking） | Fire-and-forget |

### 2.3 工具事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `tool.before` | 工具即将执行 | **Intercepting** |
| `tool.after` | 工具执行完成 | Fire-and-forget |
| `tool.error` | 工具执行失败 | Blocking |
| `tool.cancel` | 工具被用户拒绝（confirm=false） | Fire-and-forget |

### 2.4 上下文 / 内存事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `context.warning` | token 使用超过警告阈值（默认 70%） | Blocking |
| `context.critical` | token 使用超过高危阈值（默认 90%） | Blocking |
| `context.truncate` | 会话历史即将被截断压缩 | Blocking |
| `memory.update` | `.agent/memory.md` 被更新 | Fire-and-forget |

### 2.5 任务 / 规划事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `plan.created` | Agent 输出了一个执行计划 | Fire-and-forget |
| `plan.step_start` | 计划中某步骤开始执行 | Fire-and-forget |
| `plan.step_complete` | 计划中某步骤完成 | Fire-and-forget |
| `plan.complete` | 完整计划执行完毕 | Blocking |
| `plan.failed` | 计划执行失败 | Blocking |

### 2.6 模式 / 状态切换事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `mode.switch` | pipeline 模式切换 | Blocking |
| `skill.load` | `load_skill` 工具被调用 | Fire-and-forget |
| `mcp.connect` | MCP 服务器连接成功 | Fire-and-forget |
| `mcp.disconnect` | MCP 服务器断开连接 | Fire-and-forget |
| `node.connect` | 远程节点连接成功（分布式） | Fire-and-forget |

### 2.7 文件 / 命令事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `file.write` | Agent 写入了文件 | Fire-and-forget |
| `file.read` | Agent 读取了文件 | Fire-and-forget |
| `command.run` | Agent 即将执行 shell 命令 | **Intercepting** |
| `command.complete` | Shell 命令执行完毕 | Fire-and-forget |

### 2.8 错误 / 恢复事件

| 事件名 | 触发时机 | 建议模式 |
|--------|----------|----------|
| `error.llm` | LLM API 调用失败 | Blocking |
| `error.rate_limit` | API 限流触发 | Blocking |

---

## 3. Hook 定义格式

### 3.1 文件位置

每个插件的 `hooks/` 目录下，每个 `.toml` 文件定义一个 hook：

```
my-plugin/
├── plugin.toml
└── hooks/
    ├── auto_summarize.toml      # 上下文不足时自动压缩
    ├── notify_plan_done.toml    # 计划完成后发通知
    ├── block_dangerous_cmd.toml # 拦截危险命令
    └── scripts/
        ├── summarize.sh
        ├── notify.sh
        └── check_command.sh
```

### 3.2 Hook TOML 格式

```toml
# hooks/auto_summarize.toml

# 必需：订阅的事件名
event = "context.warning"

# 必需：描述（用于日志和 /plugin hooks 命令展示）
description = "When context is 80% full, compress old messages"

# 必需：执行脚本（相对于插件根目录）
script = "hooks/scripts/summarize.sh"

# 必需：执行模式
# fire_and_forget | blocking | intercepting
mode = "blocking"

# 可选：过滤条件，所有字段与 payload.data 中的字段匹配
# 不满足条件的事件直接跳过此 hook
[filter]
min_ratio = 0.8          # payload.data.ratio >= 0.8 才触发

# 可选：注入给脚本的额外环境变量
[env]
SUMMARY_MAX_TOKENS = "500"
SUMMARY_KEEP_RECENT = "10"

# 可选：超时秒数（默认 30）
timeout = 60

# 可选：同一事件多个 hook 时的执行顺序（小的先执行，默认 100）
priority = 10
```

更多示例：

```toml
# hooks/notify_plan_done.toml
event = "plan.complete"
description = "Send desktop notification when a plan finishes"
script = "hooks/scripts/notify.sh"
mode = "fire_and_forget"
priority = 100
```

```toml
# hooks/block_dangerous_cmd.toml
event = "tool.before"
description = "Block rm -rf and other destructive commands"
script = "hooks/scripts/check_command.sh"
mode = "intercepting"
timeout = 5

[filter]
tool_name = "run_command"   # 只拦截这个工具的调用
```

```toml
# hooks/on_load_setup.toml
event = "plugin.load"
description = "Run environment setup when this plugin loads"
script = "hooks/scripts/setup.sh"
mode = "blocking"
timeout = 120

[filter]
plugin_id = "my-plugin@1.0.0"   # 只响应本插件自身加载事件
```

---

## 4. 事件 Payload 规范

所有事件通过 `AGENT_EVENT` 环境变量将 JSON payload 注入脚本。

### 4.1 通用结构

```json
{
  "event": "事件名",
  "timestamp": "2026-03-30T10:00:00Z",
  "session_id": "abc123",
  "data": { /* 事件特定数据 */ }
}
```

### 4.2 各事件的 data 字段

**`agent.start`**
```json
{
  "project_dir": "/path/to/project",
  "config_model": "claude-opus-4-5",
  "mode": "cli"
}
```

**`agent.session_end`**
```json
{
  "session_id": "abc123",
  "total_turns": 12,
  "total_tokens_in": 45000,
  "total_tokens_out": 8000,
  "duration_seconds": 180
}
```

**`turn.end`**
```json
{
  "turn": 5,
  "tokens_in": 8500,
  "tokens_out": 1200,
  "tools_called": ["read_file", "edit_file"],
  "duration_ms": 2300
}
```

**`tool.before`**
```json
{
  "tool_name": "run_command",
  "plugin_id": null,
  "params": { "command": "rm -rf /tmp/test" },
  "turn": 5
}
```

**`tool.after`**
```json
{
  "tool_name": "run_command",
  "plugin_id": null,
  "success": true,
  "duration_ms": 150,
  "output_preview": "Removed /tmp/test"
}
```

**`context.warning`**
```json
{
  "used_tokens": 85000,
  "max_tokens": 100000,
  "ratio": 0.85,
  "model": "claude-opus-4-5"
}
```

**`context.truncate`**
```json
{
  "messages_before": 45,
  "messages_after": 12,
  "tokens_before": 95000,
  "tokens_after": 40000
}
```

**`plan.complete`**
```json
{
  "total_steps": 8,
  "completed_steps": 8,
  "failed_steps": 0,
  "duration_seconds": 120,
  "tokens_used": 45000
}
```

**`mode.switch`**
```json
{
  "from": "default",
  "to": "pipeline",
  "reason": "user_command"
}
```

**`file.write`**
```json
{
  "path": "/path/to/file.rs",
  "bytes_written": 2048,
  "tool_name": "edit_file"
}
```

**`error.rate_limit`**
```json
{
  "provider": "anthropic",
  "retry_after_seconds": 60,
  "retry_count": 2
}
```

**`plugin.load`**
```json
{
  "plugin_id": "git-tools@1.0.0",
  "plugin_name": "git-tools",
  "scope": "project",
  "tools_count": 5,
  "skills_count": 3
}
```

---

## 5. Intercepting 模式协议

`intercepting` 模式的脚本通过 **stdout** 返回 JSON 控制主流程。

### 5.1 允许继续
```json
{ "cancel": false }
```
或脚本不输出任何内容（默认 allow）。

### 5.2 取消操作
```json
{
  "cancel": true,
  "reason": "Dangerous command detected: rm -rf /"
}
```
返回值中的 `reason` 会作为工具错误信息展示给 LLM，让 LLM 可以理解被拒绝的原因并作出调整。

### 5.3 修改参数（高级）
```json
{
  "cancel": false,
  "patch_params": {
    "command": "rm -rf /tmp/test_safe"
  }
}
```
`patch_params` 中的字段会覆盖原始 params，允许 hook 对工具参数进行安全净化。

### 5.4 多 hook 串联规则
- 同一事件有多个 `intercepting` hook 时，按 priority 顺序执行
- 任意一个返回 `cancel: true`，**立即中止**，后续 hook 不再执行
- 全部通过后才执行实际操作

---

## 6. Rust 实现架构

### 6.1 文件结构

```
src/plugin/
├── hook_loader.rs   # 扫描 hooks/*.toml，解析 HookDefinition
└── hook_bus.rs      # 事件发布、匹配、脚本调度

src/agent.rs         # 在关键点调用 hook_bus.emit*()
src/tools/mod.rs     # tool.before / tool.after 触发点
```

### 6.2 核心类型

```rust
// hook_loader.rs

/// 单个 hook 的定义（来自 hooks/*.toml）
#[derive(Debug, Clone)]
pub struct HookDefinition {
    pub event: String,
    pub description: String,
    pub script_path: PathBuf,           // 绝对路径
    pub mode: HookMode,
    pub filter: Option<serde_json::Value>,
    pub env: HashMap<String, String>,
    pub timeout_secs: u64,              // 默认 30
    pub priority: i32,                  // 默认 100，小的先执行
    pub plugin_id: String,
}

#[derive(Debug, Clone)]
pub enum HookMode {
    FireAndForget,
    Blocking,
    Intercepting,
}

/// 从插件目录扫描所有 hook 定义
pub fn load_hooks_from_plugin(
    plugin_id: &str,
    plugin_path: &Path,
) -> Vec<HookDefinition>;
```

```rust
// hook_bus.rs

/// 运行时事件总线
pub struct HookBus {
    /// event_name → 按 priority 排序的 hook 列表
    hooks: HashMap<String, Vec<HookDefinition>>,
}

/// 发布的事件
pub struct HookEvent {
    pub name: String,
    pub session_id: String,
    pub data: serde_json::Value,
}

/// Intercepting 模式的执行结果
pub enum HookResult {
    Continue,
    Cancel { reason: String },
    PatchParams { params: serde_json::Value },
}

impl HookBus {
    /// 注册插件的所有 hook（插件加载时调用）
    pub fn register_plugin_hooks(&mut self, plugin_id: &str, plugin_path: &Path);

    /// 注销插件的所有 hook（插件卸载时调用）
    pub fn unregister_plugin_hooks(&mut self, plugin_id: &str);

    /// Fire-and-forget：spawn 异步任务，不等结果
    pub fn emit(&self, event: HookEvent);

    /// Blocking：等待所有匹配 hook 按序完成
    pub async fn emit_blocking(&self, event: HookEvent);

    /// Intercepting：顺序执行，任意 hook cancel 则中止
    pub async fn emit_intercepting(
        &self,
        event: HookEvent,
    ) -> HookResult;

    /// 内部：filter 匹配检查
    fn matches_filter(hook: &HookDefinition, data: &serde_json::Value) -> bool;

    /// 内部：执行单个脚本
    async fn run_script(
        hook: &HookDefinition,
        payload: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>>;
}
```

### 6.3 Agent 内核集成点

```rust
// agent.rs - 关键触发点

// ── 生命周期 ──────────────────────────────────────────────────────
// 在 new() 完成后
hook_bus.emit_blocking(HookEvent::new("agent.start", json!({
    "project_dir": project_dir,
    "config_model": config.model,
}))).await;

// ── 轮次 ──────────────────────────────────────────────────────────
// 每轮 LLM 调用前
hook_bus.emit(HookEvent::new("turn.start", json!({ "turn": turn_count })));

// 每轮 LLM 返回后
hook_bus.emit(HookEvent::new("turn.end", json!({
    "turn": turn_count,
    "tokens_in": usage.input,
    "tokens_out": usage.output,
})));

// ── 上下文检查（在 context::check_context 之后）────────────────────
if ratio > 0.7 {
    hook_bus.emit_blocking(HookEvent::new("context.warning", json!({
        "used_tokens": used,
        "max_tokens": max,
        "ratio": ratio,
        "model": config.model,
    }))).await;
}
if ratio > 0.9 {
    hook_bus.emit_blocking(HookEvent::new("context.critical", json!({
        "used_tokens": used,
        "max_tokens": max,
        "ratio": ratio,
    }))).await;
}
```

```rust
// tools/mod.rs - 工具拦截点

pub async fn execute(&mut self, name: &str, params: Value) -> ToolResult {
    // tool.before（intercepting）
    let event = HookEvent::new("tool.before", json!({
        "tool_name": name,
        "params": params,
        "turn": self.turn_count,
    }));
    match hook_bus.emit_intercepting(event).await {
        HookResult::Cancel { reason } => {
            return ToolResult::Error(format!("Hook blocked tool: {}", reason));
        }
        HookResult::PatchParams { params: new_params } => {
            params = new_params;  // 使用净化后的参数
        }
        HookResult::Continue => {}
    }

    // 实际执行工具
    let start = Instant::now();
    let result = self.tools[name].execute(&params).await;
    let duration = start.elapsed();

    // tool.after（fire-and-forget）
    hook_bus.emit(HookEvent::new("tool.after", json!({
        "tool_name": name,
        "success": result.is_ok(),
        "duration_ms": duration.as_millis(),
    })));

    result
}
```

### 6.4 HookBus 与 PluginManager 的关系

```
PluginManager
├── load_plugin_at_path()
│   ├── tool_loader.load_tools_from_plugin(...)
│   ├── skill_loader.load_skills_from_plugin(...)
│   └── hook_bus.register_plugin_hooks(...)   ← 新增
│
└── remove_plugin_internal()
    ├── tool_loader.unload_plugin_tools(...)
    ├── skill_loader.unload_plugin_skills(...)
    └── hook_bus.unregister_plugin_hooks(...) ← 新增
```

`HookBus` 通过 `Arc<Mutex<HookBus>>` 注入 `PluginManager`，与 `ToolLoader`/`SkillLoader` 平级。`Agent` 也持有同一个 `Arc` 来 emit 事件。

---

## 7. Filter 规范

Filter 字段与 `event.data` 中的字段做简单相等或比较匹配：

```toml
[filter]
# 字符串精确匹配
tool_name = "run_command"

# 数值下限（>= 比较）
min_ratio = 0.8

# 数值上限（<= 比较）
max_turn = 50

# 字符串前缀匹配
path_prefix = "/src/"

# 插件 ID 精确匹配
plugin_id = "my-plugin@1.0.0"
```

Filter 规则：
- 所有字段都满足才触发（AND 语义）
- `min_*` 前缀 → `data[field] >= value`
- `max_*` 前缀 → `data[field] <= value`
- `*_prefix` 后缀 → `data[field].starts_with(value)`
- 其他 → 精确相等

---

## 8. 脚本编写指南

### 8.1 访问事件数据

```bash
#!/bin/bash
# hooks/scripts/notify.sh

# 完整 payload JSON
EVENT_JSON="$AGENT_EVENT"

# 用 jq 提取字段
STEPS=$(echo "$EVENT_JSON" | jq -r '.data.completed_steps')
DURATION=$(echo "$EVENT_JSON" | jq -r '.data.duration_seconds')
SESSION=$(echo "$EVENT_JSON" | jq -r '.session_id')

notify-send "Agent 完成" "执行了 $STEPS 步，用时 ${DURATION}s (session: $SESSION)"
```

```python
#!/usr/bin/env python3
# hooks/scripts/check_command.py

import json, os, sys

payload = json.loads(os.environ["AGENT_EVENT"])
command = payload["data"]["params"].get("command", "")

# 检测危险命令
BLOCKLIST = ["rm -rf /", "dd if=", "mkfs", "> /dev/sda"]
for pattern in BLOCKLIST:
    if pattern in command:
        # intercepting 模式：输出 JSON 到 stdout
        print(json.dumps({
            "cancel": True,
            "reason": f"Blocked dangerous command pattern: '{pattern}'"
        }))
        sys.exit(0)

# 允许执行
print(json.dumps({"cancel": False}))
```

```bash
#!/bin/bash
# hooks/scripts/summarize.sh

# 插件自定义环境变量（来自 hook.toml [env]）
MAX_TOKENS="${SUMMARY_MAX_TOKENS:-500}"
KEEP_RECENT="${SUMMARY_KEEP_RECENT:-10}"

# 执行压缩逻辑（调用 API 或本地脚本）
python3 "$(dirname "$0")/compress_history.py" \
    --max-tokens "$MAX_TOKENS" \
    --keep-recent "$KEEP_RECENT"
```

### 8.2 退出码语义

| 退出码 | 含义 |
|--------|------|
| `0` | 执行成功 |
| `1` | 执行失败（打印警告，不中止主流程） |
| 任意非零 | 视为失败，记录日志 |

Intercepting 模式下，退出码非零且无有效 stdout JSON 时，默认视为 `{ "cancel": false }`（安全降级，不阻塞主流程）。

---

## 9. 调试与可观测性

### 9.1 verbose 模式

启动时加 `--verbose`，Hook 触发和执行结果会打印到 stderr：

```
[hook] event=context.warning  matched=2 hooks
[hook] → auto_summarize (blocking, priority=10) ... ok (1.2s)
[hook] → log_context (fire_and_forget, priority=100) spawned
[hook] event=tool.before tool=run_command
[hook] → block_dangerous_cmd (intercepting, priority=5) ... CANCEL: Dangerous command
```

### 9.2 `/plugin hooks` 命令

CLI/TUI 中新增交互命令：

```
/plugin hooks                     # 列出所有已注册的 hook
/plugin hooks context.warning     # 列出某事件的所有 hook
/plugin hooks --test agent.start  # 触发测试事件（dry-run）
```

### 9.3 Server 模式 WebSocket 事件

Hook 执行情况通过 WS 事件推送给前端：

```json
{
  "type": "hook_triggered",
  "event": "context.warning",
  "hook": "auto_summarize",
  "plugin": "context-manager@1.0.0",
  "mode": "blocking",
  "result": "ok",
  "duration_ms": 1200
}

{
  "type": "hook_intercepted",
  "event": "tool.before",
  "hook": "block_dangerous_cmd",
  "plugin": "safety-guard@1.0.0",
  "reason": "Dangerous command detected"
}
```

---

## 10. 场景示例

### 10.1 自动上下文压缩插件

```
context-manager/
├── plugin.toml
└── hooks/
    ├── warn_at_70.toml
    ├── compress_at_85.toml
    ├── emergency_at_95.toml
    └── scripts/
        ├── warn.sh
        ├── compress.py
        └── emergency_save.sh
```

```toml
# hooks/compress_at_85.toml
event = "context.warning"
description = "Auto-compress conversation history at 85% context usage"
script = "hooks/scripts/compress.py"
mode = "blocking"
timeout = 60
[filter]
min_ratio = 0.85
```

### 10.2 命令安全审计插件

```toml
# hooks/audit_commands.toml
event = "command.run"
description = "Log all shell commands to audit file"
script = "hooks/scripts/audit.sh"
mode = "fire_and_forget"

# hooks/block_destructive.toml
event = "tool.before"
description = "Block destructive file operations"
script = "hooks/scripts/safety_check.py"
mode = "intercepting"
priority = 1
timeout = 5
[filter]
tool_name = "run_command"
```

### 10.3 计划完成通知插件

```toml
# hooks/notify_slack.toml
event = "plan.complete"
description = "Send Slack notification when plan completes"
script = "hooks/scripts/slack_notify.sh"
mode = "fire_and_forget"
[env]
SLACK_WEBHOOK = "https://hooks.slack.com/..."
```

### 10.4 环境初始化插件

```toml
# hooks/on_load.toml
event = "plugin.load"
description = "Setup environment when this plugin loads"
script = "hooks/scripts/setup.sh"
mode = "blocking"
timeout = 120
[filter]
plugin_id = "docker-tools@2.0.0"
[env]
DOCKER_BIN = "/usr/bin/docker"
```

### 10.5 Token 不足自动保存 checkpoint

```toml
# hooks/checkpoint_on_critical.toml
event = "context.critical"
description = "Save session checkpoint when context is near limit"
script = "hooks/scripts/save_checkpoint.sh"
mode = "blocking"
priority = 1  # 最先执行
timeout = 30
```

---

## 11. 实施路线图

### Phase 1 — 核心骨架（约 1 周）

- [ ] `hook_loader.rs`：`HookDefinition` 结构、扫描插件 `hooks/*.toml`
- [ ] `hook_bus.rs`：`HookBus`、`emit`（fire-and-forget）、`emit_blocking`
- [ ] `PluginManager` 集成：`load_plugin_at_path` 注册 hooks，`remove_plugin_internal` 注销
- [ ] 接入 3 个高价值事件：`agent.start`、`context.warning`、`plan.complete`
- [ ] Filter 基本匹配（精确相等 + `min_*`）

### Phase 2 — 工具拦截（约 1 周）

- [ ] `emit_intercepting` 实现
- [ ] `tool.before` / `tool.after` 集成到 `ToolExecutor`
- [ ] `patch_params` 支持
- [ ] 超时保护（tokio timeout）
- [ ] Hook 执行失败的降级处理（打印 warning，继续主流程）

### Phase 3 — 完整事件覆盖（持续）

- [ ] 补全所有事件触发点
- [ ] `--verbose` 模式的 hook 日志输出
- [ ] `/plugin hooks` 交互命令
- [ ] Server 模式的 `hook_triggered` / `hook_intercepted` WS 事件推送
- [ ] `*_prefix` 等扩展 filter 规则

---

## 12. 注意事项

1. **Hook 脚本绝不能崩溃主流程**：所有执行均在 `catch_unwind` / `Result` 包裹内，失败只打印 warning
2. **Intercepting 超时也默认 allow**：超时视为 `{ "cancel": false }`，防止 hook 卡住 agent
3. **避免 hook 中调用工具**：hook 脚本是独立进程，不在 agent 的 LLM 上下文中，不应该与 agent 通信（会造成递归）
4. **Fire-and-forget 不保证执行顺序**：同一事件的多个 fire-and-forget hook 是并发执行的
5. **Blocking hook 串行执行**：按 priority 顺序一个接一个，总耗时是所有 hook 耗时之和，设置合理 timeout

---

**设计版本**: v1.0  
**设计日期**: 2026年3月30日  
**状态**: 待实现（Phase 1 优先）
