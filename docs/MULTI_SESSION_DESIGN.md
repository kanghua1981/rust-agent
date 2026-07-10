# Multi-Session Design (多会话支持)

> **版本**: 1.0  
> **最后更新**: 2026-07-25  
> **状态**: 实现中

## 1. 概述

Rust Agent 之前一个工程目录（workdir）只对应一个会话（`.agent/session.json`）。多会话支持允许同一个工程拥有多个命名会话，彼此独立存储和切换，工作目录和记忆（memory）共享，但对话历史隔离。

---

## 2. 存储布局

### 2.1 目录结构

```
<project>/.agent/
├── sessions/              ← 新：命名会话目录
│   ├── _active            ← 纯文本，记录当前活跃会话名（如 "feature-foo"）
│   ├── default.json       ← 默认会话
│   ├── feature-foo.json   ← 命名会话
│   └── bugfix-bar.json
├── archive/               ← 溢出归档（已有，不变）
│   └── 2026-07.jsonl
├── memory.md              ← 持久记忆（已有，所有会话共享）
├── summary.md             ← 项目摘要（已有）
└── skills/                ← 项目技能（已有）
```

### 2.2 会话文件格式 (`<name>.json`)

```json
{
  "meta": {
    "id": "feature-foo",
    "session_name": "feature-foo",
    "created_at": "2026-07-25T10:00:00",
    "updated_at": "2026-07-25T14:30:00",
    "message_count": 42,
    "summary": "帮我实现用户登录功能",
    "working_dir": "/home/user/projects/myapp"
  },
  "system_prompt": "You are an expert...",
  "messages": [ ... ]
}
```

`id` 和 `session_name` 相同（本地会话），`working_dir` 记录项目路径。

### 2.3 `_active` 标记文件

纯文本，内容为当前活跃会话名，如 `feature-foo`。由 `write_active_session_name()` / `read_active_session_name()` 管理。

---

## 3. 命名规则

- 仅允许字母（a-z, A-Z）、数字（0-9）、连字符（`-`）、下划线（`_`）
- 最长 64 字符
- 不允许 `.` `/` `\` `..` 等路径分隔符
- 大小写敏感
- `_active` 为保留名，不可用作会话名
- 默认会话名为 `default`

---

## 4. 向后兼容性

### 4.1 自动迁移

启动时检测旧 `.agent/session.json`：
- 如果存在且 `.agent/sessions/default.json` 不存在 → 自动迁移为 `sessions/default.json` → 删除旧文件
- 如果 `default.json` 已存在 → 旧文件视为过期数据，直接删除
- 迁移由 `migrate_old_local_session()` 完成，`load_local_session()` 和 `save_local_session()` 内部自动调用

### 4.2 存量 API 不变

`save_local_session()` 和 `load_local_session()` 保持兼容，内部委托到 `save_local_named_session("default", ...)` / `load_local_named_session("default", ...)`。

---

## 5. CLI 交互

### 5.1 启动参数

```bash
agent --session-name feature-foo    # 指定启动会话
agent                                # 自动恢复 _active 标记的会话
agent --session-name new-name        # 会话不存在则自动创建
```

### 5.2 斜杠命令

| 命令 | 行为 | 示例 |
|---|---|---|
| `/new <name>` | 保存当前 → 创建空会话 `<name>` → 写入 `_active` → 提示 | `/new feature-login` |
| `/switch <name>` | 保存当前 → 加载 `<name>` → 写入 `_active` → 显示消息数 | `/switch default` |
| `/list` | 列出 `sessions/*.json`，`*` 标记当前活跃 | `/list` |
| `/rename <old> <new>` | 重命名会话文件 + 更新内部 meta | `/rename foo bar` |
| `/save` | 保存当前会话（命名感知） | `/save` |

### 5.3 会话解析优先级（启动时）

```
1. --session-name <name>   （CLI 显式指定）
2. .agent/sessions/_active （上次活跃会话）
3. "default"                （兜底）
```

由 `persistence::resolve_session_name(workdir, cli_override)` 实现。

---

## 6. WebSocket 协议

### 6.1 新增客户端 → 服务端消息

| type | data | 说明 |
|---|---|---|
| `list_local_sessions` | `{}` | 请求本地会话列表 |
| `switch_local_session` | `{"name": "feature-foo"}` | 切换到命名会话 |
| `new_local_session` | `{"name": "new-session"}` | 创建命名空会话 |
| `delete_local_session` | `{"name": "old-session"}` | 删除命名会话 |
| `rename_local_session` | `{"old_name": "foo", "new_name": "bar"}` | 重命名会话 |

### 6.2 新增服务端 → 客户端事件

| type | data | 说明 |
|---|---|---|
| `local_sessions_list` | `{"sessions": [...SessionMeta], "active": "default"}` | 本地会话列表 + 当前活跃名 |
| `session_switched` | `{"name": "feature-foo", "message_count": 42, "messages": [...]}` | 切换成功，含历史消息 |
| `session_renamed` | `{"old_name": "foo", "new_name": "bar"}` | 重命名成功 |
| `session_info` | `{"exists": true, "session_name": "default", ...}` | 增加 `session_name` 字段 |

### 6.3 URL 参数

Worker 连接 URL 增加 `session` 参数：

```
ws://host:9527/ws?workdir=/path&session=feature-foo
```

Worker 启动时调用 `resolve_session_name(workdir, Some(url_session))`。

---

## 7. Worker 启动流程

```
1. 解析 URL 参数 ?session=<name>
2. session_name = resolve_session_name(workdir, url_session)
3. migrate_old_local_session(workdir)         // 静默迁移
4. session = load_local_named_session(name, workdir)
5. 如果 session 存在 → 恢复对话历史 → emit session_available
6. 如果 session 不存在 → 创建空白会话 → save → write_active
7. 启动 agent loop
8. 每次 tool 循环后 auto-save 到命名会话
```

---

## 8. WebUI 交互流程

### 8.1 SessionsPanel 组件结构

```
┌─────────────────────────────────────┐
│  会话管理                            │
│  ┌─────────────────────────────────┐ │
│  │ 当前会话: default  [切换] [新建] │ │
│  │ 42 条消息 · 最后更新 14:30      │ │
│  └─────────────────────────────────┘ │
│                                      │
│  本地会话 (3)            [↻ 刷新]    │
│  ┌─────────────────────────────────┐ │
│  │ ★ default                       │ │
│  │   实现用户登录 · 42 条消息       │ │
│  │   [切换] [重命名] [删除]         │ │
│  ├─────────────────────────────────┤ │
│  │   feature-payment                │ │
│  │   集成支付宝 · 18 条消息          │ │
│  │   [切换] [重命名] [删除]         │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

### 8.2 操作序列

**新建会话**:
1. 用户点击 "新建" → 弹窗输入名称 → 确认
2. 前端: `sendRaw({type: "new_local_session", data: {name}})` 
3. Worker: 保存当前 → 创建空会话 → `emit("session_switched", ...)`
4. 前端: 清空消息列表 → 更新活跃会话名 → 刷新列表

**切换会话**:
1. 用户点击 "切换" → 确认（丢失未保存更改?）
2. 前端: `sendRaw({type: "switch_local_session", data: {name}})`
3. Worker: 保存当前 → 加载新会话 → `emit("session_switched", {messages})`
4. 前端: `clearSession()` → 填充历史消息 → 更新活跃名

---

## 9. 边界情况

| 场景 | 处理 |
|---|---|
| 切换到不存在的会话 | Worker 自动创建空白同名会话 |
| 删除当前活跃会话 | 自动回退到 `default`，若 `default` 不存在则创建 |
| 空会话被保存 | 允许（含 system_prompt，messages 为空数组） |
| 同时两个连接操作同一会话 | 各自独立保存（最后一次 wins），不做锁竞争 |
| 会话名冲突（新建已存在） | Worker 返回 error |
| `_active` 被手动删除 | `read_active_session_name()` 返回 `None` → 回退 `default` |

---

## 10. Rust API 速查表

| 函数 | 签名 | 用途 |
|---|---|---|
| `save_local_named_session` | `(name, &Conversation, workdir) -> Result<()>` | 保存命名会话 |
| `load_local_named_session` | `(name, workdir) -> Result<Option<SavedSession>>` | 加载命名会话 |
| `list_local_sessions` | `(workdir) -> Result<Vec<SessionMeta>>` | 列出所有本地会话 |
| `delete_local_named_session` | `(name, workdir) -> Result<()>` | 删除命名会话 |
| `rename_local_named_session` | `(old, new, workdir) -> Result<()>` | 重命名 |
| `read_active_session_name` | `(workdir) -> Option<String>` | 读取 `_active` |
| `write_active_session_name` | `(workdir, name) -> Result<()>` | 写入 `_active` |
| `resolve_session_name` | `(workdir, Option<&str>) -> String` | 解析最终会话名 |
| `migrate_old_local_session` | `(workdir) -> Result<Option<String>>` | 迁移旧格式 |
| `save_local_session` | `(&Conversation, workdir) -> Result<()>` | 向后兼容（→ `default`） |
| `load_local_session` | `(workdir) -> Result<Option<SavedSession>>` | 向后兼容（→ `default`） |
