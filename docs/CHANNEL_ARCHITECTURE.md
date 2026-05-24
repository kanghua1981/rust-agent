# Channel 架构设计

> 状态：Phase 1 ✅ / Phase 2 待实现 / Phase 3 ✅
> 最后更新：2026-06-08

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

## 5. `./agent connect` 客户端极简化设计

### 5.1 设计原则

`./agent connect` 是一个**纯粹的事件转发+渲染层**。它不执行任何工具，不维护对话状态。所有工具在 Server 侧执行，Client 只负责将 Server 推送的事件渲染到终端。

```
┌─ ./agent connect ─────────────────────────────────────────────────┐
│                                                                     │
│  ┌──────────────┐                        ┌──────────────────┐       │
│  │  通信层       │                        │  渲染层（复用）   │       │
│  │  WebSocket   │                        │  ui.rs           │       │
│  │  客户端       │                        │  output.rs       │       │
│  │              │                        │  cli.rs          │       │
│  │  收发 JSON   │                        │                  │       │
│  │  事件        │                        │  （纯 display,    │       │
│  │              │                        │   不做决策）      │       │
│  └──────┬───────┘                        └────────┬─────────┘       │
│         │                                         │                 │
│         └────────────── 事件 → 渲染 ──────────────┘                 │
│                                                                     │
│  简单循环：用户输入 → send → recv 事件 → 渲染                        │
└─────────────────────────────────────────────────────────────────────┘
```

**Client 不做的事：**
- ❌ 不做 LLM 推理
- ❌ 不做对话管理
- ❌ 不做工具执行（所有工具在 Server 侧执行）
- ❌ 不做上下文窗口管理

**Client 做的事：**
- ✅ WebSocket 连接管理（重连、心跳）
- ✅ 将用户输入包装为 `user_message` 发送
- ✅ 接收 Server 事件并渲染到终端/UI
- ✅ 将用户确认/回答包装为 `confirm_response`/`ask_user_response` 发回

### 5.2 `./agent connect` 实现草图

```rust
// src/connect.rs — 核心循环

pub async fn run(url: &str) -> Result<()> {
    // 1. 连接 WebSocket
    let (ws, _) = tokio_tungstenite::connect_async(url).await?;

    // 2. 等待 ready
    let ready = wait_for_ready(&ws).await?;

    // 3. 主循环：用户输入 ↔ Server 事件
    let mut repl = CliRepl::new();
    let output = CliOutput::new();
    loop {
        tokio::select! {
            // 用户在终端输入
            line = repl.read_line() => {
                ws.send(json!({"type":"user_message","data":{"text": line}})).await?;
            }
            // Server 推送事件
            event = ws.recv() => {
                match event["type"].as_str() {
                    "streaming_token" => output.on_streaming_text(&event["data"]["token"]),
                    "tool_use" => output.on_tool_use(&event["data"]),
                    "tool_result" => output.on_tool_result(&event["data"]),
                    "confirm_request" => {
                        output.on_confirm_request(&event["data"]);
                        let approved = repl.ask_yes_no();
                        ws.send(json!({"type":"confirm_response","data":{"approved":approved}})).await?;
                    }
                    "done" => output.flush(),
                    // ... 其他事件全部翻译为 output 调用
                }
            }
        }
    }
}
```

---

## 6. 实施路线图

### Phase 1：WeChat Bridge MVP（TypeScript，独立包，零侵入 Agent） ✅ 已完成

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

### Phase 2：`./agent connect` 远程 CLI 客户端（待实现）

**目标：** 实现远程 CLI 客户端，最大化代码复用。

| 任务 | 位置 | 说明 |
|------|------|------|
| 新增 `src/connect.rs` | 新建 | WebSocket 客户端核心 |
| `main.rs` 增加 `Connect` 子命令 | 编辑 | clap 解析 |

Agent Server 侧改动：**零**（现有协议已足够）。

### Phase 3：`[[channels]]` 扩展点 ✅ 已完成

**目标：** 让 PluginManager 管理 Server-side Channel 进程的生命周期。

| 任务 | 位置 | 说明 |
|------|------|------|
| 新增 `src/plugin/channel.rs` | 新建 | ChannelConfig、ChannelManager |
| 编辑 `src/plugin/metadata.rs` | 编辑 | PluginMeta 增加 channels 字段 |
| 编辑 `src/plugin/manager.rs` | 编辑 | collect/spawn/stop 方法 |
| 编辑 `src/plugin/mod.rs` | 编辑 | 导出 channel 模块 |
| 编辑 `src/server.rs` | 编辑 | 启动/关闭时调用 ChannelManager |

### Phase 4：微信插件化封装（待实现）

**目标：** 将 Phase 1 的 WeChat Bridge 打包为标准插件，通过 `[[channels]]` 声明实现开箱即用。

| 任务 | 位置 | 说明 |
|------|------|------|
| 创建 `sample-plugin/wechat-bridge/` | 新建 | 插件目录 |
| `plugin.toml` + `channels.toml` | 新建 | 通道声明 + 配置 |
| `system_prompt.md` | 新建 | 微信场景提示词 |
| 二进制自动下载 | `channel.install` | GitHub Release 下载 |

### 依赖关系

```
Phase 1 (WeChat Bridge)  ──独立──►  可直接使用，不依赖后续 Phase ✅
        │
Phase 2 (./agent connect) ─独立──►  可直接使用，不依赖 Phase 1
        │
Phase 3 ([[channels]])    ◄── Phase 1  通过插件声明管理 Bridge 生命周期 ✅
        │
Phase 4 (Plugin 封装)     ◄── Phase 3  将 Bridge 打包为开箱即用的插件
```

Phase 1 和 Phase 2 可以**并行开发**——互不依赖，分别验证 Server-side 和 Client-side Channel 模式。

---

## 7. 决策记录

| 决策点 | 结论 | 理由 |
|--------|------|------|
| CLI 是否 Channel 化 | ❌ 不做 | CLI 是 Agent 原生界面，同进程，不需要网络 |
| `./agent connect` 是否新建 crate | ❌ 作为子命令 | 大量复用 ui.rs/output.rs/cli.rs |
| 工具代理（Tool Proxy） | ❌ 不做 | 增加大量复杂度（协议扩展、Agent 循环分支、Client 本地执行器），收益有限——用户完全可以在本地运行 Agent 或使用 Web UI 操作远程 Agent |
| 会话模型 | persistent + TTL | 多轮对话体验好，TTL 防止资源泄漏 |
| Channel 二进制分发 | 自动下载（待实现） | 类似 rust-analyzer，从 GitHub Release 下载 |
| WeChat Bridge 语言 | **TypeScript** | `@wechatbot/wechatbot` 是官方 TypeScript SDK；复用 `web-ui/src/types/agent.ts` 协议类型；Rust 生态无成熟微信 SDK |
| WeChat Bridge 在哪 | `bridges/wechat/` 独立 npm 包 | 零侵入 Agent，故障隔离 |
| Bridge 连接模式 | 一个微信用户一条 WebSocket → 一个 worker 进程 | 利用现有 process-per-connection 模型；用户间天然隔离；并发无阻塞 |
| Streaming → 微信 | 聚合模式（默认），可选分片 | 微信不支持真正流式；聚合模式避免刷屏 |
| 先做 Bridge 还是先做 Channels | 先做 Bridge MVP | 验证协议可行性后再投入架构改动 |

---

## 8. 附录：WeChat Bridge 语言选择分析

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
