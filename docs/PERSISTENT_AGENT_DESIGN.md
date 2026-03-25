# Persistent Agent with Task Detachment

## 设计目标

用户的核心体验诉求是：
> "永远有一个 Agent 随时给我服务，长任务不阻塞我继续交互。"

这个设计叫做 **常驻 Agent + 任务分离（Persistent Agent with Task Detachment）**，
工程上属于 **Session Multiplexing（会话复用）** 模式。

---

## 现状痛点

当前 Web UI 是单 Session 线性模型：

```
用户输入 ──▶ Agent 处理 ──▶ 等待结果 ──▶ 用户输入
                │
              阻塞中（输入框 disabled）
              call_node 可能要等 5-10 分钟
```

用户要等 Agent 完全完成，才能发下一条消息。

---

## 目标体验

```
┌─────────────────────────────────────────────────────────┐
│  Task Panel: "训练模型"                    ● 3m12s  [−] │
│  gpu-box > python train.py                              │
│  epoch 3/10  loss=0.412...                              │
├─────────────────────────────────────────────────────────┤
│  Task Panel: "编译 release"                ✓ done   [−] │
│  build-box > cargo build --release ✓                    │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  主对话 (常驻 Agent，始终 ready)                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ > _                           [发送] [新任务]   │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

- 主对话框永远可用，无阻塞
- 每个长任务在独立面板实时显示进度
- 面板可折叠、可关闭（任务仍在后台运行）
- 多个任务并行存在

---

## 架构设计

### 后端：天然支持，无需改动

server.rs 已经对每条 WebSocket 连接独立 `tokio::spawn` 一个 Agent 实例：

```
Web UI                       Backend (server.rs)
  │
  ├── WS #ready ──────────▶  Agent A  (◌ idle，等你说话)
  ├── WS #task-1 ─────────▶  Agent B  (● 训练模型，running)
  └── WS #task-2 ─────────▶  Agent C  (✓ 编译完成，done)
```

每条连接完全隔离，互不干扰。后端 **不需要任何改动**。

### 前端：三个核心模块

#### 1. `useAgentPool` — 连接池 Hook

```typescript
interface AgentSession {
  id: string;
  ws: WebSocket;
  status: 'ready' | 'running' | 'done' | 'error';
  title: string;       // 第一条消息摘要，作为面板标题
  messages: Message[];
  startedAt?: Date;
}

// 核心规则:
// - 始终维护一个 status='ready' 的连接（主对话框用）
// - 当 ready session 开始有 tool_use 事件时，自动"升级"为 Task Panel
// - 升级后立即创建新的 ready session 备用
```

**关键状态机：**

```
ready ──(用户发消息)──▶ thinking
  │                        │
  │              (收到 tool_use 事件)
  │                        │
  │                        ▼
  │                  "升级"为 Task Panel (detach)
  │                  + 新建 ready session
  │
  └──(短问答，无 tool_use)──▶ ready (原地答复，不升级)
```

#### 2. `TaskPanel` — 任务面板组件

```tsx
<TaskPanel
  session={session}          // AgentSession
  onClose={() => detach(id)} // 关闭面板（不中断任务）
/>
```

- 实时流式渲染（复用现有消息渲染逻辑）
- 状态徽章：`● connecting` / `● running (Xm Xs)` / `✓ done` / `✗ error`
- 可折叠（collapsed 时只显示标题和状态徽章）
- 任务完成时自动通知（badge / toast）

#### 3. 升级触发策略

何时把主对话"升级"为独立面板？推荐：**LLM 信号触发**

```
收到 { type: "tool_use" } 事件 → 自动升级
```

- 纯聊天、快速问答：不升级，原地回复
- 涉及 call_node / run_command 等工具：自动升级
- 对用户完全透明，无需手动选择

可选：提供手动按钮 `[作为任务运行]` 强制升级。

---

## 数据流

```
用户在主对话发消息
        │
        ▼
useAgentPool.sendMessage(readySession, text)
        │
        ▼
    WS 发送 user_message
        │
   Backend Agent 处理
        │
  ┌─────┴──────┐
  │            │
(无 tool_use)  (有 tool_use 事件)
  │            │
  │       前端检测到 → detachSession(readySession)
  │            │         → createReadySession()
  │            │
  ▼            ▼
原地渲染     Task Panel 渲染 (独立面板)
主对话继续   主对话已解锁，新 ready session 就位
```

---

## 与 Node 异步执行的关系

这两件事解决的是**不同层级的问题**：

| | 解决什么 | 受益者 |
|---|---|---|
| **Persistent Agent（本文档）** | 用户层面的阻塞感 | 用户 |
| **Node 异步执行（call_node_async）** | 单任务内多节点并发 | LLM / 执行效率 |

**当前阶段**：只做本文档的前端方案即可满足"永远有 Agent 服务"的体验目标，后端零改动。

**未来可选**：当出现"一个任务需要同时调用多个节点"的场景时，再叠加 Node 异步执行方案，二者正交，互不冲突。

---

## 实施计划

### Week 1：核心机制

| 任务 | 工作量 |
|---|---|
| `useAgentPool` hook（连接池 + 状态机） | 1.5天 |
| 升级触发逻辑（监听 tool_use 事件） | 0.5天 |
| `TaskPanel` 组件（流式渲染 + 状态徽章） | 1天 |

### Week 2：完善体验

| 任务 | 工作量 |
|---|---|
| 面板折叠 / 展开 / 关闭 | 0.5天 |
| 任务完成通知（toast / badge） | 0.5天 |
| 主对话 + 面板布局调整 | 0.5天 |
| 测试 & 细节打磨 | 0.5天 |

**总计：约 5 天前端工作，后端零改动。**

---

## 命名说明

这个设计模式的几种叫法：

- **产品层**：多任务管理 / 任务看板
- **工程层**：Session Multiplexing（会话复用）/ Task Detachment（任务分离）
- **完整名称**：Persistent Agent with Task Detachment

类比：
- 类似 iTerm2 的 Session + Tab 管理，但任务是 AI Agent
- 类似 VS Code 的后台任务面板（Background Tasks），但内容是对话流
