# 🤖 Rust Coding Agent — 使用指南

> 一个用 Rust 编写的 AI 编码助手 CLI 工具。它可以读写文件、执行命令、搜索代码，并通过 LLM 进行智能交互。

---

## 目录

- [🤖 Rust Coding Agent — 使用指南](#-rust-coding-agent--使用指南)
  - [目录](#目录)
  - [🚀 快速开始](#-快速开始)
  - [📦 安装](#-安装)
    - [从源码编译](#从源码编译)
    - [安装到系统路径（可选）](#安装到系统路径可选)
  - [⚙️ 配置](#️-配置)
    - [API Key](#api-key)
    - [模型管理 (models.toml)](#模型管理-modelstoml)
    - [环境变量参考](#环境变量参考)
  - [🖥️ 启动与运行模式](#️-启动与运行模式)
    - [CLI 交互模式（默认）](#cli-交互模式默认)
    - [Stdio 模式（脚本集成）](#stdio-模式脚本集成)
    - [WebSocket 服务器模式](#websocket-服务器模式)
  - [💬 日常使用](#-日常使用)
    - [基本对话](#基本对话)
    - [多轮迭代](#多轮迭代)
    - [文件操作](#文件操作)
    - [代码搜索](#代码搜索)
    - [命令执行](#命令执行)
    - [配合 Git 使用](#配合-git-使用)
  - [📋 斜杠命令速查](#-斜杠命令速查)
  - [🔄 模型切换](#-模型切换)
    - [查看当前模型](#查看当前模型)
    - [运行时切换](#运行时切换)
    - [添加新模型](#添加新模型)
    - [设置默认模型](#设置默认模型)
    - [通过 CLI 参数指定](#通过-cli-参数指定)
  - [� 执行模式切换](#-执行模式切换)
    - [查看当前模式](#查看当前模式)
    - [切换执行模式](#切换执行模式)
    - [执行模式说明](#执行模式说明)
    - [自适应路由](#自适应路由)
  - [📝 Plan 模式（先分析后执行）](#-plan-模式先分析后执行)
    - [`/plan` 斜杠命令（手动）](#plan-斜杠命令手动)
    - [自动 Pipeline（Planner → Executor → Checker）](#自动-pipelineplanner--executor--checker)
      - [流程概览](#流程概览)
      - [计划审核](#计划审核)
      - [执行前注入背景（approve 时）](#执行前注入背景approve-时)
      - [执行中随时打断（Ctrl+\\）](#执行中随时打断ctrl)
  - [📜 会话管理](#-会话管理)
    - [列出历史会话](#列出历史会话)
    - [恢复会话](#恢复会话)
    - [手动保存](#手动保存)
  - [🧠 记忆与项目摘要](#-记忆与项目摘要)
    - [持久记忆](#持久记忆)
    - [项目摘要](#项目摘要)
  - [📚 Skills 系统](#-skills-系统)
    - [目录结构](#目录结构)
    - [AGENT.md 示例](#agentmd-示例)
    - [Skill 文件示例](#skill-文件示例)
    - [查看已加载的 Skills](#查看已加载的-skills)
  - [✏️ 自定义系统提示词](#️-自定义系统提示词)
  - [🔒 安全与确认机制](#-安全与确认机制)
    - [需要确认的操作](#需要确认的操作)
    - [确认交互](#确认交互)
    - [跳过确认](#跳过确认)
    - [无需确认的操作](#无需确认的操作)
  - [🛡️ 沙盒模式（文件保护与回滚）](#️-沙盒模式文件保护与回滚)
    - [启用方式](#启用方式)
      - [方式一：服务器启动时全局开启（推荐 Web UI 用户）](#方式一服务器启动时全局开启推荐-web-ui-用户)
      - [方式二：CLI 模式逐次开启](#方式二cli-模式逐次开启)
      - [方式三：Web UI 逐连接开启](#方式三web-ui-逐连接开启)
    - [隔离后端](#隔离后端)
    - [Overlay 后端架构](#overlay-后端架构)
    - [扩展工具链（`extra_binds`）](#扩展工具链extra_binds)
    - [沙盒命令（CLI）](#沙盒命令cli)
      - [`/changes` — 查看改动](#changes--查看改动)
      - [`/rollback` — 撤销全部改动](#rollback--撤销全部改动)
      - [`/commit` — 提交改动](#commit--提交改动)
    - [Web UI 沙盒面板](#web-ui-沙盒面板)
    - [典型工作流](#典型工作流)
    - [故障排查](#故障排查)
  - [🤖 多 Agent 协作（多进程专家池）](#-多-agent-协作多进程专家池)
    - [为什么需要多 Agent？](#为什么需要多-agent)
    - [两种子 Agent 调用方式](#两种子-agent-调用方式)
      - [1. `call_sub_agent` - 连接预启动的 WebSocket 服务器](#1-call_sub_agent---连接预启动的-websocket-服务器)
      - [2. `spawn_sub_agent` - 动态创建 stdio 子进程](#2-spawn_sub_agent---动态创建-stdio-子进程)
    - [工具参数说明](#工具参数说明)
      - [`call_sub_agent` 参数：](#call_sub_agent-参数)
      - [`spawn_sub_agent` 参数：](#spawn_sub_agent-参数)
    - [透明度与安全](#透明度与安全)
  - [🧰 内置工具一览](#-内置工具一览)
    - [外部依赖（可选）](#外部依赖可选)
  - [🏷️ CLI 参数速查](#️-cli-参数速查)
  - [📂 目录结构约定](#-目录结构约定)
    - [用户配置目录 (`~/.config/rust_agent/`)](#用户配置目录-configrust_agent)
    - [项目级目录 (`.agent/`)](#项目级目录-agent)
  - [❓ 常见问题](#-常见问题)
    - [Q: API Key 报错 "not set"](#q-api-key-报错-not-set)
    - [Q: 如何使用国内 API（通义千问、DeepSeek 等）？](#q-如何使用国内-api通义千问deepseek-等)
    - [Q: 上下文太长，API 报错](#q-上下文太长api-报错)
    - [Q: 想让 Agent 用中文回复](#q-想让-agent-用中文回复)
    - [Q: 如何让 Agent 了解项目规范](#q-如何让-agent-了解项目规范)
    - [Q: 如何恢复昨天的对话](#q-如何恢复昨天的对话)
    - [Q: 切换模型会丢失对话吗？](#q-切换模型会丢失对话吗)

---

## 🚀 快速开始

```bash
# 1. 编译
cargo build --release

# 2. 配置 API Key（二选一）
export ANTHROPIC_API_KEY=sk-ant-xxxxx       # 环境变量
# 或者编辑 ~/.config/rust_agent/.env        # 持久化

# 3. 启动
./target/release/agent

# 4. 开始对话
🤖 > 帮我看看当前目录有什么文件
```

一条命令就能用：

```bash
./target/release/agent -p "分析一下这个项目的目录结构"
```

---

## 📦 安装

### 从源码编译

```bash
git clone <repo-url>
cd rust_agent

# 标准编译
cargo build --release

# 静态链接编译（推荐，无系统依赖）
cargo build --release --target x86_64-unknown-linux-musl

# 产物位置
ls target/release/agent
```

### 安装到系统路径（可选）

```bash
cp target/release/agent ~/.local/bin/
# 或
sudo cp target/release/agent /usr/local/bin/
```

---

## ⚙️ 配置

### API Key

Agent 需要一个 LLM API Key 才能工作。

**方式一：环境变量**

```bash
# Anthropic Claude（默认）
export ANTHROPIC_API_KEY=sk-ant-xxxxx

# OpenAI
export OPENAI_API_KEY=sk-xxxxx

# 兼容 API（通义千问、DeepSeek 等）
export LLM_API_KEY=your-key
```

**方式二：`.env` 文件**

支持三个位置（后面的优先级更高）：

| 位置 | 优先级 | 用途 |
|------|--------|------|
| `~/.config/rust_agent/.env` | 低 | 全局配置（XDG 规范） |
| `~/.env` | 中 | 用户级配置 |
| `项目目录/.env` | 高 | 项目级配置 |

```bash
cp .env.example .env
vim .env  # 填入 API Key
```

### 模型管理 (models.toml)

对于使用多个模型的用户，推荐使用 `~/.config/rust_agent/models.toml` 统一管理：

```toml
# 默认使用的模型别名
default = "sonnet"

[models.sonnet]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[models.opus]
provider = "anthropic"
model = "claude-opus-4-20250514"

[models.gpt4o]
provider = "openai"
model = "gpt-4o"

[models.qwen]
provider = "compatible"
model = "qwen-max"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key = "sk-xxxxx"   # 可选，不设则 fallback 到环境变量

[models.local]
provider = "compatible"
model = "llama3-70b"
base_url = "http://localhost:11434/v1"
```

**每个模型 entry 支持的字段：**

| 字段 | 必填 | 说明 |
|------|------|------|
| `provider` | ✅ | `anthropic` / `openai` / `compatible` |
| `model` | ✅ | 模型名称（如 `claude-sonnet-4-20250514`） |
| `base_url` | ❌ | API 地址，不设则用 provider 默认值 |
| `api_key` | ❌ | API 密钥，不设则 fallback 到环境变量 |
| `max_tokens` | ❌ | 最大输出 token 数，默认 8192 |

**配置优先级：**

```
--model CLI参数 > models.toml default > LLM_MODEL 环境变量 > 硬编码默认值
```

也可以通过运行时命令 `/model add <alias>` 交互式添加模型，无需手动编辑文件。

### 环境变量参考

| 变量 | 说明 | 示例 |
|------|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API 密钥 | `sk-ant-xxxxx` |
| `OPENAI_API_KEY` | OpenAI API 密钥 | `sk-xxxxx` |
| `LLM_API_KEY` | 兼容 API 密钥 | `your-key` |
| `LLM_MODEL` | 模型名（低优先级） | `claude-sonnet-4-20250514` |
| `LLM_PROVIDER` | Provider（低优先级） | `anthropic` / `openai` / `compatible` |
| `LLM_BASE_URL` | API 地址（低优先级） | `https://api.openai.com` |

> 当配置了 `models.toml` 后，`LLM_MODEL` / `LLM_PROVIDER` / `LLM_BASE_URL` 环境变量通常不再需要，API Key 环境变量仍作为 fallback。

---

## 🖥️ 启动与运行模式

### CLI 交互模式（默认）

```bash
# 基本启动
./target/release/agent

# 指定工作目录
./target/release/agent --workdir /path/to/your/project

# 带初始提示启动
./target/release/agent -p "帮我看看当前目录有什么文件"

# 跳过所有确认
./target/release/agent --yes

# 开启调试日志
./target/release/agent --verbose
```

### Stdio 模式（脚本集成）

JSON-over-stdio 协议，适合被外部程序（VS Code 扩展、脚本）驱动：

```bash
./target/release/agent --mode stdio --yes -p "列出当前目录结构"
```

输出为逐行 JSON 事件：

```json
{"type":"thinking","data":{}}
{"type":"stream_start","data":{}}
{"type":"streaming_token","data":{"token":"当前"}}
{"type":"tool_use","data":{"tool":"list_directory","input":{"path":"."}}}
{"type":"tool_result","data":{"tool":"list_directory","output":"...","is_error":false}}
{"type":"stream_end","data":{}}
```

**事件类型：**

| 事件 | 说明 |
|------|------|
| `thinking` | LLM 正在处理 |
| `stream_start` / `stream_end` | 流式响应开始 / 结束 |
| `streaming_token` | 流式文本 token |
| `assistant_text` | 非流式完整文本 |
| `tool_use` | 即将执行工具 |
| `tool_result` | 工具执行结果 |
| `diff` | 文件变更 diff |
| `confirm_request` | 请求确认（需回复 `{"approved": true}` 或 `false`） |
| `warning` / `error` | 警告 / 错误 |
| `context_warning` | 上下文窗口压力通知 |

### WebSocket 服务器模式

每个连接独立运行一个 Agent 实例，适合 Web UI 或远程调用：

```bash
# 默认 127.0.0.1:9527
./target/release/agent --mode server

# 指定地址和端口
./target/release/agent --mode server --host 0.0.0.0 --port 8080
```

**客户端协议：**

```json
// 发送消息
{"type": "user_message", "content": "你的问题"}

// 响应确认
{"type": "confirm_response", "approved": true}
```

服务端事件格式同 Stdio 模式。

---

## 💬 日常使用

### 基本对话

启动后进入 REPL，`🤖 >` 是输入提示符：

```
🤖 > 帮我看看这个项目的目录结构

⏳ Thinking...
📂 Tool: list_directory → .
────────────────────────────
这个项目包含以下文件...
────────────────────────────
```

### 多轮迭代

Agent 记住整个对话上下文，可以像聊天一样反复迭代：

```
🤖 > 帮我修改设备树里 GPIO4_A1 的配置
（Agent 修改了 .dts 文件）

🤖 > 不对，应该是 Active Low，你改反了
（Agent 知道你指的是刚才的修改，自动修正）

🤖 > 改好后帮我编译一下
（Agent 执行 make dtbs 并反馈结果）
```

### 文件操作

```
🤖 > 读一下 src/main.rs 的前 50 行
🤖 > 把第 42 行的 println! 改成 eprintln!
🤖 > 创建一个新文件 src/utils.rs，写一个字符串截断函数
🤖 > 同时读取 Cargo.toml 和 src/main.rs
```

### 代码搜索

```
🤖 > 搜索项目中所有用到 unwrap() 的地方
🤖 > 找一下名字里包含 config 的文件
🤖 > 在 src/ 目录下搜索 TODO 注释
```

### 命令执行

```
🤖 > 执行 cargo test
🤖 > 运行 ls -la src/
🤖 > 帮我执行 git status 看看改了什么
```

> 写入/编辑文件和执行命令会要求确认（见「安全与确认机制」）。

### 配合 Git 使用

推荐工作流：

```bash
# 1. 创建工作分支
git checkout -b fix/my-feature

# 2. 启动 Agent
./target/release/agent --workdir /path/to/project

# 3. 让 Agent 修改代码
🤖 > 帮我重构所有 GPIO 初始化代码

# 4. 查看改动
🤖 > 帮我执行 git diff

# 5. 确认后提交
🤖 > 帮我 git add . && git commit -m "refactor: unify GPIO init"

# 6. 退出，在 VS Code 做最终 Review
🤖 > /quit
```

---

## 📋 斜杠命令速查

在 `🤖 >` 提示符下输入：

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助信息 |
| `/clear` | 清空对话历史，开始新话题 |
| `/usage` | 显示本次会话 Token 消耗 |
| `/context` | 查看上下文窗口使用率 |
| `/save` | 手动保存当前会话 |
| `/sessions` | 列出所有已保存的会话 |
| `/yesall` | 关闭所有确认提示（本次会话） |
| `/confirm` | 重新开启确认提示 |
| `/mode` | 查看或设置执行模式：simple/plan/pipeline/auto |
| `/mode <simple|plan|pipeline|auto>` | 设置执行模式 |
| `/model` | 列出当前模型与所有已配置模型 |
| `/model <alias>` | 热切换到指定模型 |
| `/model add <alias>` | 交互式添加新模型配置 |
| `/model remove <alias>` | 删除模型配置 |
| `/model default <alias>` | 设置默认模型 |
| `/memory` | 显示持久记忆 |
| `/summary` | 查看或生成项目摘要 |
| `/summary generate` | 强制（重新）生成项目摘要 |
| `/plan <任务>` | 生成执行计划（只读分析） |
| `/plan show` | 查看待执行的计划 |
| `/plan run` | 执行已生成的计划 |
| `/plan clear` | 清除当前计划 |
| `/changes` | 查看沙盒模式下所有已修改的文件 |
| `/rollback` | 撤销沙盒内的全部改动，恢复原始状态 |
| `/commit` | 将沙盒改动（overlay 上层）合并写入真实项目目录 |
| `/skills` | 查看已加载的项目 Skills |
| `/quit` | 退出（自动保存会话） |

---

## 🔄 模型切换

### 查看当前模型

```
🤖 > /model

🤖  Current model: claude-sonnet-4-20250514 (anthropic)
  Alias: sonnet

  📋  Configured models:
    ▶ sonnet → anthropic/claude-sonnet-4-20250514 ⭐
    • opus → anthropic/claude-opus-4-20250514
    • gpt4o → openai/gpt-4o
    • qwen → compatible/qwen-max

  Switch: /model <alias>  Add: /model add <alias>  Remove: /model remove <alias>
```

- `▶` 标记当前使用的模型
- `⭐` 标记默认模型

### 运行时切换

```
🤖 > /model opus
🔄  Switched to 'opus' → claude-opus-4-20250514 (anthropic)

🤖 > /model gpt4o
🔄  Switched to 'gpt4o' → gpt-4o (openai)
```

切换是即时的，**不会丢失对话上下文**。

### 添加新模型

```
🤖 > /model add deepseek
➕  Adding model 'deepseek'
  Provider (anthropic/openai/compatible): compatible
  Model name: deepseek-chat
  Base URL (leave blank for default): https://api.deepseek.com/v1
  API key (leave blank to use env var): sk-xxxxx

✅  Model 'deepseek' saved to ~/.config/rust_agent/models.toml
```

### 设置默认模型

```
🤖 > /model default opus
⭐  Default model set to 'opus'.
```

### 通过 CLI 参数指定

```bash
# 直接传 model 别名（会从 models.toml 解析）
./target/release/agent --model sonnet

# 或者传完整模型名 + provider
./target/release/agent --provider openai --model gpt-4o
```

---

## 🔀 执行模式切换

Agent 支持多种执行模式，可通过 `/mode` 命令实时切换：

### 查看当前模式

```
🤖 > /mode

🔀  Current execution mode: auto (router decides)
  Use /mode <option> to change:
    simple      — single-model loop, fast & cheap
    plan        — planner + executor, no checker
    pipeline    — full planner → executor → checker
    auto        — let the router decide (default)
```

### 切换执行模式

```
🤖 > /mode simple
🔀  Mode locked to simple: single-model loop for all messages.

🤖 > /mode plan
🔀  Mode locked to plan: planner + executor for all messages.

🤖 > /mode pipeline
🔀  Mode locked to pipeline: full pipeline for all messages.

🤖 > /mode auto
🔀  Mode reset to auto: adaptive router will classify each task.
```

### 执行模式说明

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| **simple** | 单模型循环模式，快速响应，成本低 | 简单问答、代码解释、单文件修改 |
| **plan** | 规划+执行模式，无检查器 | 中等复杂度任务，多文件修改 |
| **pipeline** | 完整流水线模式（规划→执行→检查） | 复杂任务，架构设计，需要验证的任务 |
| **auto** | 自适应路由（默认） | 让 Agent 根据任务复杂度自动选择模式 |

### 自适应路由

当模式设为 `auto` 时，Agent 会根据任务复杂度自动选择执行模式：

- **简单任务** → `simple` 模式（单模型循环）
- **中等任务** → `plan` 模式（规划+执行）
- **复杂任务** → `pipeline` 模式（完整流水线）

路由决策基于：
1. **规则启发式**：关键词匹配（如 "refactor" → 复杂，"explain" → 简单）
2. **LLM 分类**：当启发式不确定时，使用轻量级 LLM 调用分类

---

## 📝 Plan 模式（先分析后执行）

Agent 支持两种「先分析后执行」机制：**`/plan` 斜杠命令**（手动触发）和**自动 Pipeline**（通过 `models.toml` 配置，自动路由）。

### `/plan` 斜杠命令（手动）

对于复杂任务，先让 Agent 只读分析项目，生成方案后再决定是否执行：

```
🤖 > /plan 重构所有 GPIO 初始化代码，统一使用 HAL 库

📝  Generating plan (read-only exploration)...
（Agent 使用只读工具分析代码：read_file, grep_search, list_directory, run_command...）
✅  Plan generated and saved.
  💡 Use /plan run to execute or /plan show to view again.
```

**查看计划：**

```
🤖 > /plan show
📋  Pending Plan:
  1. 找到所有直接操作寄存器的 GPIO 初始化代码（共 5 处）
  2. 引入 HAL 库头文件
  3. 逐个替换为 HAL API 调用
  4. 编译验证
```

**执行计划：**

```
🤖 > /plan run
🚀  Executing plan...
```

**丢弃计划：**

```
🤖 > /plan clear
🗑️  Pending plan cleared.
```

Plan 阶段允许使用只读工具（`read_file`、`grep_search`、`list_directory` 等）以及**只读 shell 命令**（`git status`、`git log`、`git diff`、`find` 等），确保**零副作用**。

---

### 自动 Pipeline（Planner → Executor → Checker）

通过 `models.toml` 配置多角色流水线后，Agent 会根据任务复杂度自动路由（或始终走 Pipeline）。用户**无需学习任何新命令**，整个流程完全透明交互。

#### 流程概览

```
用户输入
  └─▶ Planner（只读探索，生成计划）
            │
            ▼
       计划审核（用户控制）
            │
            ▼
       Executor（全工具，按计划执行）
            │
            ▼
       Checker（验证结果，可重试）
```

#### 计划审核

Planner 生成计划后，会暂停等待你的确认：

```
📋 Pipeline Plan:
────────────────────────────────────────────────────────────
1. 检出新分支并查看文件结构
2. 识别冲突的模块路径
3. 按新分支的结构调整 include/import
...
────────────────────────────────────────────────────────────
   Review: [y] approve  [n] reject  [type feedback to refine]
   > 
```

| 输入 | 效果 |
|------|------|
| `y` / `yes` | 批准并进入执行（见下方「执行前注入背景」） |
| `n` / `no` | 取消整个 Pipeline |
| 直接输入文字 | 作为反馈重新生成计划（最多 5 轮） |

#### 执行前注入背景（approve 时）

输入 `y` 后，系统会追加询问你是否有背景信息需要告知执行器。这是最关键的干预时机——当你知道 LLM 可能不清楚的项目细节时，在此补充：

```
   Review: [y] approve  [n] reject  [type feedback to refine]
   > y
   Context: add background info for the executor (Enter to skip)
   > 注意：新分支已将 module.rs 重构为 foo/mod.rs + foo/types.rs + foo/handler.rs，旧路径已删除
```

这段背景会以最高优先级注入到 Executor 的初始 prompt，LLM 在第一步就能感知到这个事实，避免找错文件或做出错误假设。

#### 执行中随时打断（Ctrl+\）

Executor 运行期间，你可以在**任意 LLM 迭代之间**按 `Ctrl+\` 暂停并注入新指导：

```
⚡ Executor running — press Ctrl+\ to pause and inject guidance at any time
（LLM 正在执行第 3 步...）

按下 Ctrl+\ 后：

⚡ Guidance: type a note for the executor (or press Enter to continue)
   > 等一下，那个文件已经被删了，你应该去看 src/driver/new_gpio.c
💡 Guidance injected into executor context.
```

指导会追加到 Executor 的 system prompt，LLM 在下一次调用中完整接收，**不打乱 API 消息结构**。

> **Ctrl+C vs Ctrl+\**
> - `Ctrl+C` — 立即中断当前 Pipeline，停止执行
> - `Ctrl+\` — 暂停等待你的指导，输入后继续执行

---

## 📜 会话管理

Agent 在每次交互后自动保存会话，支持跨天任务。

### 列出历史会话

```bash
./target/release/agent --sessions

# 📜 Saved Sessions:
#   ID         Updated                  Msgs   Summary
#   ──────────────────────────────────────────────
#   a1b2c3d4   2026-02-15T10:30:00      12     帮我修改 GPIO 配置...
#   e5f6g7h8   2026-02-14T16:20:00       8     编译内核模块...
```

### 恢复会话

```bash
./target/release/agent --resume a1b2c3d4

# 🔄  Resumed session a1b2c3d4 (12 messages)
```

### 手动保存

```
🤖 > /save
💾  Session saved: a1b2c3d4
```

---

## 🧠 记忆与项目摘要

### 持久记忆

Agent 自动将工具操作记录到 `.agent/memory.md`，跨会话保持：

```
🤖 > /memory
🧠  Agent Memory (15 entries):
  📖 Project Knowledge:
    • Target board: RK3588 custom board
    • Toolchain: aarch64-linux-gnu-
  📁 Key Files:
    • src/main.c (edited)
    • rk3588-myboard.dts (written)
  📝 Session Log:
    • edited src/main.c
    • ran `make -j8`
```

### 项目摘要

首次使用时，运行 `/summary` 生成项目概述，后续会话自动加载：

```
🤖 > /summary
📋  No project summary found.
  Generate one now? [y/N] y
📝  Generating project summary...
✅  Project summary saved to .agent/summary.md
```

摘要包含项目名称、技术栈、目录结构、构建命令等，Agent 无需每次重新分析项目。

---

## 📚 Skills 系统

Skills 是项目级的知识文件，让 Agent "理解" 你的项目规范。

### 目录结构

```
your-project/
├── AGENT.md                    # 全局指令（自动加载）
└── .agent/
    └── skills/
        ├── coding-style.md     # 编码规范
        ├── architecture.md     # 架构说明
        └── deploy.md           # 部署流程
```

### AGENT.md 示例

```markdown
# Project: My Embedded BSP

- Target: RK3588 based custom board
- Toolchain: aarch64-linux-gnu-
- Kernel source: kernel/
- After any code change, run `make -j$(nproc)` to verify compilation
```

### Skill 文件示例

`.agent/skills/coding-style.md`:

```markdown
# Skill: Coding Style

- Use Rust 2021 edition
- All public functions must have doc comments
- Error handling: use `anyhow::Result`, avoid `unwrap()` in library code
- Format: run `cargo fmt` before committing
```

### 查看已加载的 Skills

```
🤖 > /skills
📋  3 skill(s) loaded:
  • Project Instructions (AGENT.md) [embedded]
  • Coding Style (.agent/skills/coding-style.md) [on-demand]
  • Architecture (.agent/skills/architecture.md) [on-demand]
```

`[embedded]` 表示已注入 system prompt，`[on-demand]` 表示可通过 `load_skill` 工具按需加载。

---

## ✏️ 自定义系统提示词

通过 Markdown 文件自定义 LLM 行为：

| 文件 | 作用域 | 优先级 |
|------|--------|--------|
| `~/.config/rust_agent/system_prompt.md` | 全局 | 低 |
| `<项目>/.agent/system_prompt.md` | 当前项目 | 高 |

**追加模式（默认）— 直接写内容：**

```markdown
你是一个嵌入式 C 开发专家，擅长 ARM Cortex-M。
请用中文回复。
代码注释使用英文。
```

**替换模式 — 第一行写 `# OVERRIDE`：**

```markdown
# OVERRIDE
你是一个 Rust 系统编程专家。
请按照项目 .editorconfig 的代码风格编写。
```

**加载顺序：**

```
默认提示词 → 全局 system_prompt.md → 项目 system_prompt.md → summary → skills → memory
```

---

## 🔒 安全与确认机制

### 需要确认的操作

| 操作 | 确认时显示 |
|------|-----------|
| 写入文件 (`write_file`) | 文件路径和行数 |
| 编辑文件 (`edit_file` / `multi_edit_file`) | 文件路径，执行后展示 Diff |
| 执行命令 (`run_command`) | 完整命令内容 |

### 确认交互

```
⚠️  Write file: src/driver.c (45 lines)
   Proceed? [y]es / [n]o / [a]lways:
```

| 输入 | 效果 |
|------|------|
| `y` | 确认本次操作 |
| `n` | 拒绝（Agent 会调整策略） |
| `a` | 本次会话内跳过所有确认 |

### 跳过确认

```bash
# 启动时
./target/release/agent --yes

# 运行时
🤖 > /yesall     # 关闭确认
🤖 > /confirm    # 重新开启
```

Auto-approve 时会显示 `⚡ auto-approved:` 提示，让你知道跳过了什么。

### 无需确认的操作

`read_file`、`batch_read_files`、`grep_search`、`file_search`、`list_directory`、`fetch_url`、`read_pdf`、`read_ebook`、`think` — 所有只读工具不需要确认。

---

## 🛡️ 沙盒模式（文件保护与回滚）

沙盒模式是本工具的核心安全特性。开启后，Agent 对原始项目**零写入**——所有文件修改都落入隔离层，你可以随时查看、提交或回滚，源码始终受到保护。

### 启用方式

#### 方式一：服务器启动时全局开启（推荐 Web UI 用户）

```bash
# 服务器模式下，所有后续 Web UI 连接默认启用沙盒
./target/release/agent --mode server --sandbox

# 同时指定工作目录
./target/release/agent --mode server --sandbox --workdir /path/to/project
```

#### 方式二：CLI 模式逐次开启

```bash
./target/release/agent --sandbox
./target/release/agent --sandbox --workdir /path/to/project
```

#### 方式三：Web UI 逐连接开启

在 ConnectModal（连接对话框）中勾选「启用沙盒模式」，即可仅对当前连接启用沙盒，服务器无需加 `--sandbox` 参数。

也可在 WebSocket URL 中显式指定：

```
ws://localhost:9527?sandbox=1&workdir=/path/to/project
```

### 两种隔离后端

| 后端 | 触发条件 | 保护范围 | 适用场景 |
|------|---------|---------|---------|
| **Overlay**（内核叠加层） | Linux + 已安装 `fuse-overlayfs` | 全部文件写入 + 命令副作用（如 `cargo build` 产物） | 生产环境推荐，保护最完整 |
| **Snapshot**（快照） | 跨平台回退方案（overlayfs 不可用时自动回退） | 仅 Agent 工具直接写入的文件 | 非 Linux 环境，或快速体验 |

启动时终端会显示当前后端：

```
🔒  Sandbox enabled (overlay) — original project untouched, all changes in overlay layer
   Use /changes to view changes, /rollback to undo, /commit to accept.
```

### Overlay 后端架构

Overlay 后端使用 Linux 内核的 overlayfs 叠加文件系统，结合 **用户命名空间（user namespace）+ 挂载命名空间（mount namespace）** 实现进程级隔离：

```
┌─────────────────── Agent 进程看到的视图 ───────────────────────┐
│                     /workspace (merged)                       │
│   读：优先读 upper，upper 没有则穿透到 lower                   │
│   写：所有写操作由内核重定向到 upper（tmpfs），lower 不变       │
└───────────────────────────────────────────────────────────────┘
              ↑                          ↑
    upper layer (tmpfs)          lower layer (bind mount)
    所有写入落这里                 原始项目目录，只读
    Agent 崩溃自动清理            始终不动
```

每个 WebSocket 连接独立一个命名空间：
- 不同客户端同时连接互不干扰
- 连接断开或回滚后，tmpfs 上层自动清理，不留垃圾文件
- `run_command` 执行的 Shell 命令（如 `cargo build`、`make`）也在此隔离环境中运行，构建产物不污染源码树

### 扩展工具链（`extra_binds`）

容器环境默认只挂载系统标准路径（`/usr`、`/lib`、`/bin` 等）。若项目需要 `rustup`、`cargo`、`node`、浏览器等非标准工具，需在 `~/.config/rust_agent/models.toml` 中配置额外绑定挂载：

```toml
# Rust 工具链（rustup 管理的版本，需写权限以支持 sccache / target 缓存）
[[extra_binds]]
src = "/home/user/.rustup"
dst = "/root/.rustup"
readonly = false

[[extra_binds]]
src = "/home/user/.cargo"
dst = "/root/.cargo"
readonly = false

# Node.js（只读即可）
[[extra_binds]]
src = "/usr/local/lib/nodejs"
dst = "/usr/local/lib/nodejs"
readonly = true

# 只读数据集或模型权重
[[extra_binds]]
src = "/mnt/models"
dst = "/models"
readonly = true
```

> **提示**：可以通过 `which rustc` / `which cargo` 在宿主机确认工具链路径，再填写到 `src`。

配置后重启 server，沙盒内的 `run_command` 即可调用这些工具链。

### 沙盒命令（CLI）

#### `/changes` — 查看改动

```
🤖 > /changes

📊  Sandbox changes (3 files):
  ✏️  modified   src/driver/gpio.c       (312 → 328 bytes)
  ✏️  modified   include/gpio.h          (80 → 95 bytes)
  ✨  created    src/driver/gpio_hal.c   (new, 210 bytes)
```

#### `/rollback` — 撤销全部改动

```
🤖 > /rollback

🔄  Rolling back all sandbox changes...
✅  Rollback complete: 2 files restored, 1 file deleted
```

> ⚠️ rollback 不可逆，执行前请确认。rollback 后可以继续与 Agent 对话，从干净状态重新开始。

#### `/commit` — 提交改动

```
🤖 > /commit

✅  Committed: 2 modified, 1 created
```

将 tmpfs 上层的所有变更合并写入原始项目目录，然后卸载 overlayfs。

### Web UI 沙盒面板

使用 Web UI 时，顶部 Header 会显示沙盒状态徽标：

| 徽标 | 含义 |
|------|------|
| 🔒 **沙盒**（绿色）| 沙盒已激活，尚无待提交的改动 |
| 🔒 **沙盒 · N 待提交**（黄色）| 有 N 个文件已修改/新建，等待处理 |

点击「沙盒」徽标（或通过侧边栏「沙盒」Tab）可打开**沙盒面板**，其中提供：

- **改动列表**：显示所有 modified / created / deleted 文件及大小变化
- **全量提交**：一键将所有改动合并到原始项目
- **全量回滚**：一键撤销所有改动
- **逐文件提交** ✓：点击单个文件右侧的 ✓ 按钮，只将该文件合并到原始目录，其他改动继续保留在沙盒中——适合部分接受 Agent 修改的场景

### 典型工作流

**CLI 模式：**

```bash
# 1. 以沙盒模式启动
./target/release/agent --sandbox --workdir /path/to/project

# 2. 指派任务（所有修改都在隔离层）
🤖 > 帮我重构 GPIO 驱动，统一用 HAL 接口

# 3. 查看 Agent 做了什么
🤖 > /changes

# 4a. 满意 → 提交到真实项目
🤖 > /commit

# 4b. 不满意 → 全部回滚
🤖 > /rollback

# 4c. 部分满意 → 先回滚，再让 Agent 重新针对性修改，再提交
🤖 > /rollback
🤖 > 只修改 gpio.c，不要动 gpio.h
🤖 > /commit
```

**Web UI 模式：**

1. 启动服务器：`./target/release/agent --mode server --sandbox`
2. 在 ConnectModal 填写服务器地址，确认「启用沙盒模式」已勾选，点击连接
3. 在聊天框输入任务，Agent 执行期间 Header 会显示「🔒 沙盒 · N 待提交」
4. 打开沙盒面板查看改动详情
5. 逐文件或全量提交/回滚

### 故障排查

**Q：连接后收到「⚠️ 沙盒模式请求失败」警告**

沙盒只有 Overlay 一种后端，依赖 `fuse-overlayfs`。若该工具不存在，沙盒直接禁用，**不会降级保护**，所有文件操作将直接作用于真实项目。安装方式：

```bash
# Ubuntu / Debian
sudo apt install fuse-overlayfs

# Arch Linux
sudo pacman -S fuse-overlayfs

# 验证
fuse-overlayfs --version
```

安装后重启 Agent 并重新连接即可。

**Q：容器内 `cargo`/`rustc` 命令找不到**

需在 `models.toml` 中添加 `extra_binds` 将 `~/.rustup` 和 `~/.cargo` 挂载进容器。详见上方「扩展工具链」小节。

**Q：`/commit` 后原始项目文件有变化，但 Git 没有显示 diff**

文件内容与提交前完全相同时，Git 不会报变更。可以用 `/changes` 确认沙盒内的改动是否非空，再决定是否提交。

---

## 🤖 多 Agent 协作（多进程专家池）

多 Agent 协作允许主 Agent 将复杂任务分解，并指派给专注于特定子目录或领域的“专家 Agent”实例。

### 为什么需要多 Agent？

- **Monorepo 支持**：在大型项目中，通过 `target_dir` 锁定子 Agent，避免其误触全局代码。
- **专注度提升**：子 Agent 只关注局部上下文，Token 消耗更低，响应更精准。
- **并发处理**：主 Agent 可以根据需要同时维持多个独立运行的子任务环境。

### 两种子 Agent 调用方式

Agent 提供两种子 Agent 调用机制，适用于不同场景：

#### 1. `call_sub_agent` - 连接预启动的 WebSocket 服务器

**适用场景**：长期运行的专家服务，如：
- 预配置的代码审查专家
- 持续运行的测试专家  
- 需要保持状态的对话专家

**配置方式**：在 `~/.config/rust_agent/models.toml` 中配置 `sub_agents`：

```toml
# 定义子 Agent 专家池
[sub_agents.coder]
port = 9001
role = "executor"

[sub_agents.reviewer]
port = 9002
role = "checker"
```

主 Agent 启动时会自动在后台拉起这些服务。

**调用示例**：
```json
{
  "prompt": "重构 frontend/src/components 目录下的所有 React 组件",
  "server_url": "ws://localhost:9001",
  "target_dir": "frontend/src/components",
  "auto_approve": false
}
```

#### 2. `spawn_sub_agent` - 动态创建 stdio 子进程

**适用场景**：临时性、一次性任务，如：
- 快速代码片段生成
- 单文件修改
- 不需要长期运行的小任务

**特点**：
- 无需预启动服务器
- 按需创建，用完即销毁
- 默认自动批准所有工具调用（`auto_approve: true`）

**调用示例**：
```json
{
  "prompt": "为 utils/helpers.rs 添加错误处理",
  "target_dir": "utils",
  "auto_approve": true,
  "timeout_secs": 300
}
```

### 工具参数说明

#### `call_sub_agent` 参数：
- `prompt`: 给专家的具体指令
- `server_url`: 专家 Agent 的 WebSocket 地址（默认：ws://localhost:9527）
- `target_dir`: （推荐）隔离执行的相对路径
- `auto_approve`: 是否允许子 Agent 自动执行修改（默认为 false，由主 Agent 转发确认）

#### `spawn_sub_agent` 参数：
- `prompt`: 给子 Agent 的任务描述
- `target_dir`: 可选的工作目录，子 Agent 的文件工具将限定在此目录
- `auto_approve`: 是否自动批准所有工具调用（默认：true）
- `timeout_secs`: 子 Agent 最大运行时间（默认：300秒）

### 透明度与安全

- **实时日志**：主 Agent 的 Terminal 会以前缀 `[Sub-Agent Thinking]` 和 `[Sub-Agent Tool Use]` 实时回放专家的思考和操作流程。
- **授权代理**：当子 Agent 需要写文件或跑命令时，主 Agent 会截获请求并弹窗询问你，确保安全受控。
- **禁止递归**：子 Agent 无法再调用其他 Agent，确保任务拓扑简单清晰。

---

## 🧰 内置工具一览

| 工具 | 图标 | 用途 | 需确认 |
|------|------|------|--------|
| `read_file` | 📖 | 读取文件内容（支持行范围） | ❌ |
| `batch_read_files` | 📚 | 一次读取多个文件 | ❌ |
| `write_file` | ✏️ | 创建或覆盖写入文件 | ✅ |
| `edit_file` | 🔧 | 精确 find & replace 编辑 | ✅ |
| `multi_edit_file` | 🔧 | 单文件多处批量编辑 | ✅ |
| `run_command` | ⚡ | 执行 Shell 命令（含超时） | ✅ |
| `grep_search` | 🔍 | 按正则搜索文件内容 | ❌ |
| `file_search` | 📁 | 按 glob 搜索文件名 | ❌ |
| `list_directory` | 📂 | 列出目录内容 | ❌ |
| `think` | 💭 | 内部推理（无副作用） | ❌ |
| `read_pdf` | 📄 | PDF 文本提取 | ❌ |
| `read_ebook` | 📕 | 电子书读取 (MOBI/EPUB/AZW3) | ❌ |
| `fetch_url` | 🌐 | 网页抓取与正文提取 | ❌ |
| `call_sub_agent` | 🤝 | 委派任务给预启动的 WebSocket 专家 Agent | ✅ |
| `spawn_sub_agent` | 👶 | 动态创建 stdio 子进程处理临时任务 | ✅ |
| `load_skill` | 📚 | 加载项目技能（.agent/skills/） | ❌ |
| `create_skill` | ✍️ | 创建或更新项目技能 | ✅ |

### 外部依赖（可选）

部分工具依赖系统命令，Agent 会自动按优先级尝试：

| 工具 | 后端 | 安装方式 |
|------|------|----------|
| `read_pdf` | marker_single → pdftotext → mutool | `pip install marker-pdf` / `apt install poppler-utils` / `apt install mupdf-tools` |
| `read_ebook` | ebook-convert → pandoc | `apt install calibre` / `apt install pandoc` |
| `fetch_url` | readable → pandoc → 内置 regex | `npm install -g @nickersoft/readability-cli` / `apt install pandoc` |

---

## 🏷️ CLI 参数速查

```
agent [OPTIONS]

Options:
  -p, --prompt <PROMPT>        初始提示词
  -m, --model <MODEL>          模型名或 models.toml 别名 [默认: claude-sonnet-4-20250514]
      --provider <PROVIDER>    API provider: anthropic / openai / compatible [默认: anthropic]
  -d, --workdir <DIR>          工作目录 [默认: 当前目录]
  -v, --verbose                开启调试日志
  -y, --yes                    跳过所有确认提示
  -r, --resume <ID>            恢复历史会话
      --sessions               列出所有已保存的会话
      --mode <MODE>            运行模式: cli / stdio / server [默认: cli]
      --host <HOST>            WebSocket 绑定地址 [默认: 127.0.0.1]
      --port <PORT>            WebSocket 端口 [默认: 9527]
      --max-iterations <N>     工具最大迭代次数 [默认: 25]
      --sandbox                开启沙盒模式（文件保护+回滚）
  -h, --help                   显示帮助
```

---

## 📂 目录结构约定

### 用户配置目录 (`~/.config/rust_agent/`)

```
~/.config/rust_agent/
├── .env                    # 全局环境变量（API Key）
├── models.toml             # 模型管理配置
└── system_prompt.md        # 全局系统提示词（可选）
```

### 项目级目录 (`.agent/`)

```
your-project/
├── AGENT.md                # 全局项目指令（自动加载）
└── .agent/
    ├── memory.md           # 持久记忆（自动维护）
    ├── summary.md          # 项目摘要（/summary 生成）
    ├── system_prompt.md    # 项目级系统提示词（可选）
    └── skills/             # 项目级 Skills
        └── *.md
```

建议在 `.gitignore` 中添加：

```
.agent/memory.md
.agent/sessions/
```

---

## ❓ 常见问题

### Q: API Key 报错 "not set"

检查以下位置是否正确配置了对应的环境变量：

| Provider | 环境变量 |
|----------|----------|
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Compatible | `LLM_API_KEY` |

也可以在 `models.toml` 的对应 entry 中直接设置 `api_key`。

### Q: 如何使用国内 API（通义千问、DeepSeek 等）？

```toml
# ~/.config/rust_agent/models.toml
[models.qwen]
provider = "compatible"
model = "qwen-max"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key = "sk-xxxxx"

[models.deepseek]
provider = "compatible"
model = "deepseek-chat"
base_url = "https://api.deepseek.com/v1"
api_key = "sk-xxxxx"
```

然后 `/model qwen` 或 `/model deepseek` 即可切换。

### Q: 上下文太长，API 报错

```
🤖 > /context
```

查看上下文使用率。接近 80% 时 Agent 会自动截断旧消息。可以用 `/clear` 清空对话重新开始。

### Q: 想让 Agent 用中文回复

在 `~/.config/rust_agent/system_prompt.md` 中写入：

```markdown
请始终使用中文回复。
```

### Q: 如何让 Agent 了解项目规范

1. 在项目根目录创建 `AGENT.md`，写入项目背景和规范
2. 在 `.agent/skills/` 下放置具体技能文件
3. 运行 `/summary` 生成项目摘要

### Q: 如何恢复昨天的对话

```bash
./target/release/agent --sessions    # 查看会话列表
./target/release/agent --resume <ID> # 恢复指定会话
```

### Q: 切换模型会丢失对话吗？

不会。`/model <alias>` 是热切换，对话上下文完整保留。
