# Distributed Nodes 设计文档

> 状态：架构设计完成，编码中
> 讨论日期：2026-03-24 / 2026-03-25

---

## 一、背景与定位

### 与 OpenClaw Nodes 的本质区别

OpenClaw 的 Nodes 机制是 **"有 AI 的主机控制没有 AI 的哑终端"**：

```
OpenClaw Gateway (有AI) → Node (没有AI，只会执行 system.run / camera / GPS)
```

rust-agent 的 Nodes 机制是 **"AI 指挥 AI"**：

```
Agent (有AI，有判断力) → Agent (也有AI，理解任务、自主决策、可再次委派)
```

每台机器地位对等，都跑 `agent --mode server`。谁接受用户输入，谁就是当前的 manager。角色是**任务维度**的，不是机器维度的。

### 典型用例

```
用户: "把这个项目在 x86 和 ARM 上各编译测试一遍"

manager-agent (本机):
  → call_node target="build-x86"  "编译并运行测试，返回结果"  ← 并发
  → call_node target="build-arm"  "编译并运行测试，返回结果"  ← 并发
  ← 汇总两台机器的结果，生成报告

用户: "训练这个模型"
manager-agent:
  → call_node target="any:gpu"  "用 train.py 训练，报告最终 loss"
```

链式委派也天然支持：

```
A (manager) → B (worker) → C (sub-worker)
```

---

## 二、核心设计原则

1. **零协议新增**：复用现有 `--mode server` WebSocket + StdioOutput JSON 事件格式，不引入新协议
2. **最小侵入**：主体扩展在 `call_node` 和 `ready` 握手，不改 agent 核心循环
3. **渐进式**：Phase 1 手动配置即可用，Phase 2 加能力通告 + 自动 probe，Phase 3 加 mDNS 自动发现
4. **对称性**：每台机器代码完全一样，无特殊 master/slave 编译选项
5. **Server 是目录服务，不是代理**：server 负责聚合所有节点信息，sub-agent 向本机 server 查询节点，拿到真实 URL 后**直连**目标节点，server 不做任何流量转发

---

## 三、工作目录（workdir）问题

### 现有机制

server 已通过 URL 参数接收 workdir：

```
ws://192.168.1.10:9527/?workdir=/home/build/myapp
```

`server.rs` 在握手时从 HTTP 头中解析，设为 worker 的工作目录。

### 跨机器的三种场景

**场景 1：同一份代码，不同机器（最常见）**

```
A: /home/kanghua/myapp   (本地开发机)
B: /home/build/myapp     (CI 构建机，同一个 git 仓库)
```

解法：worker 在 `ready` 握手帧里广播自己的 workdir，manager 看到后 LLM 自行决策：

```json
{
  "type": "ready",
  "data": {
    "version": "1.2.0",
    "workdir": "/home/build/myapp"
  }
}
```

**场景 2：任务自带路径**

prompt 里直接写了路径（如 "编译 ~/projects/foo"），worker LLM 自行处理，**已经能工作**。

**场景 3：文件不在远程机，需要同步**

通过 prompt 指示 worker 先同步：

```
"先 git pull origin main，然后 cargo build --release"
```

或 manager 先用 `run_command` 做 rsync，再委派任务。

### 实现方案

在 worker 发出 `ready` 事件时附带 `workdir`。`call_sub_agent` 收到后在 `ToolResult` 前缀中告知 manager LLM：

```
Sub-agent connected.
  Remote node: build-server (192.168.1.10:9527)
  Remote workdir: /home/build/myapp
  Capabilities: gpu, cargo, docker
```

LLM 基于这些信息自主决定是否需要路径适配。

---

## 四、唯一配置文件：`workspaces.toml`

### 设计思路

**一个配置文件管所有拓扑**，两种键类型职责严格分离：

- **`[[node]]`**：本机可调用节点，必须有 `workdir`。LLM 可以直接 `call_node target="<name>"` 调用。
- **`[[peer]]`**：对等 agent server 的入口地址，必须有 `url`。**只有 server 进程能看到**，LLM 永远不感知这个配置。server 启动时 probe 对方，把展开的子节点以 `name@alias` 形式写入注册表，LLM 通过 `list_nodes` 看到的是展开后的节点，而非原始 `[[peer]]` 条目。
- **`[cluster]`**：集群共享 token
- **不配置此文件** = 通用 agent，行为与现在完全一致

### 配置文件：`workspaces.toml`

路径：`~/.config/rust_agent/workspaces.toml`（全局）或 `.agent/workspaces.toml`（项目级）

```toml
# 集群共享 token
[cluster]
token = "my-secret-token-123"

# ── 本机节点：每个 [[node]] = 一个可调用节点，LLM 可直接 call_node ──────────
[[node]]
name        = "upper-sdk"
workdir     = "/home/user/upper-project"
description = "上位机 SDK 工程（Qt + C++）"
sandbox     = false
tags        = ["upper", "cpp", "qt"]

[[node]]
name        = "firmware-bk7236"
workdir     = "/home/user/firmware/bk7236"
description = "BK7236 WiFi 芯片固件"
sandbox     = true
tags        = ["lower", "embedded", "bk7236", "wifi"]

# ── 对等机器：每个 [[peer]] = 另一台 agent server，只有 server 进程感知 ──────
# server 启动时 probe 对方，子节点以 name@alias 展开，LLM 看展开结果不看这里
[[peer]]
name = "gpu-box"
url  = "ws://192.168.1.20:9527"

[[peer]]
name = "pi"
url  = "ws://raspberrypi.local:9527"
```

> **Phase 3（mDNS）后**：`[[peer]]` 条目也不需要手写，服务器自动发现局域网内的 peer。

---

## 五、Server 作为本地目录服务（核心架构）

### 原则：查询与连接分离

```
Sub-agent
  Step 1: list_nodes → localhost:9527/nodes        ← 只向本机 server 查目录
            └─ 拿到完整节点列表，每条含真实 URL

  Step 2: call_node("firmware-bk7236")
            └─ 直接连 ws://localhost:9527/?workdir=/home/user/firmware/bk7236
               （本机节点 → 经本机 server fork worker）

  Step 3: call_node("模型训练@gpu-box")
            └─ 直接连 ws://192.168.1.20:9527/?workdir=/data/model
               （远端节点 → 点对点直连远端 server，本机 server 不参与流量）
```

**本机 server 不做任何 WS 代理，角色等同于 DNS / 服务注册表。**

### 本机 server 的节点注册表（NodeRegistry）

```
节点名                  真实 URL                                          来源    标签
──────────────────────────────────────────────────────────────────────────────────────
upper-sdk              ws://localhost:9527/?workdir=/home/user/upper      local   [upper,cpp]
firmware-bk7236        ws://localhost:9527/?workdir=/home/user/fw/bk7236  local   [embedded]
模型训练@gpu-box        ws://192.168.1.20:9527/?workdir=/data/model        remote  [gpu]
数据集@gpu-box          ws://192.168.1.20:9527/?workdir=/datasets          remote  [gpu]
```

`gpu-box` 这个配置条目本身**不出现在注册表**，只有 probe 展开后的子节点出现。

### 三种节点注册时机

**① 启动时（startup probe）**
- 本机节点：直接注入，无 IO
- 远端节点：并发对每个 `url` 发起 `/probe`，收到 `ready` 帧后展开子节点写入注册表，标记 `online`
- 远端不可达：标记 `offline`，不阻塞启动

**② 定时轮询（periodic retry）**
- `offline` 节点每 30s 重试一次 probe
- `online` 节点每 120s 心跳确认，超时降级为 `offline`

**③ 消息驱动（on-demand re-probe）**
- `call_node` 遇到 `offline` 节点立即触发一次 probe
- 成功：更新注册表，继续调用；失败：返回清晰错误

### 节点命名规则：`name@alias`

- **本机节点**：名字直接来自配置 `name` 字段，无后缀
- **远端子节点**：`{子节点name}@{远端entry的name}`，例如 `模型训练@gpu-box`
- **名字冲突**：本机节点不加后缀，远端子节点强制加 `@alias`
- **一跳原则**：只展开直接配置的远端节点，不递归，避免拓扑爆炸

### sub-agent 防止自连接

- `list_nodes`：只查 `localhost:{AGENT_PARENT_PORT}/nodes`，从不直接访问远端 server 的 `/nodes`
- `call_node` 连 `localhost:{PARENT_PORT}` 且无 workdir → 自连接检测，立即报错
- `call_node` 连 `localhost:{PARENT_PORT}?workdir=/other/path` → 合法（不同工程的本机节点）

### Server 互探的节点隔离（双 Endpoint 设计）

**问题**：A、B 互相配置了对方为 `[[peer]]`。A 探测 B 时，如果 B 返回自己的 `/nodes`（其中包含 A 的节点），A 就会把自己的节点重新注册一遍，形成节点污染。

**解决方案**：用两个职责不同的 endpoint，物理隔离"互探"和"查目录"：

| Endpoint | 调用方 | 返回内容 |
|---|---|---|
| `ws://{host}/probe` | 其他 Server（互探） | **只返回本机 local_nodes()**，即本机 workdir 下的节点 |
| `http://{host}/nodes` | 子 Agent 的 `list_nodes` 工具 | 完整注册表：local + 远端探测展开的 `name@alias` |

`/probe` endpoint 使用 `probe_capabilities(ws_cfg.local_nodes())` 构建响应，永远不包含通过远端探测得到的节点。因此：

```
A 探测 B  →  B 只返回 B 自己的本机节点
B 探测 A  →  A 只返回 A 自己的本机节点
```

**两者注册到各自注册表时，都加了 `@alias` 后缀**，也不会同名冲突。

**额外约束**：`/nodes` HTTP endpoint 最终不应暴露原始 `[[peer]]` 的远端网关配置条目（即 `source: "static"` 的 raw URL 条目）。这些是 server 内部路由配置，对子 Agent 无意义，且可能被误用为直连目标。Phase 2 完成后，`/nodes` 只返回 local 节点 + `name@alias` 形式的展开节点，去掉 `source: "static"` 条目。

---

## 六、节点能力通告（NodeCapabilities）

### 数据结构

```rust
pub struct NodeCapabilities {
    /// 当前连接的工作目录（URL 参数传入）
    pub workdir: String,
    /// 是否在沙盒模式下运行
    pub sandbox: bool,
    /// 硬件信息（启动时自动探测）
    pub hardware: HardwareInfo,
    /// 软件能力（探测 + 配置）
    pub software: SoftwareInfo,
    /// 本 server 声明的虚拟节点列表（来自 workspaces.toml，无配置时为空）
    pub virtual_nodes: Vec<VirtualNodeInfo>,
}

pub struct VirtualNodeInfo {
    pub name: String,
    pub workdir: String,
    pub description: String,
    pub sandbox: bool,
    pub tags: Vec<String>,
    /// 该目录下探测到的关键 bin
    pub bins: Vec<String>,
}

pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub ram_gb: f32,
    pub gpus: Vec<GpuInfo>,        // 探测 nvidia-smi / rocm-smi
    pub arch: String,              // x86_64 / aarch64 / ...
    pub os: String,                // linux / macos / windows
}

pub struct GpuInfo {
    pub name: String,
    pub vram_gb: f32,
}

pub struct SoftwareInfo {
    /// 当前注册的工具名列表
    pub tools: Vec<String>,
    /// 配置的模型别名列表
    pub models: Vec<String>,
    /// PATH 上存在的关键 bin（探测列表见下）
    pub bins: Vec<String>,
}
```

### virtual_nodes 注入 manager LLM 的格式

`call_node` 收到 `ready` 帧后，把 `virtual_nodes` 格式化后前置到 ToolResult，让 manager LLM 完整感知远端能力：

```
Node '192.168.1.10:9527' connected.
  Arch: x86_64 / linux   Caps: gcc, make, git, openocd

  Virtual nodes on this server (use name directly in call_node):
  ┌─────────────────────┬───────────────────────────┬─────────┬──────────────────────┐
  │ name                │ workdir                   │ sandbox │ description          │
  ├─────────────────────┼───────────────────────────┼─────────┼──────────────────────┤
  │ upper-sdk           │ /home/user/upper-project  │ off     │ 上位机 SDK（Qt+C++）  │
  │ firmware-bk7236     │ /home/user/firmware/bk7236│ on      │ BK7236 WiFi 固件      │
  │ firmware-t23        │ /home/user/firmware/t23   │ on      │ Ingenic T23 ISP 固件  │
  └─────────────────────┴───────────────────────────┴─────────┴──────────────────────┘
```

此后 manager LLM 可以直接说 `call_node target="firmware-bk7236"`，无需手动指定 workdir。

### 自动探测的 bin 列表

```
编译/构建:  cargo, rustc, gcc, clang, make, cmake, ninja, bazel
容器/部署:  docker, podman, kubectl, helm
AI/ML:      python3, pip, nvcc, nvidia-smi, rocm-smi, ollama
工具链:     node, npm, go, java, mvn, gradle
系统工具:   git, rsync, ssh, ffmpeg, convert (ImageMagick)
```

### 探测逻辑（启动时一次性运行）

```rust
fn probe_bins(candidates: &[&str]) -> Vec<String> {
    candidates.iter()
        .filter(|bin| which::which(bin).is_ok())
        .map(|s| s.to_string())
        .collect()
}

fn probe_gpus() -> Vec<GpuInfo> {
    // 尝试 nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
    // 尝试 rocm-smi --showmeminfo vram
    // 失败则返回空 Vec
}
```

---

## 七、节点路由

### `call_node` 工具参数

```json
{
  "target": "build-server",          // 节点名，直接寻址
  "target": "any:gpu",               // 路由：选任意含 gpu 标签的节点
  "target": "all:arm",               // 路由：广播到所有 arm 节点
  "target": "ws://1.2.3.4:9527",    // 直接 URL，向后兼容

  "prompt": "编译并运行测试",
  "workdir": "/home/build/myapp",    // 可选，覆盖节点配置中的 workdir
  "sandbox": true,                   // 可选，覆盖节点配置中的 sandbox 默认值
  "auto_approve": true,
  "timeout_secs": 600
}
```

### 连接参数拼装

`call_node` 内部会把工具参数组装成 WebSocket 连接 URL 的 query string，服务端在 HTTP Upgrade 握手阶段（`peek` 不消费字节）解析：

```
ws://192.168.1.10:9527/?workdir=%2Fhome%2Fbuild%2Fmyapp&sandbox=1&token=my-secret
```

参数来源优先级（高 → 低）：

| 参数 | 优先级 | 来源 |
|---|---|---|
| `call_node` 调用时显式指定 | 最高 | LLM 决策 |
| 内存路由表中的虚拟节点信息 | 次高 | 服务端 `workspaces.toml` 通过 `ready` 帧获取 |
| 服务端启动时的 `--sandbox` 标志 | 兜底 | 服务端默认值 |

`call_node target="firmware-bk7236"` 的解析顺序：
1. 查本机 server 注册表（`GET /nodes` 返回所有已展开节点）→ 找到真实 URL
2. 未找到 → 报错「未知节点，请先用 list_nodes 查看可用节点」
3. 目标节点 `offline` → 立即触发 re-probe，成功则继续，失败则报错
4. `target` 为 `ws://...` 直接 URL → 跳过注册表，直连（bypass 所有路由逻辑）

### 路由策略

| target 格式 | 语义 |
|---|---|
| `<name>` | 精确匹配节点名 |
| `any:<tag>` | 选第一个在线且含该标签的节点 |
| `best:<tag>` | 选含该标签且当前任务数最少的节点（负载均衡）|
| `all:<tag>` | 广播到所有含该标签的节点，并发执行，汇总结果 |
| `ws://...` | 直接 URL，不查配置（兼容现有 `call_sub_agent`）|

---

## 八、通信协议

系统中存在三类独立协议，职责不同，不能混用：

| 协议 | 参与方 | 传输 | 目的 |
|---|---|---|---|
| **目录同步协议** | Server ↔ Server | WebSocket `/probe` | 互探，获取对方本机节点列表 |
| **节点获取协议** | Sub-agent → 本机 Server | HTTP `GET /nodes` | 子 agent 获取可调用的完整节点目录 |
| **任务委派协议** | Agent → Worker | WebSocket `/` | 向具体节点委派任务，传输消息与结果 |

---

### 协议一：目录同步协议（Server ↔ Server）

**触发时机**：本机 server 启动时、定时轮询时、on-demand re-probe 时。  
**方向**：A server 主动发起，B server 被动响应。**两者不做反向查询**，不递归。

```
Server-A                              Server-B
   |                                     |
   |  WS connect  ws://B:9527/probe      |
   |─────────────────────────────────────→|
   |                                     |
   |  ← {"type":"ready","data":{         |   ← B 只返回 B 本机 local_nodes()
   |       "version":"1.2.0",            |
   |       "workdir": "",                |   ← 空（/probe 端口无 workdir 概念）
   |       "caps":{...},                 |   ← B 机整体硬件/软件能力
   |       "virtual_nodes":[             |   ← B 本机所有 workdir 节点
   |         {"name":"模型训练",          |
   |          "workdir":"/data/model",   |
   |          "tags":["gpu"]},           |
   |         ...                         |
   |       ]                             |
   |     }}                              |
   |                                     |
   |  WS close（Server-A 主动关闭）       |
   |─────────────────────────────────────→|
```

**关键约束**：
- `/probe` 响应**只包含本机 `local_nodes()`**，绝不包含从第三方 server 探测到的节点
- A 探测 B，B 返回 B 自己的；B 探测 A，A 返回 A 自己的 → 不会交叉污染
- Server-A 收到响应后，把 B 的节点展开写入本机 NodeRegistry，命名为 `{node}@{B的alias}`
- **认证**：连接 URL 携带 token `ws://B:9527/probe?token=xxx`，token 不匹配返回 401

---

### 协议二：节点获取协议（Sub-agent → 本机 Server）

**触发时机**：子 agent 执行 `list_nodes` 工具时。  
**方向**：子 agent 只查询 **`localhost:{AGENT_PARENT_PORT}/nodes`**，永远不直接查询远端 server 的 `/nodes`。

```
Sub-agent                          Local Server (localhost:9527)
   |                                     |
   |  HTTP GET /nodes                    |
   |─────────────────────────────────────→|
   |                                     |
   |  ← HTTP 200  application/json       |
   |    {                                |
   |      "nodes": [                     |
   |        {                            |   ── 本机节点（直接可用）
   |          "name": "upper-sdk",       |
   |          "url":  "ws://localhost:9527/?workdir=/home/user/upper",
   |          "source": "local",         |
   |          "tags": ["cpp","upper"]    |
   |        },                           |
   |        {                            |   ── 远端展开节点（直连目标）
   |          "name": "模型训练@gpu-box", |
   |          "url":  "ws://192.168.1.20:9527/?workdir=/data/model",
   |          "source": "remote",        |
   |          "status": "online",        |
   |          "last_seen": "2026-03-25T10:00:00Z"
   |        }                            |
   |      ]                              |
   |    }                                |
```

**关键约束**：
- 返回的是 NodeRegistry 快照，**不含** `[[peer]]` 原始网关配置条目（Phase 2 完成后去掉 `source: "static"` 条目）
- 子 agent 拿到 URL 后**直连目标**，本机 server 不参与后续流量转发
- offline 节点也出现在列表中（带 `"status":"offline"`），由 `call_node` 决定是否触发 re-probe

---

### 协议三：任务委派协议（Agent → Worker）

完全复用现有 WebSocket 协议，连接 `/`（非 `/probe`）：

```
Manager Agent                      Worker (forked by Server)
   |                                  |
   |  WS connect                      |
   |  ws://192.168.1.10:9527/         |
   |  ?workdir=/path&token=xxx        |
   |─────────────────────────────────→|
   |                                  |
   |  ← {"type":"ready","data":{      |   ← 实际执行环境确认
   |       "version":"1.2.0",         |
   |       "workdir":"/home/build",   |
   |       "sandbox": false,          |
   |       "caps":{...},              |
   |       "virtual_nodes":[...]      |
   |     }}                           |
   |                                  |
   |  → {"type":"user_message",       |
   |      "data":{"text":"..."}}      |
   |                                  |
   |  ← 流式事件（现有协议不变）        |
```

**认证**：连接 URL 携带 token，server 在 HTTP 握手阶段校验，不匹配返回 401。无 token 配置时无认证（适合纯本机使用）。

---

### LLM 可见边界（关键约束）

LLM 只能通过工具间接触碰协议一和协议二，**绝不能直接调用内部管理 endpoint**。

```
LLM 可见                           LLM 不可见
─────────────────────────────────  ──────────────────────────────────
list_nodes 工具                    /probe  （目录同步，Server 内部）
call_node  工具                    /nodes  （由 list_nodes 工具代理）
                                   /health
                                   /metrics
```

**防护措施（已在代码中实现）：**

1. **工具注册表隔离**：`/probe`、`/nodes` 不注册为工具，LLM 的 tool-use 调用链里根本看不到它们。
2. **call_node 路径拦截**：即使 LLM 构造出 `ws://host:port/probe` 的直连 URL，`call_node` 会在执行前拦截并返回错误：
   ```
   '/probe' is a server-internal management endpoint and cannot be used as a task target.
   Use `list_nodes` to discover available agent nodes...
   ```
   被拦截的路径列表：`/probe`、`/nodes`、`/health`、`/metrics`。
3. **工具描述不暴露 endpoint 细节**：`list_nodes` 和 `call_node` 的 description 字段只描述业务语义，不提及 HTTP/WebSocket endpoint 路径。

**为什么 LLM 会"自作主张"调 /probe？**

- LLM 在推理"如何发现节点"时，可能从系统提示、memory 或对话历史里拼凑出 `/probe` 的存在
- 若 `call_node` 没有路径拦截，LLM 构造 `ws://localhost:9527/probe` 并调用，会收到一个 `ready` 帧——LLM 可能误以为这是一个正常的节点响应，进而用错误的 URL 继续操作
- 根本原则：**内部协议的存在对 LLM 完全透明（它感知不到），不依赖 LLM 的"自觉不调用"来保证安全**

---

## 九、健康检查与故障恢复

### Heartbeat

- 现有 `call_sub_agent` 已有 15s ping 间隔 + 180s 无响应自动断开
- 节点配置加 `last_seen` 时间戳，`/nodes` 命令显示在线状态

### 故障恢复

- `any:<tag>` 路由自动跳过离线节点，选下一个
- `all:<tag>` 路由某个节点失败时，在结果中标记该节点错误，不影响其他节点
- 任务超时时返回已收集到的部分结果 + 超时错误

### 优雅下线

节点收到 `SIGTERM` 时：
1. 停止接受新连接
2. 等待当前任务完成（最多 60s）
3. 发送 `{"type":"shutdown"}` 给所有连接的 manager
4. 退出

---

## 十、可观测性：`/nodes` 命令与 `list_nodes` 工具

`/nodes`（CLI 斜杠命令）和 `list_nodes`（sub-agent 工具）都查询本机 `GET /nodes` 端点，展示本机注册表的完整视图：

```
🤖 > /nodes

本机节点 (local)
  upper-sdk         ws://localhost:9527/?workdir=...  sandbox=off  [upper,cpp,qt]
  firmware-bk7236   ws://localhost:9527/?workdir=...  sandbox=on   [embedded,bk7236]

远端节点 via gpu-box (ws://192.168.1.20:9527)  ✅ 在线  x86_64
  模型训练@gpu-box   ws://192.168.1.20:9527/?workdir=/data/model    [gpu,python]
  数据集@gpu-box     ws://192.168.1.20:9527/?workdir=/datasets       [gpu]

远端节点 via pi (ws://raspberrypi.local:9527)  ❌ 离线
  （上次在线：2026-03-25 14:32，正在重试...）

提示：使用 call_node target="<名字>" 调用节点
```

---

## 十一、mDNS 自动发现（Phase 3）

### 广播

每个 `agent --mode server` 启动时注册 mDNS 服务：

```
服务类型: _rust-agent._tcp.local
端口: 9527（或实际端口）
TXT 记录:
  name=build-server
  version=1.2.0
  tags=x86,cargo,docker
  token_hint=<token前4位，用于快速匹配>
```

### 发现

manager 后台定期扫描，自动将局域网内的节点加入候选列表（需 token 匹配才信任）。

### 依赖

```toml
# Cargo.toml
mdns-sd = "0.11"   # 约 10KB，纯 Rust，无系统依赖
```

---

## 十二、实现计划

### Phase 1：静态配置 + 基础委派（✅ 已完成）

- [x] `call_node` 工具（替代 `call_sub_agent` / `spawn_sub_agent`）
- [x] `workspaces.toml` 解析（仅支持 `[[node]]` 语法）
- [x] `ready` 帧加 `workdir` / `sandbox` / `caps` / `virtual_nodes` 字段
- [x] server 端 token 校验（URL 参数，不匹配返回 HTTP 401）
- [x] `GET /nodes` HTTP 端点（返回本机注册表 JSON）
- [x] `list_nodes` 工具（sub-agent 专用，查父 server 的 `/nodes`）
- [x] sub-agent 自连接检测（无 workdir 时拒绝回连父 server）

### Phase 2：Server 作为目录服务 + 自动 Probe ✅ 已完成

- [x] **Server 启动时 probe 所有 peer**
  - 对每个 `[[peer]]` 的 `PeerEntry` 并发发起 `/probe`
  - 从 `ready` 帧提取 `virtual_nodes`，展开为 `name@alias` 写入注册表（`registry_update_peer`）
  - 不可达节点标记 `offline`，不阻塞启动
- [x] **定时轮询**（`spawn_probe_loop`）
  - `offline` 节点：每 30s 重试 probe
  - `online` 节点：每 120s 心跳确认
- [x] **消息驱动 re-probe**（`GET /reprobe?peer=<name>`）
  - `call_node` 遇到 `offline` 节点时立即触发一次 probe，10s 超时
- [x] **`GET /nodes` 返回完整注册表**（`registry_snapshot()`；含本机节点 + 展开的远端子节点，不含原始远端条目）
- [x] **`/nodes` 命令 / `list_nodes` 工具** 展示新格式（本机节点 + 远端子节点分组 + 在线状态）

实际代码量：**~380 行**（`workspaces.rs` +120，`server.rs` +230，`call_node.rs` +55，`list_nodes.rs` 重写）

### Phase 3：mDNS 自动发现

- [ ] 引入 `mdns-sd` crate
- [ ] server 启动时广播 mDNS（含 `has_nodes` TXT 记录）
- [ ] 自动填充注册表；`/nodes scan` 命令手动触发
- [ ] （可选）`/nodes save` 将发现的节点追加到 `workspaces.toml`

预计代码量：**~200 行**

---

## 十三、与现有功能的关系

| 现有功能 | 关系 |
|---|---|
| `connect_service` | 通用 WS 连接，Nodes 是 agent-aware 的特化版本 |
| `--mode server` | 每个 node 的运行模式，**不需要改动** |
| `StdioOutput` JSON 协议 | Nodes 通信的事件格式，**直接复用** |
| `workspaces.toml` | 统一配置文件：`[[node]]`（本机/远端）+ `[cluster]` token |

---

## 十四、和 OpenClaw 的兼容机会

完成 Phase 1 后，rust-agent 既可以作为独立集群运行，也可以通过以下方式接入 OpenClaw 生态：

1. **方案 C（Skill）**：写一个 `SKILL.md` + `tool.json`，让 OpenClaw 通过 `call_node` 把任务委派给 rust-agent 集群
2. **方案 B（Bridge）**：实现 OpenClaw node 握手协议（Ed25519 签名），让 rust-agent 直接注册为 OpenClaw 的一个 "AI Node"，在 `node.invoke` 时触发完整 LLM 循环

两个方案不冲突，可以先做方案 C，后续再做方案 B。
