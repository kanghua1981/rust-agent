Now I have a thorough understanding of the entire WebSocket protocol. Here's the document:

---

## Rust Agent WebSocket 接口文档

### 1. 连接建立

#### 1.1 服务器启动

```bash
./target/release/agent --mode server [--port 9527] [--host 127.0.0.1]
```

服务器支持**多路径路由**：

| 路径 | 用途 | 类型 |
|------|------|------|
| `/agent` 或 `/` | LLM 对话会话（fork worker 子进程） | WebSocket |
| `/probe` | 获取节点能力快照（不 fork，服务器内联处理） | WebSocket |
| `/nodes` | 获取所有已知节点列表 | HTTP GET |
| `/reprobe?peer=<name>` | 按需重新探测指定 peer | HTTP GET |

#### 1.2 WebSocket URL 参数

连接 `/agent` 时支持以下 query 参数：

| 参数 | 说明 | 示例 |
|------|------|------|
| `workdir` | 指定工作目录 (URL 编码) | `?workdir=%2Fhome%2Fuser%2Fmyproject` |
| `mode` | 隔离模式: `normal` / `container` / `sandbox` | `?mode=sandbox` |
| `sandbox` | 旧版兼容: `1` → Sandbox, `0` → Container | `?sandbox=1` |
| `token` | 集群认证 token | `?token=my-secret` |

**完整 URL 示例**：
```
ws://127.0.0.1:9527/agent?workdir=%2Fhome%2Fuser%2Fmyproject&mode=sandbox&token=xxx
```

---

### 2. 消息信封格式

所有 WebSocket 消息均为 **JSON 文本帧**，统一信封结构：

```json
{
  "type": "<message_type>",
  "seq": 0,
  "data": { ... },
  "id": "<optional_correlation_id>"
}
```

- `type`: 字符串，标识消息类型
- `seq`: 单调递增的事件序号，从 0 开始；用于断线重连时的 gap 检测和恢复
- `data`: 对象，携带具体负载
- `id`: 可选的相关 ID，用于关联请求和响应（如 `tool_use` ↔ `tool_result`）；放在信封层而非 `data` 内部，方便客户端无需深入解析即可匹配

---

### 3. 服务器 → 客户端 事件 (Server Events)

#### 3.1 生命周期事件

| type | data | 说明 |
|------|------|------|
| `ready` | `{ version, workdir, isolation, sandbox, sandbox_backend, caps, virtual_nodes }` | 连接建立后首个事件，包含节点能力信息 |
| `done` | `{ text, id?, pending_changes }` | 一轮对话完成，`text` 是最终回复文本 |
| `cancelled` | `{ message }` | 任务被中断取消 |
| `error` | `{ message }` | 致命/一般错误 |
| `warning` | `{ message }` | 非致命警告 |

#### 3.2 流式文本事件

| type | data | 说明 |
|------|------|------|
| `stream_start` | `{}` | 流式响应开始 |
| `streaming_token` | `{ token }` | 单个流式 token (逐字/逐词) |
| `stream_end` | `{}` | 流式响应结束 |
| `assistant_text` | `{ text }` | 完整的助手文本块 (非流式) |

#### 3.3 思考/推理事件 (Thinking)

| type | data | 说明 |
|------|------|------|
| `thinking_start` | `{}` | 模型开始推理 |
| `thinking_token` | `{ token }` | 推理过程的单个 token |
| `thinking_end` | `{}` | 推理结束 |

#### 3.4 工具调用事件

| type | data | 说明 |
|------|------|------|
| `tool_use` | `{ tool, input }` | 即将调用工具。`tool_id` 放在信封 `id` 字段 |
| `tool_result` | `{ tool, output, is_error }` | 工具执行结果 |

#### 3.5 文件/Diff 事件

| type | data | 说明 |
|------|------|------|
| `file` | `{ path }` | 文件被创建或修改 (通知客户端可拉取) |
| `diff` | `{ path, diff }` | 文件修改的 unified diff 文本 |

#### 3.6 确认请求事件

| type | data | 说明 |
|------|------|------|
| `confirm_request` | `{ action, path?, lines?, command?, preview?, tool_id? }` | 请求客户端确认危险操作 |
| `ask_user` | `{ question }` | 请求用户自由文本回答 |
| `review_plan` | `{ plan }` | 请求审查执行计划 |
| `guidance_request` | `{}` | 请求在流水线中注入人工指导 |

#### 3.7 流水线/多角色事件

| type | data | 说明 |
|------|------|------|
| `role_header` | `{ label, model }` | 标识当前响应的角色 (如 `🤖 Agent`, `🧠 Planner`) |
| `stage_end` | `{ label }` | 流水线阶段结束 |

#### 3.8 沙盒状态事件

| type | data | 说明 |
|------|------|------|
| `sandbox_status` | `{ enabled, backend, pending_changes }` | 沙盒状态更新 |
| `sandbox_changes_result` | `{ files[], backend, pending_changes }` | `sandbox_list_changes` 响应 |
| `sandbox_commit_result` | `{ modified, created }` | `sandbox_commit` 响应 |
| `sandbox_commit_file_result` | `{ file_path, modified, created }` | `sandbox_commit_file` 响应 |
| `sandbox_rollback_result` | `{ restored, deleted, errors[] }` | `sandbox_rollback` 响应 |

#### 3.9 会话管理事件

| type | data | 说明 |
|------|------|------|
| `session_info` | `{ exists, message_count?, updated_at?, summary?, working_dir? }` | 当前会话摘要 |
| `session_restored` | `{ message_count, messages[] }` | 会话恢复成功 |
| `session_cleared` | `{ message }` | 新会话已创建 |
| `sessions_list` | `{ sessions[] }` | `list_sessions` 响应 |
| `session_deleted` | `{ id }` | `delete_session` 响应 |

#### 3.10 插件/MCP 事件

| type | data | 说明 |
|------|------|------|
| `plugins_list` | `{ plugins[] }` | 插件列表 (含 enabled 状态) |
| `plugin_status_changed` | `{ id, action, note }` | 插件启用/禁用结果 |
| `mcp_loaded` | `{ tools[], errors[] }` | MCP 工具加载结果 |
| `mcp_unloaded` | `{ prefix, removed[] }` | MCP 卸载结果 |
| `mcp_tools_list` | `{ tools[] }` | 当前 MCP 工具列表 |

#### 3.11 上传文件事件

| type | data | 说明 |
|------|------|------|
| `upload_file_result` | `{ success, name, path?, size?, error? }` | 文件上传结果 |

#### 3.12 子 Agent / 嵌套 Agent 事件

子 Agent 的输出通过统一的 `agent_event` 包装类型传递，内层复用标准事件结构：

| type | data | 说明 |
|------|------|------|
| `agent_event` | `{ agent_id, parent_id?, event: { type, data } }` | 包装子 Agent 事件的统一类型，内层 `event` 是标准事件 (如 `streaming_token`, `tool_use`, `done` 等) |

**示例**：
```json
// 子 agent 输出流式 token
{
  "type": "agent_event",
  "seq": 42,
  "data": {
    "agent_id": "abc123",
    "event": {
      "type": "streaming_token",
      "data": { "token": "你" }
    }
  }
}

// 子 agent 调用工具
{
  "type": "agent_event",
  "seq": 45,
  "data": {
    "agent_id": "abc123",
    "event": {
      "type": "tool_use",
      "data": { "tool": "read_file", "input": { "path": "src/main.rs" } }
    }
  }
}

// 子 agent 完成
{
  "type": "agent_event",
  "seq": 50,
  "data": {
    "agent_id": "abc123",
    "event": {
      "type": "done",
      "data": { "text": "任务完成" }
    }
  }
}
```

- `agent_id`: 子 Agent 的唯一标识（如 task_id 前 8 位）
- `parent_id`: 预留字段，用于未来嵌套子-子 Agent 场景
- `event.type`: 内层事件类型，复用标准 Server Event 类型名
- `event.data`: 内层事件负载，结构与对应的标准 Server Event 一致

#### 3.13 其他事件

| type | data | 说明 |
|------|------|------|
| `context_warning` | `{ usage_percent, estimated_tokens, max_tokens }` | 上下文窗口压力告警 |
| `service_notification` | `{ source, level, message }` | 外部服务推送通知 |
| `pong` | `{}` | 心跳响应 |

---

### 4. 客户端 → 服务器 消息 (Client Messages)

#### 4.1 核心消息

| type | data | 说明 |
|------|------|------|
| `user_message` | `{ text, workdir?, model? }` | 发送用户消息，触发 agent 处理 |
| `cancel` | `{}` | 中断正在执行的任务 |
| `resume` | `{ from_seq }` | 断线重连恢复：请求服务器从指定 seq 重传事件 |

#### 4.2 确认响应

| type | data | 说明 |
|------|------|------|
| `confirm_response` | `{ approved, clarify? }` | 对 `confirm_request` 的响应 |
| `ask_user_response` | `{ answer }` | 对 `ask_user` 的响应 |
| `review_plan_response` | `{ approved, feedback? }` | 对 `review_plan` 的响应 |

#### 4.3 设置类

| type | data | 说明 |
|------|------|------|
| `set_workdir` | `{ workdir }` | 切换工作目录 |
| `set_mode` | `{ mode }` | 设置执行模式: `simple` / `plan` / `pipeline` / `auto` |
| `set_model` | ... | 切换模型 (仅通知，实际切换由连接时 URL 控制) |
| `set_sandbox` | `{ enabled }` | 切换沙盒 (容器模式下仅报告状态) |

#### 4.4 会话管理

| type | data | 说明 |
|------|------|------|
| `load_session` | `{}` | 加载当前工作目录的持久会话 |
| `new_session` | `{}` | 新建空会话 |
| `load_session_by_id` | `{ id }` | 按 ID 加载指定会话 |
| `list_sessions` | `{}` | 列出所有已保存会话 |
| `delete_session` | `{ id }` | 删除指定会话 |

#### 4.5 沙盒操作

| type | data | 说明 |
|------|------|------|
| `sandbox_list_changes` | `{}` | 列出覆盖层的所有变更文件 |
| `sandbox_commit` | `{}` | 提交所有变更到真实目录 |
| `sandbox_commit_file` | `{ file_path }` | 提交单个文件 |
| `sandbox_rollback` | `{}` | 回滚所有变更 |

#### 4.6 MCP 动态加载

| type | data | 说明 |
|------|------|------|
| `load_mcp` | `{ servers: [{ name, command?, args?, env?, url?, headers? }] }` | 动态加载 MCP 服务器 |
| `unload_mcp` | `{ prefix }` | 卸载指定前缀的 MCP 工具 |
| `list_mcp_tools` | `{}` | 列出已加载 MCP 工具 |

#### 4.7 插件管理 (仅内存, 不回写磁盘)

| type | data | 说明 |
|------|------|------|
| `list_plugins` | `{}` | 列出所有插件及状态 |
| `enable_plugin` | `{ id }` | 启用指定插件本会话 |
| `disable_plugin` | `{ id }` | 禁用指定插件本会话 |

#### 4.8 文件上传

| type | data | 说明 |
|------|------|------|
| `upload_file` | `{ name, content, mime_type? }` | 上传文件到 `uploads/` 目录 (content 为 base64, 最大 50MB) |

---

### 5. 典型会话流程

```
客户端                                 服务器 (Server/Worker)
  │                                            │
  │── WS Connect ws://host:9527/agent ────────▶│
  │                                            │
  │◀─────── ready (version, workdir, ...) ─────│  ← 连接建立
  │◀─────── session_info (exists, ...) ────────│
  │◀─────── sandbox_status (enabled, ...) ─────│
  │                                            │
  │── user_message { text: "你好" } ───────────▶│
  │                                            │  Agent 开始处理
  │◀─────── thinking_start  ───────────────────│
  │◀─────── thinking_token { token: "..." } ───│
  │◀─────── thinking_end ──────────────────────│
  │                                            │
  │◀─────── stream_start ──────────────────────│
  │◀─────── streaming_token { token: "你" } ───│
  │◀─────── streaming_token { token: "好" } ───│
  │◀─────── ... ──────────────────────────────│
  │◀─────── stream_end ────────────────────────│
  │                                            │
  │◀─────── tool_use { tool: "read_file", ... }│  ← 工具调用
  │◀─────── tool_result { output: "...", ... } │
  │                                            │
  │◀── confirm_request { action: "write_file"} │  ← 请求确认
  │── confirm_response { approved: true } ────▶│  ← 客户端批准
  │                                            │
  │◀─────── done { text: "已完成", ... } ──────│  ← 本轮结束
  │◀─────── sandbox_status { pending_changes } │
  │◀─────── session_info (更新后) ─────────────│
  │                                            │
  │── user_message { text: "下一个任务" } ─────▶│  ← 新一轮
  │  ...                                       │
```

---

### 6. 确认请求 (confirm_request) 的 action 类型

| action | 额外字段 | 触发场景 |
|--------|----------|----------|
| `write_file` | `path`, `lines` | 写文件 |
| `edit_file` | `path` | 编辑文件 |
| `run_command` | `command` | 执行 shell 命令 |
| `delete_file` | `path` | 删除文件 |
| `review_plan` | `preview` | 审查执行计划 |

客户端响应格式：
```json
// 批准
{ "type": "confirm_response", "data": { "approved": true } }

// 拒绝
{ "type": "confirm_response", "data": { "approved": false } }

// 追问 (需要澄清)
{ "type": "confirm_response", "data": { "clarify": "你确定要删除这个文件吗？" } }
```

---

### 7. Ask User 响应格式

```json
// 请求 (服务器发送)
{ "type": "ask_user", "data": { "question": "你想用什么颜色？" } }

// 响应 (客户端发送)
{ "type": "ask_user_response", "data": { "answer": "蓝色" } }
```

---

### 8. Review Plan 响应格式

```json
// 请求 (服务器发送)
{ "type": "review_plan", "data": { "plan": "..." } }

// 响应 (客户端发送)
{ "type": "review_plan_response", "data": { "approved": true } }
// 或
{ "type": "review_plan_response", "data": { "approved": false, "feedback": "请改用 Rust" } }
```

---

### 9. 断线重连恢复 (Reconnection Recovery)

每个 Server Event 携带单调递增的 `seq` 序号。客户端断开重连后，可发送 `resume` 消息请求从指定序号恢复：

```json
// 客户端请求恢复 (从 seq 5 开始重传)
{ "type": "resume", "data": { "from_seq": 5 } }

// 服务器响应
{
  "type": "resume_ack",
  "seq": 99,
  "data": {
    "from_seq": 5,
    "last_seq": 100,
    "accepted": true
  }
}
```

客户端应检测 `seq` 间隙（如收到 seq=3 后直接收到 seq=6）来发现丢失事件。

### 10. WebSocket Ping/Pong

Worker 收到 WebSocket Ping 帧时自动回复 `pong` 事件：

```json
{ "type": "pong", "seq": 98, "data": {} }
```

---

### 11. 错误处理

- **无效 JSON**: 返回 `{ "type": "error", "data": { "message": "Invalid JSON: ..." } }`
- **Agent 正忙**: 返回 `{ "type": "error", "data": { "message": "Agent is busy processing a previous request" } }`
- **空消息**: 返回 `{ "type": "error", "data": { "message": "Empty user_message text" } }`
- **连接关闭**: reader 任务检测到 `Message::Close` 后退出，agent 循环随之退出，worker 进程退出

---

### 12. 集群/多节点

#### 12.1 `/probe` 端点

客户端连接 `ws://host:9527/probe`，服务器发送一个 `ready` 帧后等待客户端关闭：

```json
{
  "type": "ready",
  "data": {
    "version": "0.x.x",
    "workdir": "/path/to/project",
    "isolation": "sandbox",
    "sandbox": true,
    "caps": { ... },
    "virtual_nodes": [ ... ]
  }
}
```

#### 12.2 `/nodes` HTTP GET 端点

返回所有已知节点 (本地 + 已发现的 peer 子节点)：

```json
{
  "nodes": [
    {
      "name": "my-local-gpu",
      "url": "ws://host:9527/agent?workdir=...",
      "status": "online",
      "tags": ["gpu", "cuda"],
      "isolation": "sandbox",
      "sandbox": true,
      "description": "...",
      "workdir": "...",
      "exec_mode": "auto",
      "source": "local"
    }
  ]
}
```

#### 12.3 Peer 自动发现

- 服务器启动时并发探测所有配置的 `[[peer]]` 节点
- 每 30s 重试离线 peer，每 120s 心跳检测在线 peer
- Peer 的子节点以 `{node_name}@{peer_name}` 格式注册到路由表