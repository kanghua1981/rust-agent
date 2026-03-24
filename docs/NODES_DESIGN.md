# Distributed Nodes 设计文档

> 状态：设计阶段，待实现
> 讨论日期：2026-03-24

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
2. **最小侵入**：主体扩展在 `call_sub_agent` 和 `ready` 握手，不改 agent 核心循环
3. **渐进式**：Phase 1 手动配置即可用，Phase 2 加能力通告，Phase 3 加 mDNS 自动发现
4. **对称性**：每台机器代码完全一样，无特殊 master/slave 编译选项

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

**一个配置文件管所有拓扑**。不论是 CLI 模式还是 server 模式，执行 `call_node` 的始终是一个 agent 进程，不存在"薄客户端"。因此没有理由把"我管哪些工程"和"我能联系谁"分成两个文件。

`workspaces.toml` 是每台机器上 agent 进程的完整拓扑声明：

- **`[[workspace]]`**：本机管理的工程目录，对**入站**连接有效，通过 `ready` 帧告知 manager
- **`[[remote]]`**：可以联系的远端 server，供 `call_node` **出站**寻址
- **`[cluster]`**：集群共享 token
- **不配置此文件** = 通用 agent，行为与现在完全一致，workdir 由连接 URL 参数决定

### 配置文件：`workspaces.toml`

路径：`~/.config/rust_agent/workspaces.toml`（全局）或 `.agent/workspaces.toml`（项目级）

```toml
# 集群共享 token
[cluster]
token = "my-secret-token-123"

# ── 本机管理的工程（对入站连接暴露为虚拟节点）────────────────────────────────
[[workspace]]
name        = "upper-sdk"
workdir     = "/home/user/upper-project"
description = "上位机 SDK 工程（Qt + C++）"
sandbox     = false
tags        = ["upper", "cpp", "qt"]

[[workspace]]
name        = "firmware-bk7236"
workdir     = "/home/user/firmware/bk7236"
description = "BK7236 WiFi 芯片固件"
sandbox     = true
tags        = ["lower", "embedded", "bk7236", "wifi"]

[[workspace]]
name        = "firmware-t23"
workdir     = "/home/user/firmware/t23"
description = "Ingenic T23 ISP 固件"
sandbox     = true
tags        = ["lower", "embedded", "isp", "t23"]

# ── 可联系的远端 server（call_node 出站寻址用）────────────────────────────────
# 虚拟节点详情（workdir/sandbox/tags）从对方 ready 帧自动获取，无需在此手写
[[remote]]
name = "gpu-box"
url  = "ws://192.168.1.20:9527"

[[remote]]
name = "pi"
url  = "ws://raspberrypi.local:9527"
```

> **Phase 3（mDNS）后**：`[[remote]]` 条目也不需要手写，服务器自动发现。
> `workspaces.toml` 最终可能只剩 `[cluster]` + `[[workspace]]` 两部分。

### 工作流程

```
agent --mode server 启动
  → 读取 workspaces.toml
  → 启动时探测所有 workspace 的 bins / caps（一次性，缓存结果）

有连接进来时
  → fork worker，worker 在 ready 帧里携带 virtual_nodes 列表
  → manager 看到列表，LLM 自动理解「这台机器能做什么」
  → manager 可以直接用 name 路由，call_node 自动带上对应 workdir
```

---

## 五、节点能力通告（NodeCapabilities）

### 数据结构

```rust
pub struct NodeCapabilities {
    /// agent 版本
    pub version: String,
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

## 六、节点路由

### ✨ 亮点：一台机器，多个虚拟节点，零额外配置

`agent --mode server` 采用 **process-per-connection** 设计（每个连接 fork 独立进程，各自有独立的工作目录和沙盒），**一台机器可以通过 `workspaces.toml` 声明多个工程目录，manager 无需任何额外配置即可按名字路由**。

服务端只需在 `workspaces.toml` 里写好 `[[workspace]]` 条目（见第四节），调用方在自己的 `workspaces.toml` 里加一条 `[[remote]]` 指向这台 server：

manager 探测后即可并发调用，server 为每个连接独立 fork 进程：

```
manager agent
  → call_node target="firmware-bk7236"  ← fork 进程 A，workdir=/firmware/bk7236
  → call_node target="firmware-t23"     ← fork 进程 B，workdir=/firmware/t23
  （两进程并发，互不干扰）
  ← 汇总结果
```

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
1. 查内存路由表（`/nodes` 探测或上次连接时缓存）→ 找到 workdir / sandbox
2. 未找到 → 报错「未知节点，可在 workspaces.toml [[remote]] 中添加，或直接用 ws:// URL」
3. `target` 为 `ws://...` 直接 URL → 跳过路由表，直连（此时需显式传 workdir）

### 路由策略

| target 格式 | 语义 |
|---|---|
| `<name>` | 精确匹配节点名 |
| `any:<tag>` | 选第一个在线且含该标签的节点 |
| `best:<tag>` | 选含该标签且当前任务数最少的节点（负载均衡）|
| `all:<tag>` | 广播到所有含该标签的节点，并发执行，汇总结果 |
| `ws://...` | 直接 URL，不查配置（兼容现有 `call_sub_agent`）|

---

## 七、通信协议

### 握手流程

完全复用现有 WebSocket `ready` 事件，只扩展 `data` 字段：

```
manager                           worker (agent --mode server)
   |                                  |
   |  WS connect                      |
   |  ws://192.168.1.10:9527/         |
   |  ?workdir=/path&token=xxx        |
   |─────────────────────────────────→|
   |                                  |
   |  ← {"type":"ready","data":{      |
   |       "version":"1.2.0",         |
   |       "workdir":"/home/build",   |  ← URL 参数传入的实际目录
   |       "sandbox": false,          |  ← 实际生效的沙盒状态
   |       "caps":{...},              |  ← 硬件+软件能力
   |       "virtual_nodes":[...]      |  ← workspaces.toml 声明的工程列表
   |     }}                           |
   |                                  |
   |  → {"type":"user_message",       |
   |      "data":{"text":"..."}}      |
   |                                  |
   |  ← 流式事件（现有协议不变）        |
```

### 认证

连接 URL 中携带 token：

```
ws://192.168.1.10:9527/?token=my-secret-token-123&workdir=/path
```

server 端在 HTTP 握手阶段校验，token 不匹配则拒绝连接（返回 401）。

无 token 配置时退化为当前行为（无认证，适合纯本机使用）。

---

## 八、健康检查与故障恢复

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

## 九、可观测性：`/nodes` 命令

`/nodes` 命令对 `workspaces.toml` 中每个 `[[remote]]` 条目发一次探测连接（`?discover=1`），收到 `ready` 帧后立即 close。虚拟节点（来自远端的 workspaces.toml）在物理 server 下缩进展示：

```
🤖 > /nodes

PHYSICAL SERVER          STATUS    ARCH     CAPS
build-server             ✅ 在线   x86_64   gcc,cargo,docker
  └ upper-sdk            workdir=/home/user/upper-project    sandbox=off  [upper,cpp,qt]
  └ firmware-bk7236      workdir=/home/user/firmware/bk7236  sandbox=on   [embedded,bk7236]
  └ firmware-t23         workdir=/home/user/firmware/t23     sandbox=on   [embedded,isp]
gpu-server               ✅ 在线   x86_64   nvcc,python,ollama
  └ (通用，无 workspaces.toml 配置)
pi                       ❌ 离线   aarch64  -
(local)                  ✅ 在线   x86_64   cargo,git

提示：不配置 workspaces.toml 的 server 为通用 agent，调用时需显式传入 workdir
```

---

## 十、mDNS 自动发现（Phase 3）

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

## 十一、实现计划

### Phase 1：静态配置 + 基础委派（✅ 已完成）

- [x] `call_node` 工具替代 `call_sub_agent` / `spawn_sub_agent`
  - `workspaces.toml` 解析（`[[remote]]` 出站列表 + `[cluster]` token，兼容旧 `[[nodes]]`/`[[servers]]`）
  - 参数优先级：显式 > 内存路由表（来自远端 ready 帧）> 服务端默认值
  - `workdir`、`sandbox`、`token` 拼入 WebSocket URL query string
- [x] `connect_service` / `query_service` 描述加警告，避免 LLM 误用
- [x] `ready` 帧加 `workdir` / `sandbox` 字段（worker 回告实际值）
- [x] `call_node` 收到 `ready` 后格式化前置到 ToolResult，告知 manager LLM
- [x] server 端 token 校验（URL 参数，不匹配返回 HTTP 401）
- [x] `/nodes` 斜杠命令（探测各 `[[remote]]` 节点，展示在线状态）

预计剩余代码量：**~150 行**

### Phase 2：能力通告 + 虚拟节点自描述

- [ ] `workspaces.toml` 服务端配置（工程目录声明）
- [ ] server 启动时读取 `workspaces.toml`，探测各 workspace 的 bins / caps
- [ ] `ready` 帧加 `caps`（硬件+软件探测）和 `virtual_nodes` 字段
- [ ] `call_node` 收到 `ready` 后把 `virtual_nodes` 格式化展示给 manager LLM
- [ ] `any:<tag>` / `best:<tag>` / `all:<tag>` 路由逻辑（扫描内存路由表）
- [ ] `/nodes` 命令展示层级结构（物理 server → 虚拟节点）

预计代码量：**~300 行**

### Phase 3：mDNS 自动发现

- [ ] 引入 `mdns-sd` crate
- [ ] server 启动时广播 mDNS（含 `has_workspaces` TXT 记录）
- [ ] manager 后台扫描，发探测连接（`?discover=1`）获取完整能力+虚拟节点
- [ ] 自动填充内存路由表；`/nodes scan` 命令手动触发
- [ ] （可选）`/nodes save` 将发现的节点追加到 `workspaces.toml [[remote]]`

预计代码量：**~200 行**

---

## 十二、与现有功能的关系

| 现有功能 | 关系 |
|---|---|
| `call_sub_agent` | `call_node` 的基础，直接封装复用 |
| `spawn_sub_agent` | 本机进程级委派，Nodes 是其网络级升级版 |
| `connect_service` | 通用 WS 连接，Nodes 是 agent-aware 的特化版本 |
| `--mode server` | 每个 node 的运行模式，**不需要改动** |
| `StdioOutput` JSON 协议 | Nodes 通信的事件格式，**直接复用** |
| `workspaces.toml` | 统一配置文件：`[[workspace]]`（入站）+ `[[remote]]`（出站）+ `[cluster]` token |

---

## 十三、和 OpenClaw 的兼容机会

完成 Phase 1 后，rust-agent 既可以作为独立集群运行，也可以通过以下方式接入 OpenClaw 生态：

1. **方案 C（Skill）**：写一个 `SKILL.md` + `tool.json`，让 OpenClaw 通过 `call_node` 把任务委派给 rust-agent 集群
2. **方案 B（Bridge）**：实现 OpenClaw node 握手协议（Ed25519 签名），让 rust-agent 直接注册为 OpenClaw 的一个 "AI Node"，在 `node.invoke` 时触发完整 LLM 循环

两个方案不冲突，可以先做方案 C，后续再做方案 B。
