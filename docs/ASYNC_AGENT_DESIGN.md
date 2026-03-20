# Async Agent Architecture: SubAgent + Service

## 背景与目标

当前 `call_sub_agent` 工具依赖 WebSocket 协议，要求预先手动启动 `--mode server` 进程，架构耦合重。本设计目标：

1. **SubAgent**：即用即走，通过 stdio 子进程通信，不依赖预先存在的 server
2. **Service**：全局常驻连接，用于咨询/通知类服务，支持 tool/skill 访问
3. **输出分离**：SubAgent 输出带前缀混入主流；Service 通知走独立方法

---

## 一、SubAgent（stdio 模式）

### 工作原理

```
Agent ──spawn──► agent --mode stdio --yes
        stdin ►  {"type":"user_message","data":{"text":"..."}}
        stdout ◄ {"type":"streaming_token","data":{"token":"..."}}
                 {"type":"tool_use",...}
                 {"type":"done","data":{"text":"..."}}
```

子进程 `--mode stdio --yes` 自动批准所有工具调用，完成后进程自然退出。

### 工具参数（spawn_sub_agent）

```json
{
  "prompt": "string — 子任务描述（必填）",
  "target_dir": "string — 限制子 agent 的工作目录（可选）",
  "auto_approve": "bool — 子 agent 内部工具是否自动批准（默认 true）",
  "timeout_secs": "u64 — 最大等待秒数（默认 300s）"
}
```

### 输出前缀规则

所有来自子 agent 的输出，在主流中以 `[sub:{short_id}]` 前缀显示：

```
[agent]    派遣子任务分析日志...
[sub:a1f2] 📖 reading /var/log/app.log
[sub:a1f2] 🔨 grep -n "ERROR" app.log (3 matches)
[sub:a1f2] ✅ 任务完成
[agent]    根据子任务结果，问题在 line 42...
```

`short_id` 取 UUID 前 4 个字符，便于多个并发子任务区分。

### 与旧版 call_sub_agent 的对比

| 维度 | 旧版（WebSocket） | 新版（stdio） |
|------|-----------------|--------------|
| 依赖 | 必须预先启动 server | 无需，spawn 即用 |
| 隔离 | 共享 server 进程 | 独立进程，完全隔离 |
| 并发 | 一个 server 串行处理 | 可并发多个子进程 |
| 复杂度 | 高（WS 协议、心跳） | 低（stdin/stdout） |
| 适用场景 | 已有常驻 server | 临时子任务（主用途） |

旧版 `call_sub_agent`（WebSocket）**保留**，用于连接外部已存在的 agent server 实例。
新增 `spawn_sub_agent`（stdio）作为主推的子任务派遣工具。

---

## 二、Service（全局常驻连接）

### 定位

Service 是**带状态的工具服务器**，不是 agent。核心约束：

- **Simple only**：不做多步 pipeline，接受问题，一次性返回答案
- **全局单例**：`ServiceManager` 由 Agent 持有，跨会话复用连接
- **单队列**：同时只处理一个请求，`Semaphore(1)` 保护，超队列深度拒绝
- **多连接支持**：可同时持有多个不同 service 的连接（按名字区分）

### 模块结构

```
src/service.rs              ServiceManager + ServiceClient trait
src/tools/query_service.rs  query_service tool（向 service 查询）
```

### ServiceClient Trait

```rust
#[async_trait]
pub trait ServiceClient: Send + Sync {
    /// 连接到 service（幂等，已连接则直接返回）
    async fn connect(&mut self) -> Result<()>;

    /// 发送一个查询，等待响应（Simple 模式，单次往返）
    async fn query(&self, question: &str) -> Result<String>;

    /// 服务是否在线
    fn is_connected(&self) -> bool;

    /// 断开连接
    async fn disconnect(&mut self);
}
```

### ServiceManager

```rust
pub struct ServiceManager {
    /// 按名称存储的 service 连接
    clients: HashMap<String, Box<dyn ServiceClient>>,
    /// 每个 service 的请求信号量（单队列保护）
    semaphores: HashMap<String, Arc<Semaphore>>,
    /// Service 推送事件通道（推送模式，第二阶段实现）
    event_tx: Option<mpsc::Sender<ServiceEvent>>,
}
```

### 初期支持的 Service 类型

1. **WebSocket Service**（`WsServiceClient`）：连接到实现了简单 JSON 协议的常驻服务
2. **HTTP Service**（`HttpServiceClient`）：REST API 形式，每次查询是一个 HTTP POST
3. 未来：**本地进程 Service**（local model server，如 ollama）

### query_service 工具参数

```json
{
  "service_name": "string — service 标识符（必填）",
  "question": "string — 查询内容（必填）",
  "timeout_secs": "u64 — 超时（默认 30s）"
}
```

### connect_service 工具参数

```json
{
  "name": "string — 给这个连接起个名字（必填）",
  "url": "string — service 地址，ws:// 或 http:// （必填）",
  "protocol": "string — ws | http（默认 ws）"
}
```

---

## 三、AgentOutput Trait 扩展

新增两个方法：

```rust
/// 来自 SubAgent 的事件（带前缀标记）
fn on_sub_agent_event(&self, task_id: &str, event: SubAgentOutputEvent);

/// 来自外部 Service 的通知（走独立面板/状态栏）
fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str);
```

```rust
pub enum SubAgentOutputEvent {
    StreamStart,
    StreamEnd,
    Token(String),
    ToolUse { name: String },
    ToolResult { name: String, is_error: bool },
    Done(String),
    Error(String),
}

pub enum NotifyLevel {
    Info,
    Warning,
    Alert,
}
```

### 各实现的渲染策略

| 方法 | CliOutput | StdioOutput | WsOutput |
|------|-----------|-------------|---------|
| `on_sub_agent_event` | `[sub:id]` 前缀彩色输出 | `{"type":"sub_agent_event","task_id":...}` | 单独 frame |
| `on_service_notification` | 状态栏/带前缀行（`[svc]`） | `{"type":"service_notification",...}` | 独立 notification frame |

---

## 四、整体架构图

```
┌─────────────────────────────────────────────────────────────┐
│  输入层（未来可远程）                                          │
│  stdin / SSH / WebSocket client                              │
└───────────────────────┬─────────────────────────────────────┘
                        │ UserInput
                        ▼
┌─────────────────────────────────────────────────────────────┐
│  Agent 主循环                                                 │
│                                                              │
│  ┌──── LLM ────┐  ┌──── Tools ────────────────────────────┐ │
│  │ streaming   │  │ spawn_sub_agent  → stdio 子进程        │ │
│  │             │  │ call_sub_agent   → WS server（保留）   │ │
│  │             │  │ query_service    → ServiceManager      │ │
│  │             │  │ connect_service  → ServiceManager      │ │
│  └─────────────┘  └────────────────────────────────────────┘ │
│                                                              │
│  ┌──── ServiceManager ──────────────────────────────────┐   │
│  │  connections: HashMap<name, Box<dyn ServiceClient>>  │   │
│  │  semaphores:  HashMap<name, Semaphore(1)>             │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
         │                          │
         ▼                          ▼
┌─────────────────┐      ┌──────────────────────────┐
│ stdout/AgentOut │      │ Service 常驻进程           │
│ [sub:id] 前缀   │      │ Simple 模型 / RAG / 通知  │
│ [svc] 独立行    │      │ 单队列 Semaphore 保护      │
└─────────────────┘      └──────────────────────────┘
```

---

## 五、实施路径（分阶段）

### Phase 1（当前迭代）

- [x] 设计文档
- [ ] `AgentOutput` 扩展：`on_sub_agent_event` + `on_service_notification`
- [ ] `src/service.rs`：`ServiceClient` trait + `ServiceManager` + `WsServiceClient`
- [ ] `src/tools/spawn_sub_agent.rs`：stdio 子进程 sub-agent
- [ ] `src/tools/query_service.rs`：查询 service 工具
- [ ] `src/tools/connect_service.rs`：连接 service 工具
- [ ] 注册新工具，更新 `main.rs`

### Phase 2（下一迭代）

- [ ] `ServiceManager` 注入到 `Agent` 结构体，跨工具调用共享连接
- [ ] `HttpServiceClient` 实现
- [ ] Service 推送事件（subscribe 模式，`tokio::select!` 注入主循环）
- [ ] CLI 状态栏渲染（`crossterm` cursor positioning）

### Phase 3（远期）

- [ ] 输入/输出彻底分离：输入 channel 化，支持远程 CLI
- [ ] SubAgent 任务注册表（fire-and-forget + 异步结果通知）
- [ ] TUI 侧边 service 通知面板（`ratatui`）

---

## 六、文件改动清单

```
新增：
  src/service.rs                  ServiceManager + ServiceClient trait
  src/tools/spawn_sub_agent.rs    stdio 子进程 sub-agent 工具
  src/tools/query_service.rs      查询 service 工具
  src/tools/connect_service.rs    连接 service 工具

修改：
  src/output.rs                   新增 on_sub_agent_event / on_service_notification
  src/tools/mod.rs                注册新工具
  src/main.rs                     声明 service 模块
  src/agent.rs                    持有 ServiceManager（Phase 2）
```

---

## 七、SubAgent stdio 协议（与现有 --mode stdio 兼容）

父进程写入子进程 stdin：
```json
{"type":"user_message","data":{"text":"<task prompt>"}}
```

子进程 stdout 输出（复用现有 StdioOutput 事件格式）：
```json
{"type":"stream_start","data":{}}
{"type":"streaming_token","data":{"token":"..."}}
{"type":"tool_use","data":{"tool":"read_file","input":{...}}}
{"type":"tool_result","data":{"tool":"read_file","output":"..."}}
{"type":"stream_end","data":{}}
{"type":"done","data":{"text":"<final answer>"}}
```

无需修改 `StdioOutput`，完全复用现有协议。父进程用 `tokio::process` 读取这些事件，通过 `on_sub_agent_event` 转发给自己的 `AgentOutput`。
