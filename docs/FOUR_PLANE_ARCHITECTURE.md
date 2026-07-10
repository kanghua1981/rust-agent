# 四层架构改造设计文档

> 基于知乎文章《你以为在扩能力，其实在堆脆弱性》中提出的 Discovery / Trust / Resource / Execution
> 四层模型，对 Rust Coding Agent 当前架构进行全面诊断与渐进式改造设计。

---

## 目录

1. [问题诊断：脆弱性在哪里](#1-问题诊断脆弱性在哪里)
2. [四层模型概述](#2-四层模型概述)
3. [Discovery Plane 设计](#3-discovery-plane-设计)
4. [Trust Plane 设计](#4-trust-plane-设计)
5. [Resource Plane 设计](#5-resource-plane-设计)
6. [Execution Plane 精简化](#6-execution-plane-精简化)
7. [四层集成与数据流](#7-四层集成与数据流)
8. [分阶段实施计划](#8-分阶段实施计划)
9. [风险与注意事项](#9-风险与注意事项)
10. [验收标准](#10-验收标准)

---

## 1. 问题诊断：脆弱性在哪里

### 1.1 当前系统能力盘点

| 模块 | 当前实现 | 问题 |
|:---|:---|:---|
| **工具注册** | `ToolExecutor::new()` 中 25+ 行 `executor.register(Box::new(...))` 硬编码，工具集编译时决定 | 新增内置工具 = 改代码 + 重编译；LLM 每次拿到全部工具定义 |
| **系统提示词** | `Conversation::new()` 启动时把 summary + skills index + memory knowledge + sub-agents 全部拼进 system prompt，然后 frozen snapshot | 膨胀到 1500+ tokens，大部分 LLM 不总是需要 |
| **信任机制** | `allowed_dir`（目录限制）+ `security.rs`（命令黑名单）+ `tool.before` hook + `ConfirmationLevel` — 分散在三处 | 无统一裁决入口；无法声明 per-role per-capability 策略 |
| **角色隔离** | Pipeline planner/executor/checker 共享同一份 `conversation.system_prompt` 和 memory | "一个角色多拿上下文，另一个角色串味" |
| **资源加载** | Skills/Memory/Summary 全量注入 system prompt，而非按需拉取 | 文章说的"每次还像第一次上岗，得靠 prompt 再解释一遍" |
| **MCP 工具** | 动态加载 OK，但加载后与内置工具混在同一 HashMap，无来源区分 | 无法做来源级信任/卸载管理 |

### 1.2 根因分析

```
当前流程：
  启动 → Conversation::new() 拼装所有上下文 → frozen snapshot → 整坨塞进 LLM

理想流程：
  Intent → Discovery.search() 匹配能力 → Trust.filter() 过滤 → Resource.mount() 挂载 → Execution.run() 执行
```

**核心矛盾**：当前把"发现能力"、"判断可信"、"加载资源"、"执行动作"四件事揉在一起。每加一个工具/角色/MCP server，脆弱性就指数级增长。

---

## 2. 四层模型概述

### 2.1 各层职责

```
┌─────────────────────────────────────────────────────────┐
│                     Discovery Plane                      │
│  "针对这个意图，有哪些工具/Skills/Agents 值得考虑？"      │
│  可查询、可筛选、可维护的 Capability Registry            │
├─────────────────────────────────────────────────────────┤
│                      Trust Plane                         │
│  "搜到的能力中，哪些可信？角色有权调用吗？需要确认吗？"     │
│  统一的 TrustEngine：按角色×能力×参数 裁决               │
├─────────────────────────────────────────────────────────┤
│                     Resource Plane                       │
│  "当前任务需要挂载哪些上下文入口？"                        │
│  ResourceIndex: 标签化的资源目录，按需拉取               │
├─────────────────────────────────────────────────────────┤
│                    Execution Plane                       │
│  "拿着精选的工具和资源，执行"                             │
│  ToolExecutor 只负责运行，不负责发现/信任                │
└─────────────────────────────────────────────────────────┘
```

### 2.2 关键设计原则

1. **能力发现与执行分离** — Discovery 不做执行，Execution 不做发现
2. **显式信任边界** — 每个能力带风险/成本标签，TrustEngine 统一裁决
3. **资源按需挂载** — System prompt 只含角色指令 + 资源目录，具体内容通过工具拉取
4. **向后兼容** — 默认行为保持宽松，严格策略由用户配置

---

## 3. Discovery Plane 设计

### 3.1 核心数据模型

```rust
// src/discovery.rs

/// 能力来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CapabilitySource {
    Builtin,                           // 编译在内的工具
    Plugin { plugin_id: String },       // 插件提供
    Mcp { server_name: String },        // MCP server
    Skill { file: String },             // .agent/skills/*.md
    SubAgent { name: String, url: String },
    Workflow { id: String },            // 预设 workflow
}

/// 风险等级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    ReadOnly = 0,        // 只读操作：read_file, grep_search
    LocalMutation = 1,   // 修改本地文件：write_file, edit_file
    ShellExecution = 2,  // 执行命令：run_command
    NetworkSideEffect = 3, // 网络操作：browser, call_node
    SystemMutation = 4,   // 系统级变更：包安装、权限修改
}

/// 成本等级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CostLevel {
    Cheap,       // 瞬时：think, load_skill
    Moderate,    // 秒级：read_file, grep_search
    Expensive,   // 分钟级：run_command, browser
}

/// 一个可被发现的能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub description: String,
    pub source: CapabilitySource,
    /// 标签：用于意图匹配，如 "file", "shell", "browser", "git", "knowledge", "publish"
    pub tags: Vec<String>,
    pub risk_level: RiskLevel,
    pub cost_level: CostLevel,
    /// 该能力所属的 toolset（与现有 Toolset enum 对应）
    pub toolset: Option<Toolset>,
}

/// 能力注册表
pub struct CapabilityRegistry {
    capabilities: Vec<Capability>,
    /// tags -> capability indices 的倒排索引，加速 search()
    tag_index: HashMap<String, Vec<usize>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        CapabilityRegistry {
            capabilities: Vec::new(),
            tag_index: HashMap::new(),
        }
    }

    /// 注册一个能力
    pub fn register(&mut self, cap: Capability) {
        let idx = self.capabilities.len();
        for tag in &cap.tags {
            self.tag_index.entry(tag.clone()).or_default().push(idx);
        }
        self.capabilities.push(cap);
    }

    /// 按意图检索：基于 tag 和 keyword 匹配
    pub fn search(&self, intent: &str) -> Vec<&Capability> {
        // 实现：对 intent 分词 → 匹配 tag → 匹配 name/description keyword
        // 按匹配度排序返回
        // ...
    }

    /// 按来源过滤
    pub fn by_source(&self, source: &CapabilitySource) -> Vec<&Capability> { /* ... */ }

    /// 按风险上限过滤
    pub fn max_risk(&self, max_risk: RiskLevel) -> Vec<&Capability> { /* ... */ }

    /// 按 toolset 过滤
    pub fn by_toolsets(&self, toolsets: &[Toolset]) -> Vec<&Capability> { /* ... */ }

    /// 获取所有能力的简要描述列表（用于生成 system prompt 中的紧凑目录）
    pub fn to_catalog(&self) -> Vec<(String, String)> { /* ... */ }

    /// 将能力列表转换为 ToolDefinition 列表（给 LLM API 用）
    pub fn to_tool_definitions(&self, caps: &[&Capability]) -> Vec<ToolDefinition> { /* ... */ }
}
```

### 3.2 内置工具的标签标注

| 工具 | tags | risk_level | cost_level |
|:---|:---|:---|:---|
| `read_file` | file, read | ReadOnly | Moderate |
| `write_file` | file, write, mutate | LocalMutation | Moderate |
| `edit_file` | file, write, mutate | LocalMutation | Moderate |
| `multi_edit_file` | file, write, mutate, batch | LocalMutation | Moderate |
| `run_command` | shell, execute | ShellExecution | Expensive |
| `grep_search` | search, file, read | ReadOnly | Moderate |
| `file_search` | search, file, read | ReadOnly | Moderate |
| `list_directory` | file, read, browse | ReadOnly | Cheap |
| `batch_read_files` | file, read, batch | ReadOnly | Moderate |
| `think` | meta, internal | ReadOnly | Cheap |
| `read_pdf` | file, read, document | ReadOnly | Moderate |
| `load_skill` | meta, skill | ReadOnly | Cheap |
| `create_skill` | meta, skill, write | LocalMutation | Cheap |
| `upload_image` | file, read, vision | ReadOnly | Moderate |
| `call_node` | network, delegate, agent | NetworkSideEffect | Expensive |
| `list_nodes` | network, discover | ReadOnly | Cheap |
| `connect_service` | network, service | NetworkSideEffect | Cheap |
| `query_service` | network, service | NetworkSideEffect | Moderate |
| `subscribe_service` | network, service, push | NetworkSideEffect | Moderate |
| `todo_write/update/read` | meta, planning | ReadOnly | Cheap |
| `memory_tool` | meta, memory, knowledge | ReadOnly | Cheap |
| `browser` (feature) | browser, network, web | NetworkSideEffect | Expensive |

### 3.3 改动范围

| 文件 | 改动 | 说明 |
|:---|:---|:---|
| `src/discovery.rs` (新建) | ~200 行 | Capability, CapabilityRegistry, RiskLevel, CostLevel |
| `src/tools/mod.rs` | 修改 | Tool trait 新增 `capability() -> Capability` 方法；ToolExecutor 持有 CapabilityRegistry |
| `src/tools/*.rs` (各个工具) | 修改 | 每个工具实现 `capability()` 返回标注好的元数据 |
| `src/skills.rs` | 修改 | Skills index 注册到 registry |
| `src/plugin/manager.rs` | 修改 | 插件工具加载时注册到 registry |
| `src/mcp_client.rs` | 修改 | MCP 工具发现时注册到 registry |
| `src/router.rs` | 修改 | 分类后调用 `registry.search()` 精选工具 |
| `src/agent/mod.rs` | 修改 | 持有 registry 实例 |

---

## 4. Trust Plane 设计

### 4.1 核心数据模型

```rust
// src/trust.rs

/// 信任裁决结果
#[derive(Debug, Clone)]
pub enum TrustDecision {
    /// 直接允许
    Allow,
    /// 需要人工确认（附带原因）
    Confirm { reason: String },
    /// 拒绝（附带原因）
    Deny { reason: String },
}

impl TrustDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, TrustDecision::Allow)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, TrustDecision::Deny { .. })
    }
}

/// 权限策略：声明式规则集合
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustPolicy {
    /// 角色 → 允许的工具集白名单（空 = 不限制）
    pub role_allow_toolsets: HashMap<String, Vec<String>>,
    /// 角色 → 禁止的工具集黑名单
    pub role_deny_toolsets: HashMap<String, Vec<String>>,
    /// 角色 → 允许的最高风险等级
    pub role_max_risk: HashMap<String, RiskLevel>,
    /// 需要人工确认的工具（全局或 per-role）
    pub require_confirmation: Vec<String>,
    /// 高风险命令特征（迁移自 security.rs 的 DANGEROUS_COMMAND_PATTERNS）
    pub denied_command_patterns: Vec<String>,
    /// 敏感路径后缀（迁移自 security.rs 的 DENIED_PATH_SUFFIXES）
    pub denied_path_suffixes: Vec<String>,
    /// 禁止的目录前缀（迁移自 security.rs 的 DENIED_PREFIXES）
    pub denied_path_prefixes: Vec<String>,
    /// 禁止的能力组合：<(tool_a, tool_b), reason>
    pub deny_combinations: Vec<((String, String), String)>,
}

/// 信任引擎
pub struct TrustEngine {
    policy: TrustPolicy,
    /// 是否允许用户通过确认来覆盖 Deny
    allow_confirm_override: bool,
}

impl TrustEngine {
    pub fn new(policy: TrustPolicy) -> Self {
        TrustEngine {
            policy,
            allow_confirm_override: true,
        }
    }

    /// 裁决：该工具 + 参数在当前角色下是否允许
    pub fn check(
        &self,
        role: &str,
        tool_name: &str,
        risk_level: RiskLevel,
        tool_input: &serde_json::Value,
    ) -> TrustDecision {
        // 1. 检查角色黑名单
        // 2. 检查角色白名单
        // 3. 检查风险等级上限
        // 4. 检查是否需要确认
        // 5. 检查敏感路径（对于 write 类工具）
        // 6. 检查危险命令（对于 run_command 工具）
        // 按优先级返回第一个不通过的结果
    }

    /// 为指定角色构建策略快照
    pub fn for_role(&self, role: &str) -> RoleTrustView { /* ... */ }
}
```

### 4.2 配置化：models.toml 扩展

```toml
# models.toml — Role 配置的扩展字段

[roles.planner]
model = "sonnet"
system_prompt = "..."   # 已有
# 新增 Trust 配置
allow = ["file_read", "search", "think", "skill", "memory"]
deny = ["file_write", "shell", "browser", "agent_comms"]
max_risk = "ReadOnly"
confirm = []

[roles.executor]
model = "sonnet"
allow = []  # 空 = 不限制
deny = []
max_risk = "LocalMutation"
confirm = ["run_command"]  # 执行命令前需确认

[roles.publisher]
model = "sonnet"
allow = ["file_read", "file_write", "shell"]
deny = ["browser"]
max_risk = "LocalMutation"
confirm = ["write_file", "run_command"]

[roles.checker]
model = "mini"
allow = ["file_read", "search", "shell", "think"]
deny = ["file_write", "browser"]
max_risk = "ShellExecution"
confirm = []
```

### 4.3 security.rs 整合

当前 `security.rs` 的函数 `check_file_write_allowed()` 和 `check_dangerous_command()` 在 `ToolExecutor::execute()` 中手动调用。改造后：

- 命令黑名单 `DANGEROUS_COMMAND_PATTERNS` → 迁移到 `TrustPolicy.denied_command_patterns`
- 路径黑名单 `DENIED_PATH_SUFFIXES` / `DENIED_PREFIXES` → 迁移到 `TrustPolicy.denied_path_suffixes/prefixes`
- `check_file_write_allowed()` → 变为 `TrustEngine.check()` 的一个子步骤
- `check_dangerous_command()` → 变为 `TrustEngine.check()` 的一个子步骤
- `security.rs` 保留为兼容层，内部转发到 TrustEngine

### 4.4 改动范围

| 文件 | 改动 | 说明 |
|:---|:---|:---|
| `src/trust.rs` (新建) | ~250 行 | TrustPolicy, TrustEngine, TrustDecision |
| `src/model_manager.rs` | 修改 RoleConfig | 增加 allow/deny/confirm/max_risk 字段 |
| `src/tools/mod.rs` | 修改 | ToolExecutor::execute() 中插入 trust_engine.check() |
| `src/security.rs` | 修改 | 保留接口，内部转发到 TrustEngine |
| `src/agent/mod.rs` | 修改 | Agent 持有 TrustEngine；set_allowed_dir 逻辑移入 |
| `src/pipeline.rs` | 修改 | Planner/Executor/Checker 各自获得 TrustPolicy 快照 |
| `src/worker.rs` | 修改 | 新连接时根据 channel 配置初始化 TrustPolicy |

---

## 5. Resource Plane 设计

### 5.1 核心数据模型

```rust
// src/resources.rs

/// 资源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceType {
    Skill,
    MemoryEntry,
    KnowledgeFact,
    Summary,
    UserProfile,
    ProjectFile { path: String },
    WorkflowTemplate { id: String },
    McpResource { server: String, uri: String },
}

/// 资源寻址方式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceAccess {
    /// 通过已有工具加载：tool_name + 参数
    Tool { tool: String, params: serde_json::Value },
    /// 通过特定函数加载
    Internal { method: String },
}

/// 一个可枚举的资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// 统一标识符: "skill://browser-cdp", "memory://api_keys", "file://Cargo.toml"
    pub uri: String,
    pub name: String,
    pub resource_type: ResourceType,
    /// 一行描述，LLM 据此判断是否需要加载
    pub description: String,
    /// 与该资源相关的标签
    pub tags: Vec<String>,
    /// 如何加载该资源的完整内容
    pub access: ResourceAccess,
}

/// 资源索引
pub struct ResourceIndex {
    resources: Vec<Resource>,
}

impl ResourceIndex {
    pub fn new() -> Self {
        ResourceIndex { resources: Vec::new() }
    }

    pub fn register(&mut self, resource: Resource) { /* ... */ }

    /// 按意图检索相关资源
    pub fn search(&self, intent: &str) -> Vec<&Resource> { /* ... */ }

    /// 生成 system prompt 中的紧凑资源目录（而非嵌入完整内容）
    /// 格式：
    ///   ## Available Resources
    ///   - **Skill "Browser CDP"**: How to implement browser automation...
    ///     → use `load_skill("browser-cdp")`
    ///   - **Memory**: 3 knowledge facts about API patterns
    ///     → use `memory` tool with `read` action
    ///   - **Project File "Cargo.toml"**: Project manifest
    ///     → use `read_file("Cargo.toml")`
    pub fn to_catalog_section(&self) -> String { /* ... */ }

    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}
```

### 5.2 System Prompt 结构改造

```
改造前（~1500 tokens）：
┌─────────────────────────────────────────────┐
│  你是一个编码助手...（基础指令 ~400 tokens）   │
│  ── Project Summary ── (~300 tokens)         │  ← 不一定需要
│  ── Project Skills ── (~200 tokens)          │  ← 只需标签
│  ## Available Skills (full descriptions...)   │
│  ── Project Knowledge ── (~500 tokens)        │  ← 只需标签
│  - Fact 1: ...                                │
│  - Fact 2: ...                                │
│  ── Available Sub-Agents ── (~100 tokens)    │  ← 只需标签
└─────────────────────────────────────────────┘

改造后（~500 tokens）：
┌─────────────────────────────────────────────┐
│  你是一个编码助手...（基础指令 ~400 tokens）   │
│                                              │
│  ## Available Resources (compact catalog)    │
│  - Skills: Browser CDP | Multi-Agent | ...   │
│    → use load_skill("<name>")                │
│  - Memory: 5 knowledge facts                 │
│    → use memory read()                       │
│  - Summary: project architecture overview     │
│    → use read_file(".agent/summary.md")      │
│  - Sub-Agents: node-0 (ws://localhost:9528)  │
│    → use call_node(target="node-0")          │
│                                              │
│  Load detailed context on demand when needed. │
└─────────────────────────────────────────────┘
```

### 5.3 LLM 工作流变化

```
LLM 收到："帮我重构认证模块"

旧流程：
  启动时全量注入 → LLM 浏览长度 system prompt → 开始任务

新流程：
  LLM 看到紧凑目录 → 判断需要哪些上下文 →
    load_skill("multi-agent")     ← 按需
    memory read()                 ← 按需
    read_file("src/auth/mod.rs")  ← 按需
  → 获得精选上下文 → 开始任务
```

**额外轮次成本**：最多增加 2-3 轮工具调用，但 system prompt token 减少 ~1000 tokens。对于典型对话（10+ 轮），净收益为正。对于低轮次对话，可通过缓存已加载的资源来分摊成本。

### 5.4 改动范围

| 文件 | 改动 | 说明 |
|:---|:---|:---|
| `src/resources.rs` (新建) | ~180 行 | Resource, ResourceIndex, to_catalog_section() |
| `src/conversation.rs` | 修改 | `Conversation::new()` 改为调用 `ResourceIndex::to_catalog_section()` 而非内嵌完整内容 |
| `src/skills.rs` | 修改 | `LoadedSkills` 同时产出 `ResourceIndex` 条目 |
| `src/memory/mod.rs` | 修改 | 知识条目注册为 Resource（仅标签，不展开内容） |
| `src/summary.rs` | 修改 | Summary 从直接内嵌改为注册为 Resource |
| `src/agent/mod.rs` | 修改 | Agent 持有 `ResourceIndex`；`reset()` 刷新；缓存按需加载的资源内容 |

---

## 6. Execution Plane 精简化

执行层已经做得不错，本轮改动是**减负**而非增重。

### 6.1 改动范围

| 文件 | 改动 | 说明 |
|:---|:---|:---|
| `src/tools/mod.rs` | 修改 | 删除 `readonly_definitions()` 中的硬编码工具名列表，改为基于 `Capability.risk_level == ReadOnly` 动态过滤 |
| `src/tools/mod.rs` | 修改 | 删除 `ToolExecutor` 中的 `allowed_dir` 直接检查（已移至 TrustEngine 统一裁决） |
| `src/tools/mod.rs` | 修改 | `definitions_for_toolsets()` 改为代理到 `CapabilityRegistry` 查询 |

### 6.2 不变的部分

- `Tool trait` + `ToolDefinition` 保持不变
- `ToolExecutor::execute()` 的缓存、panic 保护、结果截断保留
- `tool.before` / `tool.after` hook 保留（在 TrustEngine 裁决之后触发）
- MCP / Plugin 动态加载链路保留

---

## 7. 四层集成与数据流

### 7.1 改造后的 Agent 主流程（伪代码）

```rust
impl Agent {
    async fn process_user_message(&mut self, task: &str) -> Result<String> {
        // ── Step 1: Discovery — "这个意图需要哪些能力" ────────
        // 由 router 先分类任务复杂度，然后从 registry 检索
        let complexity = classify_heuristic(task);
        let mode: ExecutionMode = complexity.into();
        let capabilities = self.registry.search(task);

        // ── Step 2: Trust — "当前角色能用哪些" ──────────────
        let role = self.current_role_name();
        let trusted: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| {
                let decision = self.trust_engine.check(
                    role, &c.name, c.risk_level, &serde_json::json!({}),
                );
                decision.is_allowed()
            })
            .collect();

        // ── Step 3: Resource — "挂载哪些上下文入口" ────────
        let resources = self.resource_index.search(task);
        let catalog_section = self.resource_index.to_catalog_section();
        // catalog_section 只是紧凑目录，不嵌入完整内容

        // ── Step 4: Execution — "精选工具 + 资源目录 → 跑" ──
        let tool_defs: Vec<ToolDefinition> = trusted
            .iter()
            .map(|c| self.tool_executor.definition_for(&c.name))
            .collect();

        // 构建精简 system prompt（基础角色指令 + 资源目录）
        let effective_prompt = format!(
            "{}\n\n{}",
            self.get_role_system_prompt(role),
            catalog_section,
        );

        self.run_with_tools(tool_defs, &effective_prompt, task).await
    }
}
```

### 7.2 Pipeline 集成

```
用户输入 → Router 分类
              │
              ├─ Simple → 直接 Basic Loop (Discovery + Trust + Execution)
              │
              ├─ Medium → Plan + Execute
              │              │
              │              ├─ Planner: Discovery(ReadOnly) → Trust(Role=planner) → Plan
              │              └─ Executor: Discovery(All) → Trust(Role=executor) → Execute
              │
              └─ Complex → Full Pipeline
                             │
                             ├─ Planner: Discovery(ReadOnly) + Trust(Role=planner)
                             ├─ Executor: Discovery(Write+Shell) + Trust(Role=executor)
                             └─ Checker: Discovery(ReadOnly) + Trust(Role=checker)
```

关键改进：**每个 Pipeline 阶段都有自己的 Trust 快照和 Resource 子集**，不再共享同一份臃肿上下文。

### 7.3 Agent 结构体变化

```rust
pub struct Agent {
    // ... 现有字段 ...

    /// 能力注册表（Discovery Plane）
    pub registry: CapabilityRegistry,

    /// 信任引擎（Trust Plane）
    pub trust_engine: TrustEngine,

    /// 资源索引（Resource Plane）
    pub resource_index: ResourceIndex,

    /// 已按需加载的资源内容缓存
    pub loaded_resources: HashMap<String, String>,

    // ... 其余字段不变 ...
}
```

---

## 8. 分阶段实施计划

### Phase 1：核心骨架（预计 2-3 天）

| 序号 | 任务 | 涉及文件 | 产物 |
|:---|:---|:---|:---|
| P1.1 | 创建 `discovery.rs`：`Capability` / `CapabilityRegistry` / `RiskLevel` / `CostLevel` + `search()` | `src/discovery.rs` (new) | 基础 registry，带 unit tests |
| P1.2 | 创建 `trust.rs`：`TrustPolicy` / `TrustEngine` / `TrustDecision` + `check()` | `src/trust.rs` (new) | 策略引擎，带 unit tests |
| P1.3 | 创建 `resources.rs`：`Resource` / `ResourceIndex` / `to_catalog_section()` | `src/resources.rs` (new) | 资源索引，带 unit tests |
| P1.4 | 扩展 `Tool` trait 增加 `capability()` 方法，每个内置工具实现标注 | `src/tools/*.rs` | 所有工具带 tags/risk/cost |
| P1.5 | `ToolExecutor` 初始化时构建 `CapabilityRegistry` | `src/tools/mod.rs` | 注册 = 填充 registry |
| P1.6 | `Agent` 增加 `registry` / `trust_engine` / `resource_index` 字段，默认宽松策略 | `src/agent/mod.rs` | 编译通过，行为不变 |

### Phase 2：Discovery + Trust 串联（预计 2-3 天）

| 序号 | 任务 | 涉及文件 | 产物 |
|:---|:---|:---|:---|
| P2.1 | Router 调用 `registry.search()` 精选工具子集 | `src/router.rs` | 意图驱动工具筛选 |
| P2.2 | 扩展 `models.toml` RoleConfig，加载 TrustPolicy | `src/model_manager.rs` | 策略可配置 |
| P2.3 | `ToolExecutor::execute()` 中集成 `trust_engine.check()` | `src/tools/mod.rs` | 统一裁决入口 |
| P2.4 | `security.rs` 整合进 TrustEngine（保留兼容接口） | `src/security.rs` + `src/trust.rs` | 安全逻辑一处管理 |
| P2.5 | Plugin / MCP 工具加载时同步注册到 Registry | `src/plugin/manager.rs` + `src/mcp_client.rs` | 动态能力可发现 |

### Phase 3：Resource Plane + System Prompt 瘦身（预计 1-2 天）

| 序号 | 任务 | 涉及文件 | 产物 |
|:---|:---|:---|:---|
| P3.1 | `Conversation::new()` 改为生成紧凑资源目录 | `src/conversation.rs` | < 800 token system prompt |
| P3.2 | Skills / Memory / Summary 注册到 ResourceIndex | `src/skills.rs` + `src/memory/` + `src/summary.rs` | 资源统一可枚举 |
| P3.3 | Pipeline 各阶段使用 Trust 快照 + 独立上下文 | `src/pipeline.rs` + `src/agent/mod.rs` | 角色上下文隔离 |

### Phase 4：文档与测试（预计 1 天）

| 序号 | 任务 |
|:---|:---|
| P4.1 | 全量回归测试：`cargo test` + 手动验证所有模式 |
| P4.2 | 更新 `docs/USER_GUIDE.md` 增加四层架构说明和 Trust 配置示例 |
| P4.3 | 为 `models.toml` 新字段提供迁移示例和错误提示 |

---

## 9. 风险与注意事项

| 风险 | 缓解策略 |
|:---|:---|
| **System prompt 瘦身后 LLM 多轮拉取资源，增加对话轮次** | 初始实现保留高频资源（如核心 memory facts）直接注入；通过 A/B 测试确定最优平衡点 |
| **向后兼容**：现有 `system_prompt.md` 用户自定义被破坏 | 用户自定义 prompt 保持在 system prompt 首部（角色定义），资源目录追加在后，行为完全兼容 |
| **Trust 策略默认值过严破坏已有流程** | 默认策略：`role_max_risk = SystemMutation`（最高），`require_confirmation = []`（空），即与当前行为一致 |
| **Capability tag 标注不准确导致 Discovery 效果差** | 初始标注基于工具功能描述；允许 `.agent/capabilities.toml` 覆盖标注 |
| **新增模块增加编译时间** | 三个新模块各 <300 行，增量可忽略 |
| **Memory frozen snapshot 机制与新 Resource 系统交互** | Frozen snapshot 保留：知识内容仍冻结在 system prompt（保护 prefix cache）；只有 ResourceIndex 的标签是动态的 |

---

## 10. 验收标准

- [ ] `cargo build` 通过，所有现有功能无回归
- [ ] `cargo test` 全部通过
- [ ] 内置工具全部标注 `RiskLevel` / `CostLevel` / `tags`，遗漏工具编译报错
- [ ] System prompt token 数从 ~1500 降至 <800（通过 `--verbose` 验证）
- [ ] Trust 策略可配置：在 `models.toml` 中设置 `[roles.myrole] confirm = ["write_file"]`，写文件时触发确认
- [ ] Discovery 生效：发送"帮我分析 Cargo.toml"时，给 LLM 的工具列表不含 browser/write_file/run_command 等高风险工具
- [ ] 现有 `system_prompt.md` 用户自定义内容仍出现在 system prompt 首部
- [ ] Pipeline planner/executor/checker 各自获得独立 TrustPolicy 快照
- [ ] `security.rs` 现有安全拦截全部由 TrustEngine 承载，不出现重复检查

---

## 附录 A：与现有模块的关系

```
现有模块              改造后关系
──────────────────────────────────────────────
src/tools/mod.rs      → 保留 Tool trait + ToolExecutor，新增 Capability 元数据接口
src/router.rs         → 增加调用 discovery.search() 步骤
src/conversation.rs   → build_system_prompt() 改用 ResourceIndex 紧凑格式
src/security.rs       → 保留为兼容层，内部转发到 TrustEngine
src/pipeline.rs       → 每个阶段获得独立 Trust + Resource 快照
src/agent/mod.rs      → 字段新增 registry/trust_engine/resource_index
src/model_manager.rs  → RoleConfig 扩展 Trust 字段
src/plugin/manager.rs → 工具加载时注册到 CapabilityRegistry
src/mcp_client.rs     → 工具发现时注册到 CapabilityRegistry
src/skills.rs         → 产出 Resource 条目
src/memory/mod.rs     → 产出 Resource 条目
src/summary.rs        → 产出 Resource 条目
```

## 附录 B：与 Google ARD / MCP 的关系

本文的四层模型与 Google 2026 年 6 月公布的 **Agentic Resource Discovery (ARD)** 规范思路一致但不完全等同：

- **ARD** 更关注跨框架的标准化发现协议（类似 MCP 之于资源访问）
- **本文的 Discovery Plane** 是 ARD 的本地实现：不依赖外部标准，在当前代码库内完成意图→能力的检索
- **MCP** 在四层模型中位于 Resource Plane：MCP servers 暴露资源 → ResourceIndex 将其注册为 Resource 条目 → Agent 通过工具按需拉取
- **未来兼容**：当 ARD 成为行业标准时，Discovery Plane 的 CapabilityRegistry 可以成为 ARD 的 local resolver

