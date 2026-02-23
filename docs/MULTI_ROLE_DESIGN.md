# Multi-Role Agent 架构设计文档

> 设计目标：在现有 `Agent` 框架基础上，扩展支持用户自定义角色（如规划者、实施者、审核者），
> 每个角色绑定独立的模型与系统提示词，并通过标准化的**上下文传递协议**串联为一条自动流水线。

---

## 目录

1. [核心概念与术语](#1-核心概念与术语)
2. [信息流设计：角色间如何传递上下文](#2-信息流设计角色间如何传递上下文)
3. [系统提示词设计（每个角色的 Prompt）](#3-系统提示词设计每个角色的-prompt)
4. [配置文件格式（models.toml 扩展）](#4-配置文件格式modelstoml-扩展)
5. [核心数据结构设计](#5-核心数据结构设计)
6. [执行流程（状态机）](#6-执行流程状态机)
7. [错误处理与自愈机制](#7-错误处理与自愈机制)
8. [对现有代码的影响分析](#8-对现有代码的影响分析)
9. [分阶段实施计划](#9-分阶段实施计划)
10. [未解决的设计问题（待决策）](#10-未解决的设计问题待决策)

---

## 1. 核心概念与术语

| 术语 | 说明 |
|:---|:---|
| **Role（角色）** | 一个具有独立模型配置和系统提示词的执行单元，如 `planner`、`executor`、`checker` |
| **Stage（阶段）** | 流水线中的一个执行节点，每个 Stage 对应一个 Role |
| **Artifact（制品）** | 一个 Stage 产出的结构化结果，作为下一个 Stage 的输入 |
| **Pipeline（流水线）** | 多个 Stage 按顺序串联的完整执行流程 |
| **Handoff（交接）** | 上一个 Stage 将 Artifact 注入下一个 Stage 的过程 |
| **Feedback Loop（反馈环）** | Checker 发现问题后将反馈传回 Executor 重试的闭环 |

---

## 2. 信息流设计：角色间如何传递上下文

这是整个架构中最关键的问题：**每个角色的输入来自哪里，输出传递给谁？**

### 2.1 三角色标准流水线

```
用户输入 (User Task)
       │
       ▼
┌─────────────┐    Artifact: Plan
│   Planner   │ ──────────────────────────────────────────────────────────────►
│  (规划者)   │
└─────────────┘
  只读工具: read_file, list_dir, grep_search
  输入: 用户原始任务
  输出: 结构化的执行计划 (Markdown)

                                           ┌──────────────┐    Artifact: Result
                                           │  Implementer │ ────────────────────►
                                           │   (实施者)   │
                                           └──────────────┘
                                             全量工具: read/write/edit/cmd
                                             输入: [用户任务 + 规划结果]
                                             输出: 修改摘要 + 影响文件列表

                                                                  ┌───────────┐
                                                                  │  Checker  │
                                                                  │  (审核者) │
                                                                  └───────────┘
                                                                   只读+运行工具
                                                                   输入: [用户任务
                                                                         + 规划结果
                                                                         + 修改摘要
                                                                         + 文件Diff]
                                                                   输出: 通过 ✅ 
                                                                         or 问题报告 ❌
                                                                         
                              ◄──────────────── 反馈环 (问题报告 → Executor 重试) ──────────
```

### 2.2 Artifact 格式规范

每个 Stage 必须输出符合以下格式的 Artifact，供下一个 Stage 解析。
格式选择 Markdown（可读性好，LLM 天然理解）而非 JSON（省去解析开销）。

#### Planner → Implementer：Plan Artifact

```markdown
## PLAN_ARTIFACT

### 任务摘要
<用户原始任务的一句话概括>

### 受影响的文件
- `src/agent.rs` — 添加 planner_config 字段
- `src/model_manager.rs` — 扩展 roles 字段解析
- `src/config.rs` — 新增 with_role_config 方法

### 执行步骤
1. **[读]** 阅读 `src/agent.rs` 第 1-50 行，确认 Agent 结构体当前字段
2. **[改]** 在 `src/agent.rs` 中为 `Agent` 结构体新增 `role_configs: HashMap<String, Config>` 字段
3. **[改]** 修改 `Agent::new()` 构造函数，从 `models.toml` 加载 roles 配置
4. **[运行]** 执行 `cargo build` 验证编译通过
5. **[改]** 修改 `generate_plan()` 使用 planner 角色的 Config 调用 LLM
6. **[运行]** `cargo test` 验证测试通过

### 风险提示
- 步骤 2 会改变 `Agent::new()` 的签名，需同时修改 `cli.rs` 和 `server.rs` 中的调用点
- 如 `roles.planner` 未配置，应降级（fallback）使用主模型，不能报错中断

### 成功标准
- `cargo build --release` 无 error
- 现有 tests 全部通过
- `/plan` 命令正常工作且调用的是 planner 指定的模型
```

#### Implementer → Checker：Result Artifact

```markdown
## RESULT_ARTIFACT

### 完成的步骤
- ✅ 步骤 1：阅读了 src/agent.rs
- ✅ 步骤 2：新增了 role_configs 字段
- ✅ 步骤 3：修改了 Agent::new()
- ✅ 步骤 4：cargo build 成功
- ⚠️ 步骤 5：generate_plan() 已修改，但 server.rs 中的构造函数调用尚未更新（留作 TODO）
- ✅ 步骤 6：cargo test 通过（3 个测试）

### 修改的文件
- `src/agent.rs`（+45行/-3行）
- `src/model_manager.rs`（+28行/-0行）

### 未完成事项
- `server.rs` 中的 `Agent::with_conversation()` 调用尚未传入 role_configs

### 命令执行记录
- `cargo build`: 成功（0 errors, 2 warnings）
- `cargo test`: 成功（3 passed, 0 failed）
```

#### Checker → User / Implementer：Review Artifact

```markdown
## REVIEW_ARTIFACT

### 总体评定
PASS ✅ / FAIL ❌ / PARTIAL ⚠️

### 发现的问题
#### 问题 1 [严重 🔴]
- 位置: `src/server.rs` 第 45 行
- 描述: `Agent::with_conversation()` 调用缺少新增的 `role_configs` 参数
- 建议: 传入从 `models.toml` 加载的默认 role_configs

#### 问题 2 [轻微 🟡]
- 位置: `src/agent.rs` `generate_plan` 函数
- 描述: 当 planner role 未配置时缺少 fallback 逻辑
- 建议: 添加 `self.role_configs.get("planner").unwrap_or(&self.config)` 形式的降级

### 确认符合要求的部分
- ✅ Agent 结构体字段正确添加
- ✅ models.toml 解析逻辑完整

### 决定
FAIL — 需要 Implementer 修复问题 1 后重新提交
```

---

## 3. 系统提示词设计（每个角色的 Prompt）

系统提示词决定了角色的"人格"和行为模式。

### 3.0 提示词自定义机制

角色提示词采用**三层优先级**，与现有 `system_prompt.md` 的分层覆盖机制完全一致，
用户无需学习新规则：

```
层级 1（最低）：内置默认提示词（Rust 硬编码，见 3.1-3.3）
     ↓ 追加 / # OVERRIDE 替换
层级 2：~/.config/rust_agent/roles/<role>.md   （全局级，适用所有项目）
     ↓ 追加 / # OVERRIDE 替换
层级 3：.agent/roles/<role>.md                 （项目级，只影响当前项目）
     ↓ 追加 / # OVERRIDE 替换
层级 4（最高）：models.toml [roles.xxx] 内联字段（适合短小补充）
```

**文件格式**：沿用现有 `# OVERRIDE` 标记，不另起炉灶：
```markdown
<!-- .agent/roles/executor.md -->

# OVERRIDE
<!-- 加此行则完全替换内置提示词；不加则追加到内置末尾 -->

你是一名专注的 Rust 工程师。
严格遵守 clippy 规范，每次修改后运行 cargo clippy。
```

**模板变量**：提示词中可嵌入运行时变量，Agent 在注入前展开：

| 变量 | 说明 | 典型用途 |
|:---|:---|:---|
| `{{project_dir}}` | 当前工作目录绝对路径 | 全部角色 |
| `{{task}}` | 用户原始任务描述 | 全部角色 |
| `{{plan}}` | Planner 产出的 PLAN_ARTIFACT 全文 | Executor, Checker |
| `{{result}}` | Executor 产出的 RESULT_ARTIFACT 全文 | Checker |
| `{{attempt}}` | 当前是第几次重试（从 1 开始） | Executor |

使用示例：
```markdown
<!-- .agent/roles/checker.md -->
你是一名代码审查员，当前任务是：{{task}}

重点核查以下成功标准是否达成：
{{plan}}
```

**代码实现**：新增 `build_role_system_prompt(role, project_dir, vars)` 函数，
复用 `Conversation::build_system_prompt` 中已有的文件扫描 + OVERRIDE 解析逻辑，
最后做一次 `{{变量}}` 字符串替换。

---

### 3.1 Planner（规划者）

**核心目标**：深思熟虑，消除歧义，产出精确可执行的计划。

```
你是一名软件架构师和技术规划专家。

**你的职责**：
- 在开始任何行动之前，彻底理解用户需求和现有代码库
- 使用只读工具（read_file, list_directory, grep_search）探索代码库
- 识别所有可能受影响的文件和模块
- 评估修改的风险和依赖关系
- 输出一份结构清晰、步骤原子化的执行计划

**你的限制**：
- 禁止调用任何写入或修改类工具（write_file, edit_file, run_command）
- 你的输出必须包含 `## PLAN_ARTIFACT` 段落，格式见规范
- 步骤必须足够具体，让不了解背景的 Executor 也能无歧义地执行

**行为准则**：
- 如果需求不清晰，在 Plan 中明确写出你的假设
- 如果某个步骤有多种实现方案，列出方案并给出推荐理由
- 风险标注是必填项，不能省略
- 宁愿步骤过细，也不要过于概括
```

### 3.2 Implementer（实施者）

**核心目标**：严格按照计划执行，如实记录偏差，不擅自扩大修改范围。

```
你是一名专注的软件工程师。

**你的职责**：
- 严格按照 PLAN_ARTIFACT 中列出的步骤逐一执行
- 使用全量工具完成代码修改、文件创建和命令执行
- 遇到障碍时优先尝试自我修复，修复后继续执行
- 每完成一个步骤，在内部记录完成状态

**你的限制**：
- 不得修改计划范围之外的文件（除非是明显的编译依赖）
- 执行完成后，输出必须包含 `## RESULT_ARTIFACT` 段落，格式见规范
- 如果某个步骤因为技术原因无法完成，在 RESULT_ARTIFACT 中如实记录，不要虚报成功

**行为准则**：
- 每次修改文件前，先读取目标文件确认上下文
- 修改后立即执行编译（cargo build / python -m pytest 等）验证没有引入新错误
- 如果发现 Plan 中存在明显的遗漏（如少写了一个依赖文件），可以补充修改，但必须在 RESULT_ARTIFACT 中说明
```

### 3.3 Checker（审核者）

**核心目标**：独立验证，不受实施者的"自我报告"影响，用批判性视角审查结果。

```
你是一名严格的代码审查员和质量保证工程师。

**你的职责**：
- 阅读 PLAN_ARTIFACT 了解原始意图
- 阅读 RESULT_ARTIFACT 了解实施者的自述
- 独立审查实际修改的文件内容（通过 read_file 读取，不要只看实施者的报告）
- 运行测试命令验证功能正确性
- 检查计划的每一项成功标准是否得到满足

**你的限制**：
- 禁止修改任何文件（只读 + 可运行测试命令）
- 输出必须包含 `## REVIEW_ARTIFACT` 段落，格式见规范
- 评定结果只能是：PASS / FAIL / PARTIAL

**行为准则**：
- 不要相信 RESULT_ARTIFACT 中的成功报告，要自己亲自读文件验证
- 对每个"严重问题"必须给出具体的修复建议，而不仅仅是描述问题
- 如果测试命令返回失败，这是 FAIL，不能标记为 PARTIAL
- 评审时重点关注：
  (1) 是否引入了新的编译错误或运行时错误
  (2) 是否完成了 Plan 中的所有成功标准
  (3) 是否有明显的安全问题或资源泄漏
  (4) 实施者是否修改了计划范围之外的文件
```

### 3.4 自定义角色示例

用户可以通过以下任意方式自定义角色提示词（优先级从低到高）：

**方式一：项目级文件（推荐用于复杂提示词）**
```markdown
<!-- .agent/roles/security_officer.md -->
你是一名应用安全专家（APPSEC）。
你的职责是在代码合并前审查每一处修改，识别：
- SQL 注入 / 命令注入风险
- 敏感信息硬编码
- 不安全的文件权限
只读权限，输出必须包含安全评分（0-10）和风险列表。
```

**方式二：`models.toml` 内联（适合短小补充）**
```toml
[roles.security_officer]
model = "opus"
extra_instructions = """
本项目使用 PostgreSQL，重点检查 SQL 拼接和参数化查询的使用是否正确。
"""
```

**方式三：`system_prompt` 字段完全覆盖（适合完全自定义）**
```toml
[roles.security_officer]
model = "opus"
system_prompt = """
你是一名应用安全专家... （完整提示词）
"""
```

---

## 4. 配置文件格式（models.toml 扩展）

### 4.1 完整配置示例

```toml
# ~/.config/rust_agent/models.toml

# 主模型（无角色时使用）
default = "sonnet"

# ── 角色映射 ────────────────────────────────────────────────────────────────
# pipeline 字段定义默认三角色流水线。用户可以增减角色，或完全自定义。
# stages 的执行顺序即为列表顺序。

[pipeline]
enabled = false            # 设为 true 后，所有用户输入自动走多角色流水线，无需任何新命令
stages = ["planner", "executor", "checker"]
max_checker_retries = 2    # Checker FAIL 后允许 Executor 重试的最大次数
require_plan_confirm = true  # Planner 完成后暂停，展示计划，等待用户确认后再执行

# ── 角色定义 ─────────────────────────────────────────────────────────────────

[roles.planner]
model = "sonnet"           # 引用 [models.sonnet] 配置
# system_prompt = ""       # 留空则使用内置默认提示词
extra_instructions = """
优先关注 Rust 所有权规则和借用检查器可能引发的问题。
"""

[roles.executor]
model = "deepseek"
extra_instructions = """
编写 Rust 代码时严格遵守 clippy 规范，执行修改后运行 cargo clippy 检查。
"""

[roles.checker]
model = "sonnet"
extra_instructions = """
重点检查 cargo build 和 cargo test 是否通过。
"""

# ── 可选自定义角色 ────────────────────────────────────────────────────────────
[roles.security_officer]
model = "opus"
system_prompt = """
你是一名应用安全专家... （见上文完整示例）
"""

# ── 模型定义 ──────────────────────────────────────────────────────────────────

[models.sonnet]
provider = "anthropic"
model = "claude-sonnet-4-5"
api_key = "sk-ant-..."     # 可选，优先于环境变量

[models.deepseek]
provider = "compatible"
model = "deepseek-coder-v2"
base_url = "https://api.deepseek.com"
api_key = "sk-..."

[models.opus]
provider = "anthropic"
model = "claude-opus-4"
```

### 4.2 字段说明

| 字段 | 类型 | 必填 | 说明 |
|:---|:---|:---|:---|
| `pipeline.enabled` | `bool` | 否 | 是否默认开启流水线，默认 false |
| `pipeline.stages` | `[String]` | 否 | 按顺序列出角色名，必须都在 `[roles]` 中定义 |
| `pipeline.max_checker_retries` | `u32` | 否 | 默认 2 |
| `pipeline.require_plan_confirm` | `bool` | 否 | Planner 结束后是否等待用户确认，默认 true |
| `roles.<name>.model` | `String` | 是 | 引用 `[models.xxx]` 的键名 |
| `roles.<name>.system_prompt` | `String` | 否 | 完全替换内置默认提示词（同 `# OVERRIDE` 效果） |
| `roles.<name>.extra_instructions` | `String` | 否 | 追加到最终提示词末尾（优先级最高） |

### 4.3 提示词加载顺序（以 executor 为例）

```
① 内置 EXECUTOR_DEFAULT_PROMPT (hardcoded)
② ~/.config/rust_agent/roles/executor.md  （若存在，追加或 OVERRIDE）
③ .agent/roles/executor.md               （若存在，追加或 OVERRIDE）
④ models.toml [roles.executor].system_prompt  （若非空，完全替换①②③的结果）
⑤ 模板变量展开：{{task}}, {{plan}}, {{attempt}} 等
⑥ models.toml [roles.executor].extra_instructions  （追加到末尾）
```

---

## 5. 核心数据结构设计

### 5.1 新增结构体

```rust
// src/model_manager.rs 中新增

/// 单个角色的配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    /// 引用的模型别名（对应 [models.xxx]）
    pub model: String,
    /// 完全自定义的系统提示词（留空使用内置默认）
    pub system_prompt: Option<String>,
    /// 追加到系统提示词末尾的额外指令
    pub extra_instructions: Option<String>,
}

/// 流水线配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    pub enabled: bool,
    pub stages: Vec<String>,              // 角色名列表，顺序即执行顺序
    pub max_checker_retries: Option<u32>,
    pub require_plan_confirm: Option<bool>, // 默认 true
}

// models.toml 顶层结构扩展
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsConfig {
    pub default: Option<String>,
    pub models: BTreeMap<String, ModelEntry>,
    pub roles: BTreeMap<String, RoleConfig>,   // 新增
    pub pipeline: Option<PipelineConfig>,       // 新增
}
```

```rust
// src/agent.rs 中 Agent 结构体扩展

pub struct Agent {
    pub config: Config,           // 主模型配置（无角色时使用）
    role_configs: HashMap<String, Config>,  // 新增：角色名 -> Config 的映射
    // ... 其他字段不变
}
```

### 5.2 Artifact 在代码层面的表示

```rust
// src/pipeline.rs (新文件)

/// 规划阶段的产出
pub struct PlanArtifact {
    pub summary: String,
    pub affected_files: Vec<String>,
    pub steps: Vec<String>,
    pub risks: Vec<String>,
    pub success_criteria: Vec<String>,
    pub raw_markdown: String,  // 原始 markdown，注入给 Executor 用
}

/// 实施阶段的产出
pub struct ResultArtifact {
    pub completed_steps: Vec<(String, bool)>,  // (步骤描述, 是否完成)
    pub modified_files: Vec<String>,
    pub pending_items: Vec<String>,
    pub command_outputs: Vec<(String, String)>,  // (命令, 输出)
    pub raw_markdown: String,
}

/// 审核阶段的产出
pub enum ReviewVerdict {
    Pass,
    Fail(Vec<ReviewIssue>),
    Partial(Vec<ReviewIssue>),
}

pub struct ReviewArtifact {
    pub verdict: ReviewVerdict,
    pub issues: Vec<ReviewIssue>,
    pub raw_markdown: String,
}

pub struct ReviewIssue {
    pub severity: IssueSeverity,  // Critical / Warning / Info
    pub location: Option<String>,
    pub description: String,
    pub suggestion: String,
}
```

---

## 6. 执行流程（状态机）

```
                   ┌────────────────────────────┐
                   │   用户输入（普通对话）       │
                   └──────────┬─────────────────┘
                              │
              ┌───────────────▼──────────────────────┐
              │  pipeline.enabled = true?             │
              │  是 → 走多角色流水线                   │
              │  否 → 走现有单模型 process_message     │
              └───────────────┬──────────────────────┘
                              │ 是
                              ▼
              ┌────────────────────────┐
              │  Stage: Planner        │
              │  - 加载只读工具        │
              │  - 设置 planner 系统   │
              │    提示词              │
              │  - 调用 planner 模型   │
              │  - 等待 PLAN_ARTIFACT  │
              └──────────┬─────────────┘
                         │ PLAN_ARTIFACT 产出
                         ▼
              ┌────────────────────────┐
              │  [展示 Plan 给用户]    │◄── pipeline.require_plan_confirm = true
              │  "是否执行此计划？"    │    输入 y 执行，或输入意见让 Planner 重规划
              └──────────┬─────────────┘
                         │ 确认 / 自动通过
                         ▼
         ┌───────────────────────────────────┐
         │  Stage: Executor (第 N 次尝试)    │
         │  - 加载全量工具                   │
         │  - 注入上下文：                   │
         │      系统提示词(executor角色)     │
         │    + PLAN_ARTIFACT(规划结果)      │
         │    + 上轮 REVIEW_ARTIFACT(若有)  │    ◄── 反馈环入口
         │  - 调用 executor 模型             │
         │  - 等待 RESULT_ARTIFACT           │
         └──────────────┬────────────────────┘
                        │ RESULT_ARTIFACT 产出
                        ▼
         ┌───────────────────────────────────┐
         │  Stage: Checker                   │
         │  - 只读工具 + 运行命令            │
         │  - 注入上下文：                   │
         │      系统提示词(checker角色)      │
         │    + PLAN_ARTIFACT               │
         │    + RESULT_ARTIFACT             │
         │  - 调用 checker 模型             │
         │  - 等待 REVIEW_ARTIFACT          │
         └──────────────┬────────────────────┘
                        │
          ┌─────────────┼────────────────┐
          ▼             ▼                ▼
        PASS          PARTIAL           FAIL
          │             │                │
          │       重试次数 < max?        │
          │         是 │    否           │
          │            │     │           │
          ▼            ▼     ▼           ▼
       完成 ✅    回 Executor  报告给用户  报告给用户
                  重试 🔄    用户决定 ⚠️  用户决定 ❌
```

---

## 7. 错误处理与自愈机制

### 7.1 Executor 自愈（单角色内部）
Executor 在执行步骤时如果遭遇错误（如编译失败），应尝试在同一对话内自我修复，
这是现有 `process_message` 主循环已经支持的行为，无需额外设计。

### 7.2 Checker 反馈后 Executor 重试
```
REVIEW_ARTIFACT: FAIL
  → 创建新的 Executor Conversation（不复用旧对话，避免混乱）
  → 注入 PLAN_ARTIFACT + RESULT_ARTIFACT + REVIEW_ARTIFACT（完整上下文）
  → 告知 Executor："审核者发现了以下问题，请针对性修复"
  → 重新执行，产出新的 RESULT_ARTIFACT
  → 重新进入 Checker Stage
  → 如果达到 max_checker_retries，停止并报告给用户
```

### 7.3 Planner 产出无效时的降级
如果 Planner 未产出包含 `## PLAN_ARTIFACT` 的标准格式：
- 尝试解析非标准文本，提取步骤列表
- 如解析失败，提示用户 Plan 格式不符合预期，并展示原始文本
- 用户可以选择：重新规划 / 直接用原始文本继续执行 / 取消

### 7.4 角色未配置时的降级（Graceful Degradation）
如果用户没有配置 `[roles.planner]`：
- Planner Stage 使用主模型 (`config`) 和内置 planner 系统提示词
- 在 UI 上提示："使用默认模型作为 Planner（未配置专用模型）"

---

## 8. 对现有代码的影响分析

| 文件 | 改动类型 | 改动内容 |
|:---|:---|:---|
| `src/model_manager.rs` | **扩展** | 新增 `RoleConfig`, `PipelineConfig`, 扩展 `ModelsConfig` |
| `src/config.rs` | **扩展** | 新增 `Config::for_role()` 方法，从 `RoleConfig` 构造 Config |
| `src/agent.rs` | **扩展** | 新增 `role_configs: HashMap<String, Config>` 字段；新增 `call_llm_as_role()` 方法；修改 `generate_plan()` 使用 planner 角色 |
| `src/pipeline.rs` | **新建** | `PipelineRunner` 结构体，封装三角色流水线逻辑 |
| `src/cli.rs` | **轻微扩展** | `/model` 命令增加角色配置状态展示；`/plan run` 静默切换为 executor 角色模型；**无新命令** |
| `src/streaming.rs` | **轻微扩展** | `stream_anthropic_response` 已接受 `&Config`，**无需改动** |
| `src/conversation.rs` | **无需改动** | `Conversation` 已支持自定义 `system_prompt`，可直接使用 |
| `src/server.rs` | **轻微修改** | Agent 构造时传入 role_configs |

**关键洞察**：  
现有的 `generate_plan()` 方法已经使用了独立的 `plan_conversation`（独立对话对象），  
这个设计为多角色架构奠定了完美的基础。每个角色都使用独立的 `Conversation` 对象，  
既不污染主对话，又可以注入不同的系统提示词。

---

## 9. 分阶段实施计划

### 第一阶段：配置层（低风险，可独立合并）
1. 扩展 `ModelsConfig`，支持 `[roles]` 和 `[pipeline]` 字段的解析
2. 在 `Config` 中添加 `Config::for_role()` 工厂方法
3. 在 `Agent::new()` 中加载 role_configs（不影响现有行为）

### 第二阶段：双模型（Planner + Executor，不引入 Checker）
1. `Agent` 新增 `call_llm_as_role(role: &str, ...)` 方法
2. 修改 `generate_plan()` 使用 planner 角色模型
3. 修改 `execute_plan()` 使用 executor 角色模型
4. 此时 `/plan` 命令已经可以享受双模型分离的好处

### 第三阶段：三角色流水线（引入 Checker）
1. 新建 `src/pipeline.rs`，定义 Artifact 数据结构
2. 实现 `PipelineRunner`：封装三角色流程 + 反馈环
3. 在 `process_message` 入口处检测 `pipeline.enabled`，自动分发到 `PipelineRunner`（**对用户完全透明，无新命令**）
4. 在 `require_plan_confirm = true` 时，Planner 结束后调用 `output.confirm()` 等待用户确认

### 第四阶段：用户体验打磨
1. 在 UI 层展示"当前执行角色"（如 `🧠 Planner [claude-sonnet]` → `⚙️ Executor [deepseek]`）
2. Token 使用统计按角色分类展示
3. `/model` 命令扩展：显示当前流水线配置状态（`pipeline: ✅ 已启用 / ❌ 已禁用`）
4. 完善文档，在 `models.toml` 中提供开箱即用的三角色配置示例

---

## 10. 未解决的设计问题（待决策）

### Q1：Checker 的 Conversation 是否应该看到 Executor 的全部工具调用历史？

**选项 A**：只把 RESULT_ARTIFACT（摘要）给 Checker。
- 优点：Token 消耗少，Checker 更专注
- 缺点：Checker 无法看到执行细节，可能遗漏问题

**选项 B**：把 Executor 的完整对话历史（含所有工具调用）注入给 Checker。
- 优点：Checker 信息最全，审查最严格
- 缺点：Token 消耗极大，可能超出上下文窗口

**推荐**：使用"中间策略"—— 注入 RESULT_ARTIFACT + 被修改文件的当前内容（通过 read_file 读取），
而不是 Executor 的对话历史。

---

### Q2：用户是否应该在 Planner 输出后有机会"编辑计划"？

**推荐**：是的，通过 `pipeline.require_plan_confirm = true` 配置项控制（无需新命令）。
开启后，Planner 输出 Plan 后 Agent 暂停并展示完整计划，用户直接输入 `y` 确认执行，或输入任意文字作为修改意见（Agent 将意见传回 Planner 重新规划后再继续）。

---

### Q3：多角色流水线在 Server/Stdio 模式下如何处理？

流水线运行时需要"中间状态"的展示（如 Planner 结束，等待用户确认），
这需要在 `AgentOutput` trait 中增加 `on_stage_complete(stage: &str, artifact: &str)` 事件，
让 `StdioOutput` 和 `WsOutput` 能正确序列化给外部消费者（如 VS Code 插件）。

---

*文档版本：v0.3 — 补充提示词自定义机制（三层优先级 + 模板变量），同步 PipelineConfig 字段*
