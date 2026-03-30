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
    - [MCP 服务器模式](#mcp-服务器模式)
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
  - [🔀 执行模式切换](#-执行模式切换)
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
  - [🔌 插件系统](#-插件系统)
    - [插件目录结构](#插件目录结构)
    - [plugin.toml 清单](#plugintoml-清单)
    - [插件工具](#插件工具)
    - [插件技能skills](#插件技能skills)
    - [Hooks钩子事件](#hooks钩子事件)
    - [插件-system_promptmd](#插件-system_promptmd)
    - [安装与示例插件](#安装与示例插件)
  - [✏️ 自定义系统提示词](#️-自定义系统提示词)
  - [🔒 安全与确认机制](#-安全与确认机制)
    - [需要确认的操作](#需要确认的操作)
    - [确认交互](#确认交互)
    - [跳过确认](#跳过确认)
    - [无需确认的操作](#无需确认的操作)
  - [🛡️ 隔离模式（Normal / Container / Sandbox）](#️-隔离模式normal--container--sandbox)
    - [启用方式](#启用方式)
      - [方式一：服务器启动时全局指定（推荐 Web UI 用户）](#方式一服务器启动时全局指定推荐-web-ui-用户)
      - [方式二：CLI 模式](#方式二cli-模式)
      - [方式三：Web UI 逐连接指定](#方式三web-ui-逐连接指定)
    - [隔离实现（Container / Sandbox）](#隔离实现container--sandbox)
    - [扩展工具链（`extra_binds`）](#扩展工具链extra_binds)
    - [沙盒命令（CLI）](#沙盒命令cli)
      - [`/changes` — 查看改动](#changes--查看改动)
      - [`/rollback` — 撤销全部改动](#rollback--撤销全部改动)
      - [`/commit` — 提交改动](#commit--提交改动)
    - [Web UI 沙盒面板](#web-ui-沙盒面板)
    - [典型工作流](#典型工作流)
    - [故障排查](#故障排查)
  - [🤖 多 Agent 协作（节点委派）](#-多-agent-协作节点委派)
    - [为什么需要多 Agent？](#为什么需要多-agent)
    - [节点配置（workspaces.toml）](#节点配置workspacestoml)
    - [`call_node` 统一接口](#call_node-统一接口)
      - [target 寻址方式](#target-寻址方式)
    - [工具参数说明](#工具参数说明)
    - [透明度与安全](#透明度与安全)
  - [🧰 内置工具一览](#-内置工具一览)
    - [外部依赖（可选）](#外部依赖可选)
  - [🔌 MCP集成](#-mcp集成)
    - [作为 MCP 工具服务器](#作为-mcp-工具服务器)
    - [接入外部 MCP 服务器（MCP 客户端）](#接入外部-mcp-服务器mcp-客户端)
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

### MCP 服务器模式

通过 `--mode mcp` 将 Agent 变为标准 MCP（Model Context Protocol）工具服务器，外部宿主（Claude Desktop、Cursor 等）可直接列举并调用全部内置工具，本地不运行任何 LLM：

```bash
./target/release/agent --mode mcp --workdir /path/to/project
```

**Claude Desktop 配置示例**（`claude_desktop_config.json`）：

```json
{
  "mcpServers": {
    "rust-agent": {
      "command": "/path/to/agent",
      "args": ["--mode", "mcp", "--workdir", "/your/project"]
    }
  }
}
```

支持方法：`initialize` · `tools/list` · `tools/call`（JSON-RPC 2.0 over stdio）。所有工具权限均**自动 approve**，适合作为受信任的本地工具提供者。

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

## 🔌 插件系统

插件是对 Agent 功能最自然的扩展单元，安装即用，无需修改任何 Rust 核心代码。将插件目录放入 `.agent/plugins/` 并重启 Agent 即生效。

### 插件目录结构

```
.agent/
└── plugins/
    └── my-plugin/               # 插件根目录（名称即 ID）
        ├── plugin.toml          # 插件清单（必须）
        ├── system_prompt.md     # 追加到系统提示词（可选）
        ├── tools/               # 动态工具定义（可选）
        │   ├── my_tool.json     # 工具 schema
        │   └── my_tool.sh       # 执行脚本
        ├── skills/              # 领域知识注入（可选）
        │   └── guide.md
        └── hooks/               # 生命周期钩子（可选）
            ├── on_start.toml
            └── on_tool_after.toml
```

### plugin.toml 清单

```toml
[plugin]
name        = "my-plugin"
version     = "1.0.0"
description = "这个插件做什么"
author      = "yourname"
enabled     = true      # false 可临时禁用，不删除目录
```

`name` 和 `enabled` 是必填项，其余均可省略。

### 插件工具

与 Skills 动态工具格式完全一致——在 `tools/` 下放 `.json` + 脚本即可：

```json
// tools/git_log.json
{
  "name": "git_log",
  "description": "查询 Git 提交历史，返回结构化 JSON",
  "parameters": {
    "type": "object",
    "properties": {
      "limit":  { "type": "integer", "description": "返回最多条数", "default": 20 },
      "author": { "type": "string",  "description": "按作者过滤（可省）" }
    }
  },
  "command": "./tools/git_log.sh",
  "timeout_secs": 15
}
```

LLM 调用时参数以 JSON 形式写入脚本 stdin，stdout 作为返回値，非零退出码为错误。

### 插件技能（Skills）

`skills/` 目录下的 Markdown 文件在 Agent 启动时自动加入 skills 索引，LLM 可通过 `load_skill` 工具按需加载完整内容：

```
🤖 > /skills
📋  5 skill(s) loaded:
  • Project Instructions (AGENT.md) [embedded]
  • Coding Style (.agent/skills/coding-style.md) [on-demand]
  • Git Workflow (plugin:project-stats / guide.md) [on-demand]    ← 插件技能
```

### Hooks（钩子事件）

钩子让插件在 Agent 生命周期的关键节点执行自定义逻辑。

#### Hook 配置格式

每个 `.toml` 文件定义一个 Hook：

```toml
# hooks/on_start.toml
event        = "agent.start"
mode         = "fire_and_forget"
command      = "./hooks/on_start.sh"
timeout_secs = 5
```

| 字段 | 说明 |
|------|------|
| `event` | 监听的事件名称 |
| `mode` | 执行模式（见下表） |
| `command` | 执行的脚本，路径相对于插件根目录 |
| `timeout_secs` | 超时（默认 30s） |

#### 执行模式

| 模式 | 行为 | 典型用途 |
|------|------|----------|
| `fire_and_forget` | 异步触发，不阻塞 Agent | 写日志、发通知、统计 |
| `blocking` | 同步执行，Agent 等到脚本退出才继续 | 启动前探活检查 |
| `intercepting` | 脚本 stdout 可修改 Agent 决策 | 路由覆盖、工具拦截 |

Agent 把事件 payload 以 JSON 写入脚本 **stdin**：

```bash
#!/bin/bash
# hooks/on_start.sh
payload=$(cat)
project_dir=$(echo "$payload" | jq -r '.project_dir')
echo "[$(date)] agent started in $project_dir" >> /tmp/audit.log
```

#### 已支持的 Hook 事件

| 事件 | 触发时机 | Payload 字段 | 支持 intercepting |
|------|----------|--------------|-------------------|
| `agent.start` | Agent 初始化完成后 | `project_dir`, `mode` | ❌ |
| `tool.before` | 工具调用前 | `tool_name`, `input` | ✅ |
| `tool.after` | 工具调用后 | `tool_name`, `result`, `duration_ms` | ❌ |
| `context.warning` | 上下文使用率超 80% | `usage_pct`, `tokens_used` | ❌ |
| `context.critical` | 上下文使用率超 95% | `usage_pct`, `tokens_used` | ❌ |
| `plan.complete` | 流水线通过 Checker | `mode`, `attempts`, `success` | ❌ |
| `router.decision` | 自适应路由决策时 | `proposed_mode`, `task_preview` | ✅ |

#### router.decision 拦截示例——高风险词强制走完整流水线

```bash
#!/bin/bash
# hooks/on_router.sh
payload=$(cat)
task=$(echo "$payload" | jq -r '.task_preview')
if echo "$task" | grep -qiE 'drop|delete|force.?push|truncate'; then
    echo '{"override_mode": "full_pipeline"}'
else
    echo '{}'   # 空对象 = 不干预
fi
```

```toml
# hooks/on_router.toml
event        = "router.decision"
mode         = "intercepting"
command      = "./hooks/on_router.sh"
timeout_secs = 3
```

### 插件 system_prompt.md

插件的 `system_prompt.md` 以**追加方式**写入系统提示词，位于项目级 `system_prompt.md` 之后、skills 索引之前，适合注入工具使用规范或领域约束：

```markdown
# Git 工具规范（由 project-stats 插件追加）
- 查询提交历史时使用 git_log 工具，不要执行原始 git 命令
- 写入代码前先用 word_count 评估规模
```

结合 `.agent/system_prompt.md` 的 `# OVERRIDE` 覆盖机制，可以构建完全定制化的专属 Agent：

| 文件 | 作用 |
|------|------|
| `.agent/system_prompt.md`（`# OVERRIDE`） | 完全替换默认角色定位 |
| `plugins/<name>/system_prompt.md` | 追加领域规则 |

整个工具 / 流水线 / 安全确认基础设施完整保留，只替换角色定位和领域规则。

### 安装与示例插件

```bash
# 方式一：直接复制
cp -r /path/to/plugin .agent/plugins/

# 方式二：git clone
git clone https://github.com/example/agent-plugin-xxx .agent/plugins/xxx

# 重启 Agent 即生效，无需其他配置
```

项目在 `sample/` 目录下提供了两个完整示例：

#### `sample/project-stats/`——工具 + Hooks 综合示例

| 组件 | 内容 |
|------|------|
| 工具 | `git_log`（Git 历史查询）、`word_count`（代码规模统计） |
| 技能 | `git-workflow.md`（工具使用最佳实践） |
| Hooks | `agent.start` fire_and_forget（写会话日志）、`tool.after` fire_and_forget（写审计日志）、`router.decision` intercepting（高风险词 → 强制 full_pipeline） |
| system_prompt.md | 限制 LLM 使用插件工具而非原始 git 命令 |

安装：`cp -r sample/project-stats .agent/plugins/project-stats`

#### `sample/dev-cluster/`——多节点 + MCP 配置示例

| 组件 | 内容 |
|------|------|
| 工具 | `probe_nodes`（探活本地节点 + 远程 peer /health） |
| 技能 | `multi-node-guide.md`（路由规则 + MCP 工具说明） |
| `workspaces.toml` | 3 个本地 node（frontend/backend/infra）+ 2 个 peer（gpu-box/ci-runner） |
| MCP 配置 | stdio 传输（filesystem）、HTTP/SSE 传输（brave-search）、多服务器格式（github + postgres） |
| Hooks | `agent.start` blocking（启动时探活所有节点） |
| system_prompt.md | 生产操作确认规则 + 节点路由约束 |

安装：`cp -r sample/dev-cluster .agent/plugins/dev-cluster`

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
默认提示词 → 全局 system_prompt.md → 项目 system_prompt.md
          → 插件 system_prompt.md（按插件顺序追加）
          → summary → skills 索引（内置 + 插件）→ memory
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

`read_file`、`batch_read_files`、`grep_search`、`file_search`、`list_directory`、`read_pdf`、`think`、`list_nodes`、`load_skill`、`connect_service`、`query_service`、`subscribe_service`、`unsubscribe_service`、`list_services` — 所有只读工具不需要确认。

---

## 🛡️ 隔离模式（Normal / Container / Sandbox）

Agent 支持三种隔离模式，通过 `--isolation` 指定（默认为 `container`）：

| 模式 | 参数值 | rootfs/namespace | overlayfs | /rollback | 说明 |
|------|--------|-----------------|-----------|-----------|------|
| **Normal** | `normal` | ✗ | ✗ | ✗ | 直接在宿主运行，工具全兼容 |
| **Container** | `container` | ✅ | ✗ | ✗ | namespace 隔离，写操作直接落到项目（**默认**）|
| **Sandbox** | `sandbox` | ✅ | ✅ | ✅ | overlayfs 保护 + /rollback + /commit |

### 启用方式

#### 方式一：服务器启动时全局指定（推荐 Web UI 用户）

```bash
# sandbox 模式（所有连接均受 overlayfs 保护）
./target/release/agent --mode server --isolation sandbox

# container 模式（默认）：隔离运行但写操作直落真实项目
./target/release/agent --mode server
./target/release/agent --mode server --isolation container

# normal 模式：无容器，适合本地开发
./target/release/agent --mode server --isolation normal
```

#### 方式二：CLI 模式

```bash
./target/release/agent --isolation sandbox
./target/release/agent --isolation sandbox --workdir /path/to/project
./target/release/agent --isolation normal
```

#### 方式三：Web UI 逐连接指定

在 ConnectModal（连接对话框）的「隔离模式」下拉菜单中选择，即可仅对当前连接生效。

也可在 WebSocket URL 中显式指定：

```
ws://localhost:9527?mode=sandbox&workdir=/path/to/project
ws://localhost:9527?mode=normal&workdir=/path/to/project

# 向后兼容（sandbox=1 映射到 sandbox 模式）
ws://localhost:9527?sandbox=1&workdir=/path/to/project
```

### 隔离实现（Container / Sandbox）

Container 与 Sandbox 模式均基于 **Linux 用户命名空间 + 挂载命名空间**，每个连接独立一个命名空间：

- 进程视图与宿主隔离，看不到 `/home` 等宿主目录
- 不同客户端同时连接互不干扰
- `run_command` 执行的 Shell 命令（如 `cargo build`、`make`）也在此隔离环境中运行，构建产物不污染源码树
- 连接断开时，mount namespace 随进程退出自动销毁，不留垃圾文件

**Sandbox 模式额外叠加 overlayfs：**

```
┌─────────────────── Agent 进程看到的视图 ───────────────────────┐
│                     /workspace (merged)                       │
│   读：优先读 upper，upper 没有则穿透到 lower                   │
│   写：所有写操作由内核重定向到 upper（tmpfs），lower 不变       │
└───────────────────────────────────────────────────────────────┘
              ↑                          ↑
    upper layer (tmpfs)          lower layer (bind mount)
    所有写入落这里                 原始项目目录，内核求不可绕
    Agent 崩溃自动清理            始终不动
```

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

使用 Web UI 时，顶部 Header 会根据当前隔离模式显示对应徽标：

| 徽标 | 含义 |
|------|------|
| 🔒 **沙盒**（绿色）| Sandbox 模式，尚无待提交的改动 |
| 🔒 **沙盒 · N 待提交**（黄色）| Sandbox 模式，有 N 个文件已修改/新建，等待处理 |
| 🔲 **容器**（蓝色）| Container 模式（默认）|
| 🔓 **无容器**（灰色）| Normal 模式 |

点击「沙盒」徽标（或通过侧边栏「沙盒」Tab）可打开**沙盒面板**，其中提供：

- **改动列表**：显示所有 modified / created / deleted 文件及大小变化
- **全量提交**：一键将所有改动合并到原始项目
- **全量回滚**：一键撤销所有改动
- **逐文件提交** ✓：点击单个文件右侧的 ✓ 按钮，只将该文件合并到原始目录，其他改动继续保留在沙盒中——适合部分接受 Agent 修改的场景

### 典型工作流

**CLI 模式：**

```bash
# 1. 以 sandbox 模式启动
./target/release/agent --isolation sandbox --workdir /path/to/project

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

1. 启动服务器：`./target/release/agent --mode server --isolation sandbox`
2. 在 ConnectModal 填写服务器地址，选择隔离模式为「🔒 沙盒模式」，点击连接
3. 在聊天框输入任务，Agent 执行期间 Header 会显示「🔒 沙盒 · N 待提交」
4. 打开沙盒面板查看改动详情
5. 逐文件或全量提交/回滚

### 故障排查

**Q：连接后收到「⚠️ 沙盒模式请求失败」警告**

Sandbox 模式依赖内核 overlayfs（通过非特权 user namespace 挂载，无需 root）。若当前内核版本过低或 `CONFIG_OVERLAY_FS` 未启用，可尝试：

```bash
# 检查内核是否支持 overlayfs
grep OVERLAY_FS /boot/config-$(uname -r)  # 应显示 CONFIG_OVERLAY_FS=y 或 =m

# 若不支持，切换到 container 模式（仍有进程隔离，但写操作直接落真实项目）
./target/release/agent --mode server --isolation container
```

安装后重启 Agent 并重新连接即可。

**Q：容器内 `cargo`/`rustc` 命令找不到**

需在 `models.toml` 中添加 `extra_binds` 将 `~/.rustup` 和 `~/.cargo` 挂载进容器。详见上方「扩展工具链」小节。

**Q：`/commit` 后原始项目文件有变化，但 Git 没有显示 diff**

文件内容与提交前完全相同时，Git 不会报变更。可以用 `/changes` 确认沙盒内的改动是否非空，再决定是否提交。

---

## 🤖 多 Agent 协作（节点委派）

多 Agent 协作允许主 Agent 将复杂任务分解，并指派给运行在其他机器或子目录的 Agent 实例。

### 为什么需要多 Agent？

- **跨机器执行**：将 GPU 计算、测试构建等任务委派给专用沿界机/构建服务器。
- **Monorepo 支持**：将 `workdir` 锁定到子目录，避免误触全局代码。
- **专注度提升**：远端 Agent 只关注局部上下文，Token 消耗更低，响应更精准。

### 节点配置（workspaces.toml）

在 `~/.config/rust_agent/workspaces.toml`（全局）或 `.agent/workspaces.toml`（项目级）配置节点拓扑：

```toml
# 集群共享 token（可选，保护 /nodes 端点不被未授权访问）
[cluster]
token = "my-secret-token-123"

# ── 本机节点：运行在本 server 上，LLM 可直接 call_node target="<name>" ──────
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
tags        = ["embedded", "wifi"]

# ── 对等服务器：另一台 agent server，server 进程自动 probe 并展开子节点 ────
# LLM 看到的是展开后的 "节点名@peer名"，而不是这里的原始条目
[[peer]]
name = "gpu-box"
url  = "ws://192.168.1.20:9527"
token = "gpu-box-token"   # 可选

[[peer]]
name = "pi"
url  = "ws://raspberrypi.local:9527"
```

**配置说明：**

| 键类型 | 感知方 | 说明 |
|--------|--------|------|
| `[[node]]` | LLM + server | 本机可调用节点，有 `workdir`；LLM 可直接 `call_node` |
| `[[peer]]` | 仅 server | 对等服务器入口，有 `url`；LLM 只看到展开的子节点 |
| `[cluster]` | server | 集群共享 token，保护 `/nodes` 端点 |

**自动 probe 机制**：server 启动时并发 probe 所有 `[[peer]]`，将其子节点以 `name@peer` 格式写入节点注册表；30s 重试离线节点，120s 心跳保活在线节点。`call_node` 遇到离线节点时会自动触发一次重探。

**不配置此文件** = 通用 Agent，行为与未配置完全一致，可直连 URL 使用。

### `call_node` 统一接口

`call_node` 是唯一的 Agent 委派工具。调用前**先用 `list_nodes` 查看可用节点**。

#### target 寻址方式

| 格式 | 说明 | 示例 |
|------|------|------|
| 节点名称 | 通过父服务器 `/nodes` API 解析 | `"build-server"` |
| 直接 URL | 直连，无需配置 | `"ws://192.168.1.10:9527"` |
| `any:<tag>` | 路由表中第一个匹配节点 | `"any:gpu"` |
| `all:<tag>` | 广播给所有匹配节点 | `"all:embedded"` |

**调用示例**：
```json
{
  "target": "gpu-box",
  "prompt": "重构 frontend/src/components 目录下的所有 React 组件",
  "workdir": "frontend/src/components",
  "isolation": "sandbox",
  "auto_approve": false,
  "timeout_secs": 600
}
```

### 工具参数说明

| 参数 | 类型 | 说明 |
|------|------|------|
| `target` | string | 节点名、`ws://` URL 或 `any/all:<tag>`（**必填**） |
| `prompt` | string | 给远端 Agent 的任务说明（**必填**） |
| `workdir` | string | 覆盖远端节点的工作目录 |
| `isolation` | string | 覆盖远端隔离模式：`normal` / `container` / `sandbox` |
| `auto_approve` | bool | 自动批准远端工具调用（默认 false） |
| `timeout_secs` | int | 最大等待时间（默认 600 秒） |

### 透明度与安全

- **实时日志**：远端 Agent 的所有工具调用均实时回放到主 Agent 的输出界面。
- **授权代理**：当远端 Agent 需要写文件或跑命令时，主 Agent 会截获请求并展示确认提示，确保安全受控。
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
| `browser` | 🌐 | 浏览器自动化（Chrome DevTools Protocol） | ✅ |
| `call_node` | 🤖 | 委派任务给其他 Agent 节点（按名/URL/标签路由），manager 专用 | ✅ |
| `list_nodes` | 📶 | 列出当前可用的 Agent 节点（含在线状态），manager 专用 | ❌ |
| `load_skill` | 📚 | 加载项目技能（.agent/skills/） | ❌ |
| `create_skill` | ✍️ | 创建或更新项目技能 | ✅ |
| `connect_service` | 🔌 | 注册外部服务（WebSocket/HTTP） | ❌ |
| `query_service` | ❓ | 向已注册的外部服务发送请求 | ❌ |
| `subscribe_service` | 📡 | 订阅服务的推送事件 | ❌ |
| `unsubscribe_service` | 📡 | 取消服务订阅 | ❌ |
| `list_services` | 📋 | 列出所有已注册的外部服务 | ❌ |

### 外部依赖（可选）

部分工具依赖系统命令，Agent 会自动按优先级尝试：

| 工具 | 后端 | 安装方式 |
|------|------|----------|
| `read_pdf` | marker_single → pdftotext → mutool | `pip install marker-pdf` / `apt install poppler-utils` / `apt install mupdf-tools` |
| `browser` | Chrome / Chromium（headless CDP） | `apt install chromium` / `snap install chromium` |

---

## 🔌 MCP集成

Agent 支持两个方向的 MCP（Model Context Protocol）集成：

### 作为 MCP 工具服务器
见上文「[MCP 服务器模式](#mcp-服务器模式)」章节。

### 接入外部 MCP 服务器（MCP 客户端）

通过配置文件连接外部 MCP 服务器，其工具会自动添加到 Agent 工具列表，LLM 可和内置工具一起透明调用。

**配置文件位置（两者同时加载并合并，项目级优先）：**
- `.agent/mcp.toml` — 项目级
- `~/.config/rust_agent/mcp.toml` — 用户级

```toml
# .agent/mcp.toml

[[server]]
name    = "filesystem"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[[server]]
name    = "github"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-github"]
env     = { GITHUB_PERSONAL_ACCESS_TOKEN = "ghp_xxx" }

[[server]]
name    = "my-server"
command = "/usr/local/bin/my-mcp-server"
# args 和 env 均为可选
```

**配置字段说明：**

| 字段 | 必填 | 说明 |
|------|------|------|
| `name` | ✅ | 服务器别名，将作为工具前缀 |
| `command` | ✅ | 可执行文件（如 `npx`、`/usr/bin/my-mcp-server`） |
| `args` | ❌ | 命令行参数列表 |
| `env` | ❌ | 注入到子进程的额外环境变量 |

**工具命名规则：**`<server_name>__<tool_name>`，例如 `filesystem__read_file`、`github__search_repositories`。Agent 启动时自动拉起配置的 MCP 子进程，通过 JSON-RPC 2.0 over stdio 通信。

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
      --mode <MODE>            运行模式: cli / stdio / server / mcp [默认: cli]
      --host <HOST>            WebSocket 绑定地址 [默认: 127.0.0.1]
      --port <PORT>            WebSocket 端口 [默认: 9527]
      --max-iterations <N>     工具最大迭代次数 [默认: 25]
      --isolation <MODE>        隔离模式: normal / container / sandbox [默认: container]
  -h, --help                   显示帮助
```

---

## 📂 目录结构约定

### 用户配置目录 (`~/.config/rust_agent/`)

```
~/.config/rust_agent/
├── .env                    # 全局环境变量（API Key）
├── models.toml             # 模型管理配置
├── mcp.toml                # 全局 MCP 客户端配置（可选）
└── system_prompt.md        # 全局系统提示词（可选）
```

### 项目级目录 (`.agent/`)

```
your-project/
├── AGENT.md                # 全局项目指令（自动加载）
└── .agent/
    ├── memory.md           # 持久记忆（自动维护）
    ├── summary.md          # 项目摘要（/summary 生成）
    ├── system_prompt.md    # 项目级系统提示词（可选，# OVERRIDE 可替换默认）
    ├── mcp.toml            # MCP 客户端配置（可选）
    ├── skills/             # 项目级 Skills
    │   ├── coding-style.md
    │   └── my-tool/        # 目录 Skill（SKILL.md + tool.json + 脚本）
    └── plugins/            # 插件目录（可选）
        ├── project-stats/  # 示例：Git 统计 + 审计日志 + 路由优化
        └── dev-cluster/    # 示例：多节点拓扑 + MCP 配置
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
