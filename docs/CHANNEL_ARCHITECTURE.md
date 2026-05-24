# Channel 架构与远程工具代理设计

> 状态：设计讨论阶段，尚未实现
> 最后更新：2026-05-09

---

## 1. 核心理念：无头引擎 + 通道适配器

Agent 核心保持纯粹——只负责 LLM 推理、工具编排、对话管理。所有外部平台（终端、浏览器、微信、IDE）通过**协议适配层**接入，不侵入 Agent 内部。

```
                        ┌──────────────────────────┐
                        │      Agent Core           │
                        │   (agent.rs 循环)          │
                        │                           │
                        │   • LLM 调用与推理         │
                        │   • 工具编排               │
                        │   • 对话/上下文管理         │
                        │   • Memory/Lessons         │
                        │                           │
                        │   trait AgentOutput  ◄──── 统一输出抽象
                        │   trait ToolExecutor       │
                        └──────────┬─────────────────┘
                                   │
              ┌────────────────────┼────────────────────┐
              │                    │                    │
     OutputBackend::Cli    OutputBackend::Stdio   OutputBackend::WebSocket
     (本地终端, crossterm)   (IDE 管道, JSON行)     (远程协议, JSON帧)
              │                    │                    │
         ./agent             ./agent --mode stdio   ./agent --mode server
         "Agent 的头"         "IDE 集成"              "无头引擎"
```

---

## 2. Channel 分类：三种接入形态

判断标准：**是否需要通过网络协议与 Agent 通信？**

### 2.1 OutputBackend（同进程，非 Channel）

| 实现 | 触发方式 | 用途 |
|------|----------|------|
| `OutputBackend::Cli` | `./agent` | 本地终端交互，Agent 原生界面 |
| `OutputBackend::Stdio` | `./agent --mode stdio` | IDE/编辑器的 JSON 管道通信 |

**CLI 不做 Channel 化。** 它是 Agent 自身的一部分——同进程、同生命周期、不需要网络。CLI 的特殊能力（REPL 斜杠命令、Ctrl-C 打断、crossterm 彩色渲染）深度依赖终端，强行分离只会增加复杂度和延迟。

### 2.2 Server-side Channel（Server 管理生命周期）

由 PluginManager 根据插件声明自动启动/停止的外部进程。生命周期依附于 Agent Server。

```
Agent Server 启动 → PluginManager::spawn_channels() → 拉起子进程
Agent Server 关闭 → PluginManager::stop_channels()  → 终止子进程
```

**示例：**
- WeChat Bridge：连接 wechatbot.dev，收发微信消息
- Telegram Bridge：连接 Telegram Bot API
- Discord Bridge：连接 Discord Gateway

### 2.3 Client-side Channel（用户自行启动）

用户在自己终端上独立运行的客户端进程，通过 WebSocket 连接到 Agent Server。生命周期独立于 Server。

```bash
./agent connect ws://gpu-server:9527      # 远程 CLI 客户端
# 或
浏览器打开 http://gpu-server:9527/ui/     # Web UI
```

**示例：**
- 远程 CLI 客户端（`./agent connect <url>`）
- Web UI（浏览器）
- VS Code 扩展

### 2.4 全景图

```
                              ./agent (本地, OutputBackend::Cli)
                              ./agent --mode stdio (IDE 集成, OutputBackend::Stdio)

  ┌─────────────────────────────────────────────────────────────────────┐
  │                    ./agent --mode server                            │
  │                                                                     │
  │  ┌──────────────────────────────┐    ┌──────────────────────────┐   │
  │  │  PluginManager               │    │  WebSocket :9527          │   │
  │  │                              │    │                          │   │
  │  │  [[channels]]  ← 插件声明     │    │  接受 Client-side        │   │
  │  │  ├─ wechat-gateway  (进程)   │    │  Channel 连接            │   │
  │  │  ├─ telegram-bridge  (进程)  │    │                          │   │
  │  │  └─ ...                     │    │  ◄── ./agent connect     │   │
  │  └──────────────────────────────┘    │  ◄── Web UI (浏览器)     │   │
  │                                      │  ◄── VS Code 扩展        │   │
  └──────────────────────────────────────┴──────────────────────────┘
```

---

## 3. Server-side Channel 的插件声明

在 `plugin.toml` 中新增 `[[channels]]` 段，让插件系统成为所有外部平台接入的**统一配置入口**。

### 3.1 声明语法

```toml
# wechat-bridge/plugin.toml
name = "wechat-bridge"
version = "1.0.0"
description = "微信接入通道 (wechatbot.dev)"

[[channels]]
name = "wechat-gateway"
type = "process"                    # 外部进程
command = "wechat-gateway"          # 可执行文件名（从 bin/ 目录查找）
args = [
    "--agent-url", "ws://localhost:${AGENT_PORT}",
    "--wechat-token", "${WECHAT_TOKEN}"
]
env = { WECHAT_TOKEN = "${WECHAT_TOKEN}" }
auto_start = true                   # Server 启动时自动拉起
restart_on_exit = true              # 进程退出后自动重启
restart_delay_secs = 5

[channel.session]
mode = "persistent"                 # persistent | stateless
ttl_seconds = 1800                  # 30分钟无消息自动关闭
max_sessions = 50                   # 最大并发会话数

[channel.security]
allowed_user_ids = ["wxid_abc123", "wxid_def456"]  # 微信白名单
rate_limit_per_minute = 6           # 每用户每分钟最多 6 条消息

# 可选：工具的权限策略
[channel.tool_policy]
# 该通道下所有会话的工具权限（可被会话级覆盖）
allow = ["read_file", "grep_search", "file_search", "list_directory", "think"]
require_confirm = ["write_file", "edit_file", "run_command", "git"]

[channel.install]
release_url = "https://github.com/xxx/wechat-gateway/releases/latest/download/wechat-gateway-{platform}-{arch}.tar.gz"
sha256_url = "{release_url}.sha256"
```

### 3.2 PluginManager 新增能力

```rust
// src/plugin/channel.rs  (新建)

/// 通道进程管理器
pub struct ChannelManager {
    children: Vec<ChannelChild>,
}

struct ChannelChild {
    name: String,
    process: Child,
    restart_on_exit: bool,
    restart_delay: Duration,
}

impl ChannelManager {
    /// 扫描所有已启用插件的 [[channels]] 声明
    pub fn collect_from_plugins(plugins: &[PluginMeta]) -> Vec<ChannelConfig> { ... }

    /// 启动所有 auto_start = true 的通道进程
    pub fn spawn_all(configs: &[ChannelConfig]) -> Vec<ChannelChild> { ... }

    /// 停止所有通道子进程
    pub fn stop_all(&mut self) { ... }

    /// 后台监控：检测退出并自动重启
    pub fn spawn_watchdog(&self) -> JoinHandle<()> { ... }
}
```

### 3.3 Server 启动流程增强

```rust
// src/server.rs 的 run() 末尾增加：

// 收集所有插件的 [[channels]] 声明
let channels = ChannelManager::collect_from_plugins(&plugins);

// 启动 auto_start 的通道进程
let mut channel_manager = ChannelManager::new();
channel_manager.spawn_all(&channels)?;

// 启动后台监控
let watchdog = channel_manager.spawn_watchdog();

info!("Server ready with {} channels", channels.len());

// 接受连接的主循环保持不变...

// 关闭时
watchdog.abort();
channel_manager.stop_all();
```

---

## 4. Client-side Channel：`./agent connect`

远程 CLI 客户端，复用量极大的现有代码，新增量极小。

### 4.1 使用方式

```bash
# 连接远程 Agent Server
./agent connect ws://gpu-server:9527

# 带选项
./agent connect ws://gpu-server:9527 --workdir /home/user/projects/foo --auto-approve

# 简写
./agent connect gpu-server:9527    # 自动补 ws:// 前缀
```

### 4.2 实现策略：最大化复用现有模块

```
./agent connect 的数据流：

  终端输入 (cli.rs REPL 斜杠命令)
      │
      ▼
  WebSocket 客户端 (新增, ~200行)
      │  json!({"type":"user_message","text": input })
      ▼
  ──── 网络 ────►  Agent Server (worker.rs, 不变)
                      │
                      │  streaming_token, tool_use, stream_end...
                      ▼
  ──── 网络 ────►  WebSocket 客户端 (接收事件)
                      │
                      ▼
  OutputBackend::Cli  (ui.rs Markdown渲染、Diff高亮、彩色输出)
      │
      ▼
  终端显示
```

**复用清单：**

| 模块 | 复用方式 | 说明 |
|------|----------|------|
| `ui.rs` | 直接复用 | Markdown 渲染、Diff 高亮、彩色输出 |
| `cli.rs` | 部分复用 | REPL 循环、斜杠命令解析（去掉 Agent 相关部分）|
| `output.rs` | `OutputBackend::Cli` | 终端渲染逻辑完全不变 |

**新增：**

| 新增 | 说明 | 预估规模 |
|------|------|---------|
| `src/connect.rs` | WebSocket 客户端 + 事件→OutputBackend 翻译 | ~200行 |
| `src/main.rs` | 新增 `Connect` 子命令 | ~20行 |

### 4.3 协议层：connect 客户端需要支持的消息

客户端发送到 Server 的消息（现有协议已全部支持）：

| 消息类型 | 用途 |
|----------|------|
| `user_message` | 用户输入的文本 |
| `confirm_response` | 对工具执行的确认/拒绝 |
| `ask_user_response` | 回答 Agent 的反问 |
| `cancel` | 取消当前操作 |
| `set_model` | 切换模型 |
| `set_mode` | 切换执行模式 |

Server 推送给客户端的事件（现有协议已全部支持）：

| 事件类型 | 渲染方式 |
|----------|----------|
| `thinking_start/thinking_token/thinking_end` | 灰色/折叠的思考过程 |
| `stream_start/streaming_token/stream_end` | 实时逐字输出 |
| `tool_use` | 工具调用开始（显示工具名和参数）|
| `tool_result` | 工具执行结果 |
| `diff` | Diff 高亮显示 |
| `confirm_request` | 弹出确认提示 |
| `done` | 本轮完成 |
| `error` | 错误显示 |

> **关键发现：`./agent connect` 不需要任何新的协议消息。** 现有的 WebSocket 协议（定义于 `web-ui/src/types/agent.ts`）已经完整覆盖了远程 CLI 的需求。客户端唯一要做的就是将协议事件翻译为终端渲染。

### 4.4 与现有 `--mode stdio` 的区别

| | `--mode stdio` | `./agent connect` |
|---|---|---|
| 通信方式 | stdin/stdout JSON 行 | WebSocket |
| 启动方 | IDE/编辑器 | 用户终端 |
| Agent 位置 | IDE 启动的子进程 | 远程 Server |
| 用途 | 编辑器内嵌 Agent | 远程终端访问 Agent |

两者共享相同的 JSON 事件协议，但传输层和启动方式不同。

---

## 5. 远程工具代理（Full Tool Proxy）

当 `workdir_owner == "client"` 时，所有文件/命令类工具调用由 Server 转发到 Client 本地执行。

### 5.1 为什么可行

LLM 从来没见过真实的文件系统——它始终只消费工具返回的**文本结果**。无论工具在 Server 本地执行还是代理到 Client 执行，LLM 看到的是完全相同的文本反馈。因此工具代理**不会引入新的误判**。

### 5.2 协议扩展

需要在现有协议上增加少量新消息类型：

```typescript
// === 新增 Client-side 消息 ===

// 连接握手时声明工作目录归属和代理配置
interface HandshakeMessage extends BaseMessage {
  type: 'handshake';
  data: {
    workdir: string;
    workdir_owner: 'client' | 'server';       // 工作目录在谁的文件系统上
    proxy_tools: string[];                     // 需要代理的工具列表
    environment: ClientEnvironment;            // 客户端环境快照（注入 system prompt）
  };
}

interface ClientEnvironment {
  os: string;                   // "macOS 14.3", "Ubuntu 22.04"
  shell: string;                // "zsh", "bash"
  arch: string;                 // "x86_64", "aarch64"
  package_manager: string;      // "brew", "apt", "dnf"
  project_type?: string;        // "rust", "python", "node"
  available_bins?: string[];    // ["cargo", "git", "docker"]
}

// 工具代理请求（Server → Client）
interface ToolProxyRequestEvent extends BaseMessage {
  type: 'tool_proxy_request';
  data: {
    proxy_id: string;            // 关联 ID
    tool: string;                // 工具名
    args: Record<string, any>;   // 工具参数
  };
}

// 工具代理结果（Client → Server）
interface ToolProxyResultMessage extends BaseMessage {
  type: 'tool_proxy_result';
  data: {
    proxy_id: string;
    result: ToolResultData;      // 工具返回结果（与本地工具调用格式一致）
  };
}

interface ToolResultData {
  ok: boolean;
  output?: string;
  error?: string;
}
```

### 5.3 全量代理策略

**规则：要么全代理，要么一个都不代理。不允许混合。**

```
workdir_owner == "client" → 所有 [read_file, write_file, edit_file, multi_edit_file,
                            run_command, git, grep_search, file_search, list_directory]
                            全部代理到 Client 执行

workdir_owner == "server" → 所有工具在 Server 本地执行（现有行为）
```

无状态工具（`think`）不受影响，在哪执行都一样。

### 5.4 环境信息注入

Client 在连接握手时发送环境快照，Agent Server 将其注入 system prompt：

```
You are connected to a user's local machine via a secure proxy.
ALL file operations and shell commands are executed on the USER'S MACHINE:

  OS: macOS 14.3
  Shell: zsh
  Architecture: aarch64 (Apple Silicon)
  Package Manager: Homebrew (brew)
  Working Directory: /Users/xxx/projects/foo
  Available Tools: cargo, git, docker, python3, node

Use `brew` for package installation, NOT `apt-get`.
Use macOS-style paths and commands.
```

这确保 LLM 不会用 Server 的环境去推断 Client 端的操作。

### 5.5 Agent 侧的改动

```rust
// agent.rs — 工具执行循环中的改动点

// 当前逻辑：
let result = tool_executor.execute(&tool_name, &args).await;

// 增加判断：
let result = if is_proxy_tool(&tool_name, &session_proxy_config) {
    output.send_tool_proxy_request(&tool_name, &args).await?
} else {
    tool_executor.execute(&tool_name, &args).await?
};
```

Agent 核心的改动仅限于：**在执行工具之前判断是否需要代理**。如果代理，将工具调用转发给 OutputBackend，OutputBackend 的 WebSocket 实现负责发送 `tool_proxy_request` 并等待 `tool_proxy_result`。如果 OutputBackend 是 Cli（本地模式），则永远不会触发代理路径。

### 5.6 错误处理

| 场景 | 处理 |
|------|------|
| 网络断开，代理请求未送达 | 超时后返回错误给 LLM："Tool proxy request timed out" |
| Client 执行工具时崩溃 | Client 重连后返回 `tool_proxy_result` 带 error |
| 代理的工具不在 Client 支持的列表中 | Server 返回 "Tool not available on client" |

---

## 6. 客户端极简化设计

### 6.1 设计原则

Client-side Channel 应该是一个**薄壳**：

```
┌─ Client-side Channel ──────────────────────────────────────────────┐
│                                                                     │
│  ┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐   │
│  │  通信层       │    │  本地工具执行器    │    │  渲染层（复用）   │   │
│  │  WebSocket   │    │  (if proxy mode) │    │  ui.rs           │   │
│  │  客户端       │    │                  │    │  output.rs       │   │
│  │              │    │  read_file        │    │  cli.rs          │   │
│  │  收发 JSON   │    │  write_file       │    │                  │   │
│  │  事件        │    │  run_command      │    │  （纯 display,    │   │
│  │              │    │  grep_search      │    │   不做决策）      │   │
│  └──────┬───────┘    └────────┬─────────┘    └────────┬─────────┘   │
│         │                     │                       │             │
│         └─────────────────────┼───────────────────────┘             │
│                               │                                     │
│                       事件 → 渲染 映射                               │
│                       代理 → 执行 映射                               │
└─────────────────────────────────────────────────────────────────────┘
```

**Client 不做的事：**
- ❌ 不做 LLM 推理
- ❌ 不做对话管理
- ❌ 不做工具编排（何时调用哪个工具是 Server 决定的）
- ❌ 不做上下文窗口管理

**Client 做的事：**
- ✅ WebSocket 连接管理（重连、心跳）
- ✅ 将用户输入包装为 `user_message` 发送
- ✅ 接收 Server 事件并渲染到终端/UI
- ✅ 收到 `tool_proxy_request` 时在本地执行并返回结果
- ✅ 将用户确认/回答包装为 `confirm_response`/`ask_user_response` 发回

### 6.2 `./agent connect` 的极简实现草图

```rust
// src/connect.rs — 核心循环

pub async fn run(url: &str, workdir: &Path) -> Result<()> {
    // 1. 连接 WebSocket
    let (ws, _) = tokio_tungstenite::connect_async(url).await?;

    // 2. 发送握手（环境快照 + 代理声明）
    let handshake = Handshake {
        workdir: workdir.to_string_lossy().to_string(),
        workdir_owner: "client",
        proxy_tools: vec!["read_file", "write_file", "edit_file", ...],
        environment: probe_environment()?,
    };
    ws.send(handshake.to_json()).await?;

    // 3. 等待 ready
    let ready = wait_for_ready(&ws).await?;

    // 4. 主循环：用户输入 ↔ Server 事件
    let mut repl = CliRepl::new();
    loop {
        tokio::select! {
            // 用户在终端输入
            line = repl.read_line() => {
                ws.send(json!({"type":"user_message","data":{"text": line}})).await?;
            }
            // Server 推送事件
            event = ws.recv() => {
                match event["type"].as_str() {
                    "streaming_token" => output.push_token(&event["data"]["token"]),
                    "tool_use" => output.show_tool(&event["data"]),
                    "tool_proxy_request" => {
                        // 在本地执行工具
                        let result = local_execute_tool(&event["data"]).await;
                        ws.send(json!({"type":"tool_proxy_result","data":result})).await?;
                    }
                    "done" => { output.flush(); }
                    // ... 其他事件全部翻译为 output 调用
                }
            }
        }
    }
}
```

---

## 7. 实施路线图

### Phase 1：WeChat Bridge MVP（TypeScript，独立包，零侵入 Agent）

**目标：** 验证"Agent Server ↔ 外部平台"的 WebSocket 协议是否流畅。

**语言选择：TypeScript。** 理由见 §9。

| 任务 | 位置 | 说明 |
|------|------|------|
| 创建 `bridges/wechat/` npm 包 | 新建 | TypeScript 项目，依赖 `@wechatbot/wechatbot` + `ws` |
| 导入协议类型 | 复用 | 从 `web-ui/src/types/agent.ts` 直接 import |
| 实现 `session-pool.ts` | 新建 | 连接池：WeChat 用户 ID → Agent WebSocket 连接 |
| 实现 `agent-client.ts` | 新建 | Agent WebSocket 客户端（收发 JSON 事件） |
| 实现 `gateway.ts` | 新建 | 核心桥接：微信消息 ↔ Agent 事件 翻译 |
| 实现 `main.ts` | 新建 | CLI 入口，登录 + 启动 + 信号处理 |
| 编写 README | 新建 | 使用文档 |

Agent 侧改动：**零。**

#### WeChat Bridge 详细设计

基于 wechatbot.dev 官方 SDK（`@wechatbot/wechatbot`，TypeScript，零运行时依赖，69 个测试）。

**核心 API：**

```typescript
import { WeChatBot } from '@wechatbot/wechatbot'

const bot = new WeChatBot({
  storage: 'file',              // 会话持久化，重启后自动恢复登录
  storageDir: '~/.wechatbot',
})

await bot.login()  // 首次弹二维码，后续自动恢复

bot.onMessage(async (msg) => {
  console.log(msg.userId)   // 微信用户唯一 ID
  console.log(msg.text)     // 文本内容
  console.log(msg.isGroup)  // 是否群消息
})

await bot.start()
```

**Bridge 架构：**

```
微信 → wechatbot.dev 长轮询 → WeChatBot SDK
                                  │
                            bot.onMessage(msg)
                                  │
                                  ▼
                          SessionPool.get_or_create(msg.userId)
                                  │
                          ┌───────┴───────┐
                          │ 新建连接？       │
                          │ ws://agent:9527/agent │
                          └───────┬───────┘
                                  │
                                  ▼
                   ws.send({ type: "user_message", data: { text: msg.text } })
                                  │
                                  ▼
                   接收 streaming_token / tool_use / done 等事件
                                  │
                                  ▼
                   按 chunk_size 分片 → bot.send(userId, chunk)
                   或等 stream_end 后 → bot.reply(msg, fullText)
```

**流式响应的微信适配：** 微信不支持真正的 streaming。Bridge 提供两种模式：

| 模式 | 行为 | 适用场景 |
|------|------|---------|
| **分片模式** | 每积攒 N 个 token 发送一条消息 | 长响应，用户可以看到"正在输入"的效果 |
| **聚合模式**（默认） | 等 `done` 事件后一次性发送完整回复 | 避免刷屏，更自然的对话体验 |

```typescript
// 分片模式实现草图
bot.onMessage(async (msg) => {
  const ws = await pool.get(msg.userId);
  await bot.sendTyping(msg.userId);  // 显示"正在输入..."

  ws.send(json({ type: 'user_message', data: { text: msg.text } }));

  let buffer = '';
  ws.on('message', (data) => {
    const event = JSON.parse(data);
    switch (event.type) {
      case 'streaming_token':
        buffer += event.data.token;
        if (buffer.length >= CHUNK_SIZE) {
          bot.send(msg.userId, buffer).catch(() => {});  // 静默失败
          buffer = '';
        }
        break;
      case 'stream_end':
        if (buffer) bot.reply(msg, buffer);  // 最终 reply（自动清除 typing）
        break;
    }
  });
});
```

**连接池管理（session-pool.ts）：**

```typescript
class SessionPool {
  private sessions = new Map<string, AgentSession>();

  async get(userId: string): Promise<AgentSession> {
    let session = this.sessions.get(userId);
    if (session && !session.isAlive()) {
      this.sessions.delete(userId);
      session = undefined;
    }
    if (!session) {
      session = await AgentSession.connect(AGENT_URL, userId);
      this.sessions.set(userId, session);
    }
    session.touch();  // 更新最后活跃时间
    return session;
  }

  // 定时器每 30 秒清理超时的连接（释放 Server 侧的 worker 进程）
  startCleanup(ttlMs: number = 30 * 60_000) {
    setInterval(() => {
      for (const [id, s] of this.sessions) {
        if (Date.now() - s.lastActive > ttlMs) {
          s.close();
          this.sessions.delete(id);
        }
      }
    }, 30_000);
  }
}
```

### Phase 2：`./agent connect` 远程 CLI 客户端

**目标：** 验证 Client-side Channel 模式，最大化代码复用。

| 任务 | 位置 | 说明 |
|------|------|------|
| 新增 `src/connect.rs` | 新建 | WebSocket 客户端核心 |
| `main.rs` 增加 `Connect` 子命令 | 编辑 | clap 解析 |
| 实现 handshake 与环境探测 | 新建/编辑 | `src/connect.rs` |

Agent Server 侧改动：**零**（现有协议已足够）。

### Phase 3：`[[channels]]` 扩展点

**目标：** 让 PluginManager 管理 Server-side Channel 进程的生命周期。

| 任务 | 位置 | 说明 |
|------|------|------|
| 新增 `src/plugin/channel.rs` | 新建 | ChannelConfig、ChannelManager |
| 编辑 `src/plugin/metadata.rs` | 编辑 | PluginMeta 增加 channels 字段 |
| 编辑 `src/plugin/manager.rs` | 编辑 | collect/spawn/stop 方法 |
| 编辑 `src/plugin/mod.rs` | 编辑 | 导出 channel 模块 |
| 编辑 `src/server.rs` | 编辑 | 启动/关闭时调用 ChannelManager |

### Phase 4：全量工具代理

**目标：** 实现 `workdir_owner == "client"` 时的工具全量代理。

| 任务 | 位置 | 说明 |
|------|------|------|
| 协议扩展：`handshake`、`tool_proxy_request`/`result` | `agent.ts` | 新增消息类型 |
| OutputBackend 代理接口 | `src/output.rs` | 增加 `send_tool_proxy_request()` |
| WsOutput 代理实现 | `src/output.rs` | WebSocket 的代理转发 |
| Agent 循环代理判断 | `src/agent.rs` | 工具执行前检查是否需要代理 |
| Connect 客户端本地执行 | `src/connect.rs` | 接收代理请求，本地执行，返回结果 |
| 环境探针 | `src/connect.rs` | `probe_environment()` 收集 OS/shell/etc |

### Phase 5：微信插件化封装

**目标：** 将 Phase 1 的 WeChat Bridge 打包为标准插件，通过 `[[channels]]` 声明实现开箱即用。

| 任务 | 位置 | 说明 |
|------|------|------|
| 创建 `sample-plugin/wechat-bridge/` | 新建 | 插件目录 |
| `plugin.toml` + `channels.toml` | 新建 | 通道声明 + 配置 |
| `system_prompt.md` | 新建 | 微信场景提示词 |
| 二进制自动下载 | `channel.install` | GitHub Release 下载 |

### 依赖关系

```
Phase 1 (WeChat Bridge)  ──独立──►  可直接使用，不依赖后续 Phase
        │
Phase 2 (./agent connect) ─独立──►  可直接使用，不依赖 Phase 1
        │                                    │
Phase 3 ([[channels]])    ◄──────────────────┘  将 Phase 1 的 Bridge 管理起来
        │
Phase 4 (Tool Proxy)      ◄────────────────────  connect 客户端的高级能力
        │
Phase 5 (Plugin 封装)     ◄── Phase 3 + Phase 1  打包为开箱即用的插件
```

Phase 1 和 Phase 2 可以**并行开发**——互不依赖，分别验证 Server-side 和 Client-side Channel 模式。

---

## 8. 决策记录

| 决策点 | 结论 | 理由 |
|--------|------|------|
| CLI 是否 Channel 化 | ❌ 不做 | CLI 是 Agent 原生界面，同进程，不需要网络 |
| `./agent connect` 是否新建 crate | ❌ 作为子命令 | 大量复用 ui.rs/output.rs/cli.rs |
| 工具代理策略 | 全量代理 | 不允许部分代理，防止 LLM 认知分裂 |
| 环境注入 | handshake 时发送 | 注入 system prompt，让 LLM 正确理解执行环境 |
| 会话模型 | persistent + TTL | 多轮对话体验好，TTL 防止资源泄漏 |
| Channel 二进制分发 | 自动下载 | 类似 rust-analyzer，从 GitHub Release 下载 |
| WeChat Bridge 语言 | **TypeScript** | `@wechatbot/wechatbot` 是官方 TypeScript SDK；复用 `web-ui/src/types/agent.ts` 协议类型；Rust 生态无成熟微信 SDK |
| WeChat Bridge 在哪 | `bridges/wechat/` 独立 npm 包 | 零侵入 Agent，故障隔离 |
| Bridge 连接模式 | 一个微信用户一条 WebSocket → 一个 worker 进程 | 利用现有 process-per-connection 模型；用户间天然隔离；并发无阻塞 |
| Streaming → 微信 | 聚合模式（默认），可选分片 | 微信不支持真正流式；聚合模式避免刷屏 |
| 先做 Bridge 还是先做 Channels | 先做 Bridge MVP | 验证协议可行性后再投入架构改动 |

---

## 9. 附录：WeChat Bridge 语言选择分析

### 9.1 判断标准

Bridge 和 Agent Server 之间通信的是 **WebSocket + JSON**，这是一个语言无关的标准协议。Bridge 本质上是一个协议翻译器：

```
微信 API 协议  ←→  Bridge  ←→  Agent WebSocket JSON 协议
```

选择语言的标准应该是：**哪个生态的微信 SDK 最成熟。**

### 9.2 各语言对比

| 语言 | 微信 SDK 生态 | 成熟度 | 与项目的关系 |
|------|-------------|--------|-------------|
| **TypeScript** | `@wechatbot/wechatbot`（官方）、wechaty | ⭐⭐⭐⭐⭐ | 可复用 `web-ui/src/types/agent.ts` 协议类型 |
| Python | itchat, wechaty-puppet | ⭐⭐⭐⭐ | 新语言，需单独维护协议类型 |
| Go | openwechat | ⭐⭐⭐ | 新语言 |
| Rust | 几乎没有 | ⭐ | 不做考虑 |

### 9.3 结论：TypeScript

决定因素排序：

1. **官方 SDK。** `@wechatbot/wechatbot` 是 wechatbot.dev 的官方 TypeScript SDK，零运行时依赖，69 个测试，API 设计简洁。`new WeChatBot()` → `await bot.login()` → `bot.onMessage(...)` → `await bot.start()` 四步即可运行。

2. **协议类型复用。** `web-ui/src/types/agent.ts` 已经精确定义了 Agent WebSocket 协议的全部消息类型。TypeScript Bridge 可以直接 import，获得编译时类型安全。其他语言需要手动维护一套相同的类型定义，迟早会不同步。

3. **项目一致性。** 项目已有 `web-ui/`（TypeScript + React），在 monorepo 中加一个 `bridges/wechat/` 的 TypeScript 包保持技术栈统一。

### 9.4 项目结构

```
bridges/wechat/
├── package.json              # 依赖: @wechatbot/wechatbot, ws, typescript
├── tsconfig.json             # paths: { "@agent/types": "../../web-ui/src/types/*" }
├── src/
│   ├── main.ts               # CLI 入口，环境变量读取，登录 + 启动
│   ├── gateway.ts             # 核心桥接逻辑（微信消息 ↔ Agent 事件）
│   ├── session-pool.ts        # 连接池管理（WeChat 用户 → Agent WS 连接）
│   ├── agent-client.ts        # Agent WebSocket 客户端封装
│   └── config.ts              # 配置（agent-url, chunk-size, ttl 等）
└── README.md
```
