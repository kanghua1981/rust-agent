# 🤖 Rust Coding Agent

一个用 Rust 编写的 AI 编码助手 CLI 工具，类似 Claude Code。它可以读写文件、执行命令、搜索代码，并通过 LLM 进行智能交互。

## ✨ 特性

- **🔧 工具系统**: 内置 22 种工具 — 文件读写、多文件批量读取、精确编辑与批量编辑、命令执行、代码/文件搜索、目录列表、PDF/电子书读取、浏览器自动化、内部推理、外部服务连接；另支持动态脚本工具（tool.json）
- **🔄 Agent 循环**: 自动编排 LLM 调用与工具执行，多轮迭代直到任务完成
- **📋 Plan 模式**: `/plan` 命令先用只读工具分析项目，生成方案后再执行，避免盲目修改
- **🔀 多角色流水线**: 配置独立的 Planner / Executor / Checker 角色各用不同模型，自动路由，完全透明
- **⚡ 执行前背景注入**: approve 计划时可附带背景上下文，直达 Executor 初始 prompt
- **🛑 执行中实时指导**: Pipeline 运行时按 `Ctrl+\` 随时暂停并向 Executor 注入补充信息
- **�🎨 终端 UI**: 彩色输出、Markdown 渲染、Diff 预览、友好的交互界面
- **📡 五种运行模式**: CLI 交互（默认）、**TUI 分屏界面**（ratatui）、JSON-over-stdio 协议、WebSocket 服务器、**MCP 工具服务器**（Claude Desktop / Cursor 直接接入）
- **🔌 MCP 双向支持**: **作为 MCP 服务器**（`--mode mcp`）向任何 MCP 主机暴露全部内置工具；**作为 MCP 客户端**（`mcp.toml`）自动连接外部 MCP 服务器并将其工具注册到 Agent 工具列表，LLM 透明调用
- **🌐 多 Provider 支持**: Anthropic Claude、OpenAI GPT、以及任何兼容的 API
- **🤖 模型管理**: 通过 `models.toml` 配置多个模型，运行时 `/model` 命令热切换
- **📜 对话持久化**: 支持上下文保持、会话保存与恢复
- **📚 Skills 系统**: 通过 Markdown 文件注入项目级别的专家知识；兼容 [OpenClaw AgentSkills](https://agentskills.io/) 格式（`SKILL.md`），可直接使用社区发布的技能包
- **🧩 动态工具**: 在 Skill 目录加一个 `tool.json` 即可注册新工具，参数 JSON 通过 stdin 传递给任意 shell 脚本/可执行文件
- **🧠 持久记忆**: 自动记录所有工具操作到 `.agent/memory.md`，跨会话保持
- **📋 项目摘要**: 通过 `/summary` 命令生成项目概述，跨会话复用
- **✏️ 自定义系统提示词**: 支持全局和项目级别的 `system_prompt.md` 定制 LLM 行为
- **🔒 安全确认**: 文件写入和命令执行前需用户确认，auto-approve 时也有可见提示
- **🛡️ 三种隔离模式**: `--isolation normal/container/sandbox` 按需选择；`normal` 直接运行、`container` namespace+rootfs 隔离直写、`sandbox` 额外叠加 overlayfs 保护（`/changes` · `/rollback` · `/commit`，Web UI 支持逐文件提交）；`extra_binds` 配置将 rustup/cargo/node 等工具链挂载进容器
- **🤖 多 Agent 协作**: 通过 `call_node` 工具实现任务委派，支持按名称、直连 URL、标签路由和广播四种寻址方式（`any:<tag>` / `all:<tag>`）；实时事件代理及授权转发。先用 `list_nodes` 查看可用节点
- **🛡️ 上下文安全截断**: 智能保持 tool_use/tool_result 配对完整性，避免 API 错误
- **⚡ 高性能**: Rust 原生实现，启动快速，资源占用低

---

## 📖 完整使用指南

### 第一步：安装与构建

```bash
# 克隆项目
git clone <repo-url>
cd rust_agent

# 编译（推荐 release 模式）
cargo build --release

# 编译产物在 target/release/agent
```

### 第二步：配置 API Key

Agent 需要一个 LLM API Key 才能工作。支持多种配置方式：

```bash
# 方式一：环境变量（推荐）
export ANTHROPIC_API_KEY=sk-ant-xxxxx

# 方式二：.env 文件（持久化）
# 支持三个位置（后面的覆盖前面的）：
#   ~/.config/rust_agent/.env  — 全局配置（XDG 规范）
#   ~/.env                     — 用户级配置
#   项目目录/.env              — 项目级配置（最高优先级）
cp .env.example .env
vim .env  # 填入你的 API Key

# 方式三：使用 OpenAI 或兼容 API (如通义千问 Qwen, DeepSeek)
export OPENAI_API_KEY=sk-xxxxx
# 或
export LLM_API_KEY=your-key
export LLM_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
export LLM_MODEL=qwen-max
export LLM_PROVIDER=compatible
```

> **注意**: 现在的版本已支持 OpenAI/Compatible Provider 的流式输出 (Streaming)，交互体验更佳。

### 模型管理（推荐）

对于需要频繁切换模型的用户，推荐使用 `~/.config/rust_agent/models.toml` 统一管理：

```toml
# 默认使用的模型别名
default = "sonnet"

# 多 Agent 专家池 (可选)
# 启动时自动在后台拉起这些端口的服务进程
[sub_agents.coder]
port = 9001
role = "代码实现专家"

[sub_agents.reviewer]
port = 9002
role = "代码审查专家"

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
api_key = "sk-xxxxx"  # 可选，不设则 fallback 到环境变量
```

也可以在运行时通过 `/model add <alias>` 交互式添加模型，无需手动编辑文件。

**配置优先级**：`--model CLI参数` > `models.toml default` > `LLM_MODEL 环境变量` > `硬编码默认值`

> **提示**: `.env` 文件仍然有效，推荐只放 API Key；模型/provider/base_url 的管理交给 `models.toml`。

### 多角色流水线（可选）

在 `models.toml` 中配置角色，让不同 LLM 分别负责**规划 / 实施 / 审核**：

```toml
# 开启流水线（设为 true 后所有用户输入自动走三角色流程，无需新命令）
[pipeline]
enabled = false
stages = ["planner", "executor", "checker"]
max_checker_retries = 2       # Checker 审核失败后允许 Executor 重试次数
require_plan_confirm = true   # Planner 完成后展示计划，等待用户确认再执行

# 规划者：使用顶配推理模型，只读探索代码库，产出带风险提示的执行计划
[roles.planner]
model = "sonnet"
extra_instructions = """
优先关注 Rust 所有权规则和借用检查器可能引发的问题。
"""

# 实施者：使用成本更低的编码模型，按计划严格执行，产出修改摘要
[roles.executor]
model = "deepseek"
extra_instructions = """
每次修改后运行 cargo build 验证编译通过。
"""

# 审核者：独立阅读实际文件内容，不依赖实施者自述，运行测试后给出 PASS/FAIL
[roles.checker]
model = "sonnet"
```

**如何查看当前角色配置**：运行 `/model` 命令，会显示流水线状态和各角色绑定的模型。

**自定义角色提示词**：提示词优先级为：
`内置默认` → `~/.config/rust_agent/roles/<role>.md` → `.agent/roles/<role>.md` → `models.toml extra_instructions`

详细设计见 [docs/MULTI_ROLE_DESIGN.md](docs/MULTI_ROLE_DESIGN.md)。

### 第三步：启动 Agent

```bash
# 交互模式（最常用）
./target/release/agent

# 带初始提示启动
./target/release/agent --prompt "帮我看看当前目录有什么文件"

# 指定工作目录（Agent 会在此目录下操作文件）
./target/release/agent --workdir /path/to/your/project

# 使用 OpenAI 模型
./target/release/agent --provider openai --model gpt-4o

# 使用自托管/兼容 API
./target/release/agent --provider compatible --model my-model

# 跳过所有确认提示（⚠️ 危险，适合自动化场景）
./target/release/agent --yes

# 限制工具最大迭代次数（默认 25）
./target/release/agent --max-iterations 50

# 开启详细日志（调试用）
./target/release/agent --verbose

# 分屏 TUI 界面（ratatui，输入输出完全解耦）
./target/release/agent --mode tui
```

#### Stdio 模式（脚本 / VS Code 集成）

通过 `--mode stdio` 切换为 JSON-over-stdio 协议，每个事件以独立的 JSON 行输出，适合被外部程序驱动：

```bash
# 以 stdio 模式运行单次任务
./target/release/agent --mode stdio --yes -p "列出当前目录结构"

# 输出示例（每行一个 JSON 事件）：
# {"type":"thinking","data":{}}
# {"type":"stream_start","data":{}}
# {"type":"streaming_token","data":{"token":"当前"}}
# {"type":"streaming_token","data":{"token":"目录"}}
# ...
# {"type":"tool_use","data":{"tool":"list_directory","input":{"path":"."}}}
# {"type":"tool_result","data":{"tool":"list_directory","output":"...","is_error":false}}
# {"type":"stream_end","data":{}}
```

**Stdio 模式事件类型**：

| 事件类型 | 说明 |
|----------|------|
| `thinking` | LLM 正在处理 |
| `stream_start` / `stream_end` | 流式文本响应的开始/结束 |
| `streaming_token` | 流式文本 token（`data.token`） |
| `assistant_text` | 非流式文本响应（`data.text`） |
| `tool_use` | 即将执行工具（`data.tool`, `data.input`） |
| `tool_result` | 工具执行结果（`data.output`, `data.is_error`） |
| `diff` | 文件变更 diff（`data.path`, `data.diff`） |
| `confirm_request` | 请求确认（需回复 `{"approved": true}` 或 `{"approved": false}`） |
| `warning` | 非致命警告 |
| `error` | 错误信息 |
| `context_warning` | 上下文窗口压力通知 |

#### 多 Agent 协作模式

Agent 通过 `call_node` 工具实现任务委派，支持将复杂任务分发给其他 Agent 实例：

**target 参数寻址方式**
```bash
# 1. 按名称（优先用 list_nodes 查看可用节点）
target = "build-server"

# 2. 直接 WebSocket URL
target = "ws://192.168.1.10:9527"

# 3. 标签路由：自动分配给第一个匹配的节点
target = "any:gpu"

# 4. 广播：向所有匹配节点广播
target = "all:embedded"
```

**支持指定远端隔离模式**和工作目录，远端 Agent 的所有工具调用均实时回放到主 Agent。用前调用 `list_nodes` 确认可用节点名称。

**节点配置（workspaces.toml）**

在 `~/.config/rust_agent/workspaces.toml`（全局）或 `.agent/workspaces.toml`（项目级）配置多 Agent 拓扑：

```toml
# 集群共享 token（可选）
[cluster]
token = "my-secret-token-123"

# 本机节点：LLM 可直接 call_node target="<name>"
[[node]]
name        = "upper-sdk"
workdir     = "/home/user/upper-project"
description = "上位机 SDK 工程（Qt + C++）"
tags        = ["upper", "cpp"]
sandbox     = false

[[node]]
name    = "firmware"
workdir = "/home/user/firmware"
tags    = ["embedded"]

# 对等服务器：server 进程自动 probe，LLM 看到展开的 "子节点@peer" 记录
[[peer]]
name  = "gpu-box"
url   = "ws://192.168.1.20:9527"
token = "gpu-box-token"

[[peer]]
name = "pi"
url  = "ws://raspberrypi.local:9527"
```

> **`[[node]]`**：本机可调用节点，LLM 直接感知。**`[[peer]]`**：对等 server 入口，仅 server 进程感知，LLM 看到展开的子节点。不配置 = 通用 Agent，行为不变。

#### WebSocket 服务器模式（远程 / Web UI 集成）

通过 `--mode server` 启动 WebSocket 服务器，每个连接独立运行一个 Agent 实例：

```bash
# 启动 WebSocket 服务器（默认 127.0.0.1:9527）
./target/release/agent --mode server

# 指定地址和端口
./target/release/agent --mode server --host 0.0.0.0 --port 8080

# 带项目目录
./target/release/agent --mode server --workdir /path/to/project
```

**Web UI 客户端**：我们提供了一个基于 React 的 Web 界面，支持：
- 🗨️ 实时聊天对话
- 📁 文件浏览器
- 🛠️ 工具调用监控
- ⚙️ 工作目录设置
- 🔧 模型配置管理

启动 Web UI：
```bash
cd web-ui
./start-simple.sh  # 或 npm run dev
```

访问 http://localhost:3000 即可使用 Web 界面。

**客户端通信协议**：
- 发送用户消息：`{"type": "user_message", "content": "你的问题"}`
- 响应确认请求：`{"type": "confirm_response", "approved": true}`
- 服务端事件格式与 Stdio 模式相同（JSON 帧）

#### TUI 模式（分屏终端界面）

通过 `--mode tui` 启动基于 [ratatui](https://github.com/ratatui-org/ratatui) 的分屏 TUI 界面：

```bash
# 启动 TUI 模式
./target/release/agent --mode tui

# 带初始提示
./target/release/agent --mode tui --prompt "帮我看看项目结构"

# 跳过确认提示（适合自动化）
./target/release/agent --mode tui --yes
```

**界面布局**：

```
┌────────────────────────────────────────┐
│  输出区（可滚动，显示所有 Agent 输出）  │
├────────────────────────────────────────┤
│  状态栏（● 思考中… / ✓ 就绪）         │
├────────────────────────────────────────┤
│  > [输入框 — 始终激活]                 │
└────────────────────────────────────────┘
```

**快捷键**：

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 |
| `↑` / `↓` | 命令历史 |
| `PgUp` / `PgDn` | 翻页滚动输出区 |
| `鼠标滚轮` | 滚动输出区 |
| `Ctrl+C` | 中断 Agent（Agent 空闲时退出） |
| `Ctrl+Q` | 退出 TUI |
| `Ctrl+L` | 清空输出区 |

**特点**：
- 输入框**始终可用**，agent 处理上一条消息时可继续输入下一条（自动排队）
- 所有斜杠命令（`/plan`、`/model`、`/mode`、`/quit` 等）在 TUI 模式下同样有效
- 支持彩色输出与 streaming 实时显示
- 支持鼠标滚轮滚动
- 自动滚动与手动滚动模式切换

#### MCP 服务器模式（Claude Desktop / Cursor 等接入）

通过 `--mode mcp` 将 Agent 变为一个标准 MCP 工具服务器，外部 MCP 主机（Claude Desktop、Cursor 等）可以直接列举并调用全部内置工具，**无需在这一侧运行任何 LLM**：

```bash
# 以 MCP 服务器模式启动
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

支持方法：`initialize` · `tools/list` · `tools/call`（JSON-RPC 2.0 over stdio）。所有工具权限均**自动 approve**，适合作为受信任的本地工具提供者。

---

## 💬 日常使用流程

### 基本交互

启动后进入 REPL 模式，`🤖 >` 是输入提示符：

```
🤖 > 帮我看看这个项目的目录结构
⏳ Thinking...
📂 Tool: list_directory ...
────────────────────────────
这个项目包含以下文件...
────────────────────────────

🤖 > 把 main.c 里的 GPIO 初始化改成上拉模式
⏳ Thinking...
📖 Tool: read_file → main.c
⚠️  Confirm edit_file on main.c? [y/N/a]  y
✏️ Tool: edit_file → main.c
   --- a/main.c
   +++ b/main.c
   @@ -42,1 +42,1 @@
   - GPIO_Init(GPIOA, GPIO_PIN_5, GPIO_MODE_INPUT);
   + GPIO_Init(GPIOA, GPIO_PIN_5, GPIO_MODE_INPUT | GPIO_PULLUP);
────────────────────────────
已将 GPIO5 修改为上拉输入模式。
────────────────────────────

🤖 > 编译试试看
⏳ Thinking...
⚠️  Confirm run_command: make ? [y/N/a]  y
🔨 Tool: run_command → make
   ✅ Build successful
```

### 多轮对话（反复讨论与修改）

Agent 会**记住整个对话上下文**，你可以像聊天一样反复迭代：

```
🤖 > 帮我修改设备树里 GPIO4_A1 的配置
（Agent 修改了 .dts 文件）

🤖 > 不对，应该是 Active Low，你改反了
（Agent 读取上下文，知道你指的是刚才的修改，自动修正）

🤖 > 改好后帮我编译一下设备树
（Agent 执行 make dtbs 并反馈结果）

🤖 > 编译报错了，你看看什么问题
（Agent 读取错误信息，自动定位问题并修复）
```

### 配合 Git 进行代码审查

Agent 没有内置专门的 Git 工具，但它可以通过 `run_command` 执行任何 Git 命令。推荐工作流：

```
# 1. 启动前：创建工作分支
git checkout -b fix/gpio-pullup

# 2. 启动 Agent，指定项目目录
./target/release/agent --workdir /path/to/project

# 3. 让 Agent 帮你修改代码
🤖 > 帮我把所有 GPIO 初始化改为带上拉配置

# 4. 修改完后，让 Agent 汇报改了什么
🤖 > 帮我执行 git diff，看看改了哪些内容
🤖 > 帮我执行 git status

# 5. 确认没问题后，让 Agent 提交
🤖 > 帮我 git add 所有修改的文件，commit 消息写 "fix: enable GPIO pull-up"

# 6. 退出 Agent，回到 VS Code 进行最终 Review
🤖 > /quit

# 7. 在 VS Code 中查看（推荐）
#    - Source Control 面板查看所有变更
#    - 点击文件名查看 Diff
#    - 确认后 Push
git push origin fix/gpio-pullup
```

---

## 🔧 内置命令

在 `🤖 >` 提示符下输入以下斜杠命令：

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助信息 |
| `/clear` | 清空对话历史（开始新话题时使用） |
| `/usage` | 显示本次会话的 Token 消耗量 |
| `/context` | 查看上下文窗口使用率（接近 80% 时会自动截断） |
| `/save` | 手动保存当前会话 |
| `/sessions` | 列出所有已保存的会话 |
| `/skills` | 查看当前加载的 Skills |
| `/yesall` | 关闭所有确认提示（本次会话内有效） |
| `/confirm` | 重新开启确认提示 |
| `/mode` | 查看或设置执行模式：simple/plan/pipeline/auto |
| `/mode <simple|plan|pipeline|auto>` | 设置执行模式 |
| `/model` | 列出当前模型与所有已配置模型 |
| `/model <alias>` | 热切换到指定模型 |
| `/model add <alias>` | 交互式添加新模型配置 |
| `/model remove <alias>` | 删除模型配置 |
| `/model default <alias>` | 设置默认模型 |
| `/memory` | 显示持久记忆（项目知识、文件操作记录） |
| `/summary` | 查看或生成项目摘要 |
| `/summary generate` | 强制（重新）生成项目摘要 |
| `/plan <任务>` | 让 Agent 先用只读工具分析，生成执行计划 |
| `/plan show` | 查看当前待执行的计划 |
| `/plan run` | 执行已生成的计划 |
| `/plan clear` | 清除当前计划 |
| `/quit` | 退出（会自动保存会话） |

---

## 📜 会话管理

Agent 支持**自动保存**对话，适合跨天的长任务：

```bash
# 列出所有历史会话
./target/release/agent --sessions

# 输出示例：
# 📜 Saved Sessions:
#   ID         Updated                  Msgs   Summary
#   ──────────────────────────────────────────────
#   a1b2c3d4   2026-02-15T10:30:00      12     帮我修改 GPIO 配置...
#   e5f6g7h8   2026-02-14T16:20:00       8     编译内核模块...

# 恢复某个会话继续工作
./target/release/agent --resume a1b2c3d4
```

---

## 🧠 记忆与项目摘要

### 持久记忆 (`.agent/memory.md`)

Agent 会自动将以下信息记录到 `.agent/memory.md`，跨会话持久化：

- **项目知识**：在对话中发现的重要事实
- **文件操作记录**：读取、写入、编辑过的文件
- **会话日志**：执行过的关键操作

```
🤖 > /memory
🧠  Agent Memory (15 entries):
  📖 Project Knowledge:
    • Target board: RK3588 custom board
    • Toolchain: aarch64-linux-gnu-
  📁 Key Files:
    • src/main.c (edited)
    • kernel/arch/arm64/boot/dts/rockchip/rk3588-myboard.dts (written)
  📝 Session Log:
    • edited src/main.c
    • ran `make -j8`
```

### 项目摘要 (`.agent/summary.md`)

首次使用时，运行 `/summary` 让 Agent 扫描项目并生成概述，后续会话自动加载：

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

Skills 是项目级别的知识文件，让 Agent "理解"你的项目规范。

### 目录结构

```
your-project/
├── AGENT.md                    # 全局项目指令（自动加载，原生格式）
├── SKILL.md                    # 等效入口（OpenClaw AgentSkills 兼容格式）
└── .agent/
    └── skills/
        ├── modify-dts-gpio.md  # 单文件 Skill（原生格式）
        ├── cross-compile/       # 目录 Skill
        │   ├── SKILL.md        # 入口（OpenClaw AgentSkills 兼容）或 README.md
        │   └── scripts/        # 关联资源文件
        └── add-driver.md       # 驱动开发规范
```

### Skill 文件示例

`AGENT.md`（全局指令）：
```markdown
# Project: Embedded Linux BSP

- Target: RK3588 based custom board
- Toolchain: aarch64-linux-gnu-
- Kernel source: kernel/
- Device tree: kernel/arch/arm64/boot/dts/rockchip/
- After any code change, run `make -j$(nproc)` to verify compilation
```

`.agent/skills/modify-dts-gpio.md`（GPIO 修改技能）：
```markdown
# Skill: Modify GPIO in Device Tree

## File locations
- Board DTS: kernel/arch/arm64/boot/dts/rockchip/rk3588-myboard.dts
- Pin control includes: kernel/include/dt-bindings/pinctrl/rockchip.h

## Conventions
- Use rockchip,pins format: <bank RK_PXn function &config>
- Always specify pull-up/pull-down explicitly
- Do NOT modify .dtsi files, only the board .dts

## Verification
After editing, run: make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- dtbs
```

### 查看已加载的 Skills

```
🤖 > /skills
📋  3 skill(s) loaded:
  • Project Instructions (AGENT.md)
  • Modify Dts Gpio (.agent/skills/modify-dts-gpio.md)
  • Cross Compile (.agent/skills/cross-compile.md)
```

### OpenClaw AgentSkills 兼容

本项目的 Skills 系统兼容 [OpenClaw AgentSkills](https://agentskills.io/) 标准格式，可以**直接使用**为 OpenClaw 编写的技能包，无需修改：

- **目录 Skill**：子目录内的 `SKILL.md`（OpenClaw 格式）和 `README.md`（原生格式）均被识别
- **根目录 Skill**：根目录下的 `SKILL.md` 等同于 `AGENT.md`，随会话自动注入
- **frontmatter 字段**：`name`、`description` 字段完全兼容；OpenClaw 特有的 `metadata`（环境门控、平台过滤）字段被优雅忽略
- **关联文件**：目录 Skill 中除入口文件外的所有资源都会在 Skill 内容末尾列出，方便 Agent 精准引用

**从 OpenClaw 社区安装 Skill（示例）：**
```bash
# 把任意 AgentSkills 格式技能包放入 .agent/skills/ 即可
cp -r ~/Downloads/my-skill/ .agent/skills/
# 或直接 git clone
git clone https://github.com/example/my-skill .agent/skills/my-skill
```

### 🧩 动态工具（tool.json）

在任意 Skill 目录里放一个 `tool.json`，Agent 启动时会自动扫描并注册为可调用工具，无需修改任何 Rust 代码。

**`tool.json` 格式：**
```json
{
  "name": "query-db",
  "description": "根据 SQL 查询数据库并返回结果",
  "parameters": {
    "type": "object",
    "properties": {
      "sql": { "type": "string", "description": "SQL 查询语句" }
    },
    "required": ["sql"]
  },
  "command": "./query.sh",
  "timeout_secs": 30
}
```

**执行合同：**
- Agent 调用工具时，LLM 传来的参数会被序列化为 JSON 并写入脚本的 **stdin**
- 脚本工作目录为 Skill 目录，相对路径均有效
- stdout 作为工具返回值，非零退出码会返回错误

**目录结构示例：**
```
.agent/skills/
└── query-db/
    ├── SKILL.md      # 工具使用说明（注入系统 prompt）
    ├── tool.json     # 工具定义（自动注册）
    └── query.sh      # 实际执行的脚本
```

`query.sh` 从 stdin 读取 JSON 参数：
```bash
#!/bin/bash
params=$(cat)  # 读取 stdin JSON
sql=$(echo "$params" | jq -r '.sql')
sqlite3 ./data.db "$sql"
```

或用 Python：
```python
#!/usr/bin/env python3
import sys, json
params = json.load(sys.stdin)
result = run_query(params['sql'])
print(result)
```

**扫描路径**（两者均支持）：
- `.agent/skills/*/tool.json` — 原生格式
- `skills/*/tool.json` — OpenClaw AgentSkills 目录布局兼容

---

## 🔌 支持的 LLM Provider

| Provider | 启动参数 | 环境变量 |
|----------|----------|----------|
| Anthropic Claude（默认） | `--provider anthropic` | `ANTHROPIC_API_KEY` |
| OpenAI | `--provider openai` | `OPENAI_API_KEY` |
| 兼容 API（Ollama 等） | `--provider compatible` | `LLM_API_KEY` + `LLM_BASE_URL` |

> **推荐**：使用 `~/.config/rust_agent/models.toml` 管理多个模型，运行时通过 `/model <alias>` 快速切换，无需重启 Agent。详见上文「模型管理」章节。

---

## 🛡️ 安全机制

Agent 在执行以下操作前会要求确认：

- **写入文件** (`write_file`)：显示文件路径和行数
- **编辑文件** (`edit_file` / `multi_edit_file`)：显示文件路径，执行后展示 Diff
- **执行命令** (`run_command`)：显示完整命令内容

```
⚠️  Write file: src/driver.c (45 lines)
   Proceed? [y]es / [n]o / [a]lways:
```

- 输入 `y`：确认本次操作
- 输入 `n`：拒绝（Agent 会知道你拒绝了，并调整策略）
- 输入 `a`：本次会话内跳过所有确认

也可以通过 `--yes` 启动参数或 `/yesall` 命令全局跳过。auto-approve 时会显示 `⚡ auto-approved:` 提示，让你知道跳过了什么。

只读工具（`read_file`、`grep_search`、`list_directory`、`batch_read_files`、`read_pdf`、`think`、`file_search`、`list_nodes`、`load_skill`、`connect_service`、`query_service`、`subscribe_service`、`unsubscribe_service`、`list_services`）不需要确认。

---

## 🛡️ 隔离模式

Agent 支持三种隔离模式，按安全需求选择：

| 模式 | 参数值 | rootfs | overlayfs | /rollback | 适用场景 |
|------|--------|--------|-----------|-----------|----------|
| **Normal** | `normal` | ✗ | ✗ | ✗ | 本地开发、完全信任的自动化 |
| **Container** | `container` | ✅ | ✗ | ✗ | 进程隔离、直写结果（**默认**）|
| **Sandbox** | `sandbox` | ✅ | ✅ | ✅ | overlayfs 保护，支持回滚与提交 |

### 启用方式

**服务器启动（全局生效）：**

```bash
# 默认使用 container 模式
./target/release/agent --mode server

# 指定 sandbox 模式（所有连接均受 overlay 保护）
./target/release/agent --mode server --isolation sandbox

# normal 模式（无容器，兼容性最好）
./target/release/agent --mode server --isolation normal
```

**Web UI / WebSocket 客户端逐连接指定：**

在 ConnectModal 的「隔离模式」下拉菜单中选择，或在 WebSocket URL 中附加参数：

```
# 新参数（推荐）
ws://localhost:9527?mode=sandbox&workdir=/your/project
ws://localhost:9527?mode=normal&workdir=/your/project

# 向后兼容（sandbox=1 映射到 sandbox 模式）
ws://localhost:9527?sandbox=1&workdir=/your/project
```

### 隔离实现

Container 与 Sandbox 模式均使用 **Linux 用户命名空间 + 挂载命名空间**，worker 进程在隔离的 rootfs 内运行：

- 进程视图与宿主分离，看不到 `/home` 等宿主目录
- `run_command` 执行的 Shell 命令也在此命名空间内运行
- 连接断开时，mount namespace 随进程退出自动销毁，不留垃圾文件

**Sandbox 模式额外叠加 overlayfs：**

```
原始项目目录（lower layer，内核保证不可绕过写入）
        +
tmpfs 上层（upper layer，所有写入落这里）
        =
Agent 看到的完整视图（/workspace，读写透明）
```

### 扩展工具链（`extra_binds`）

容器默认只挂载系统标准路径。若项目需要 `rustup`、`cargo`、`node` 等非标准工具链，可在 `~/.config/rust_agent/models.toml` 中配置额外挂载：

```toml
# 挂载 rustup 工具链（需写权限以支持缓存）
[[extra_binds]]
src = "/home/user/.rustup"
dst = "/root/.rustup"
readonly = false

[[extra_binds]]
src = "/home/user/.cargo"
dst = "/root/.cargo"
readonly = false

# 挂载只读数据目录
[[extra_binds]]
src = "/opt/datasets"
dst = "/datasets"
readonly = true
```

### 沙盒命令（Sandbox 模式专属）

| 命令 | 说明 |
|------|------|
| `/changes` | 列出所有已修改 / 新增 / 删除的文件（含大小变化） |
| `/rollback` | 撤销**全部**改动，恢复到沙盒建立时的状态 |
| `/commit` | 将 overlay 上层的所有变更合并写入原始项目目录 |

Web UI 的「沙盒」面板还支持**逐文件提交**：点击单个文件右侧的 ✓ 按钮，只将该文件合并到原始目录，其余改动继续保留在沙盒中。

### 状态标识（Web UI）

Header 会根据当前隔离模式显示对应徽标：

- 🔒 **沙盒**（绿色）：Sandbox 模式，无待提交改动
- 🔒 **沙盒 · N 待提交**（黄色）：Sandbox 模式，有 N 个文件已修改
- 🔲 **容器**（蓝色）：Container 模式（默认）
- 🔓 **无容器**（灰色）：Normal 模式

### 典型工作流

```bash
# 1. 以 sandbox 模式启动（--isolation sandbox 对所有连接生效）
./target/release/agent --mode server --isolation sandbox

# 2. 在 Web UI 连接并指派任务（所有修改都在隔离层）
# 或：CLI 模式
./target/release/agent --isolation sandbox --workdir /path/to/project

# 3. 查看 Agent 做了什么
🤖 > /changes

# 4a. 满意 → 提交到真实项目
🤖 > /commit

# 4b. 不满意 → 全部回滚
🤖 > /rollback
```

---

## 🏗️ 架构

```
src/
├── main.rs          # 入口：CLI 参数解析 (clap)，--mode 选择输出后端，.env 加载
├── config.rs        # 配置管理（API Key、Provider、模型参数、角色 Config 构造）
├── model_manager.rs # 模型管理（models.toml 读写、RoleConfig、PipelineConfig）
├── output.rs        # ★ AgentOutput trait + CliOutput / StdioOutput / WsOutput 实现
├── cli.rs           # 交互式 REPL 循环 (rustyline)，斜杠命令处理
├── agent.rs         # Agent 核心：LLM 调用 + Tool 编排 + Plan 模式 + 角色分发
├── pipeline.rs      # (Phase 3) 多角色流水线 Runner，Artifact 传递与反馈环
├── conversation.rs  # 对话历史：Message / ContentBlock 数据模型，system prompt 构建
├── context.rs       # 上下文窗口管理、自动截断（保持 tool_use/tool_result 配对完整）
├── streaming.rs     # Anthropic SSE 流式输出（通过 &dyn AgentOutput 解耦）
├── server.rs        # WebSocket 服务器：per-connection Agent 生命周期
├── confirm.rs       # 危险操作的用户确认机制（含 auto-approve 可视化）
├── persistence.rs   # 会话保存与恢复 (JSON)
├── diff.rs          # 文件修改的 Diff 展示
├── memory.rs        # 持久记忆系统（.agent/memory.md）
├── summary.rs       # 项目摘要管理（.agent/summary.md）
├── skills.rs        # Skills 加载系统（AGENT.md / SKILL.md + .agent/skills/），兼容 OpenClaw AgentSkills 格式
├── workspaces.rs    # 多 Agent 节点配置解析（workspaces.toml）+ NodeRegistry 全局状态
├── worker.rs        # worker 子进程入口（收 WebSocket 连接并运行 Agent）
├── sandbox.rs       # Container / Sandbox 隔离实现（Linux namespace + overlayfs）
├── ui.rs            # 终端 UI 输出（颜色、Markdown 渲染，UTF-8 安全截断）
├── llm/
│   ├── mod.rs       # LlmClient trait 定义
│   ├── anthropic.rs # Anthropic Claude API 实现
│   └── openai.rs    # OpenAI 兼容 API 实现
└── tools/
    ├── mod.rs             # Tool trait + ToolExecutor 注册中心 + readonly_definitions
    ├── read_file.rs       # 📖 读取文件（支持行范围）
    ├── write_file.rs      # ✏️ 写入/创建文件
    ├── edit_file.rs       # 🔧 精确替换文件内容（find & replace）
    ├── multi_edit_file.rs # 🔧 单文件多处批量编辑
    ├── batch_read.rs      # 📚 批量读取多个文件
    ├── run_command.rs     # ⚡ 执行 Shell 命令（含超时控制）
    ├── search.rs          # 🔍 Grep 搜索 + 📁 文件名搜索
    ├── list_dir.rs        # 📂 列出目录内容（含文件大小、权限）
    ├── think.rs           # 💭 内部推理（无副作用，不消耗工具配额）
    ├── read_pdf.rs        # 📄 PDF 文本提取（marker / pdftotext / mutool）
    ├── browser.rs         # 🌐 浏览器自动化（Chrome DevTools Protocol）
    ├── call_node.rs       # 🤖 Agent 任务委派（名称/URL/标签路由，manager 专用）
    ├── list_nodes.rs      # 📶 查看可用 Agent 节点（含在线状态，manager 专用）
    ├── connect_service.rs # 🔌 注册外部服务（WebSocket/HTTP）
    ├── query_service.rs   # ❓ 向已注册服务发送请求
    ├── subscribe_service.rs # 📡 订阅/取消订阅服务推送
    ├── list_services.rs   # 📋 列出已注册服务
    ├── load_skill.rs      # 📚 加载项目技能
    ├── create_skill.rs    # ✍️ 创建或更新项目技能
    └── script_tool.rs     # 🧩 动态脚本工具（扫描 tool.json，stdin JSON 协议）
```

### 输出抽象层

`output.rs` 中定义了 `AgentOutput` trait，所有 Agent 内部的 I/O（文本输出、工具事件、Diff 预览、确认提示）都通过此 trait 抽象，而非直接写 stdout：

```
┌──────────────┐
│   main.rs    │  --mode cli    → Arc<CliOutput>   → 彩色终端交互
│  (选择模式)   │  --mode stdio  → Arc<StdioOutput> → JSON 行协议
│              │  --mode server → Arc<WsOutput>    → WebSocket JSON 帧
└──────┬───────┘
       │ Arc<dyn AgentOutput>
       ▼
┌──────────────┐     ┌──────────────┐
│   agent.rs   │────▶│ streaming.rs │
│  (核心循环)   │     │  (SSE 解析)   │
└──────────────┘     └──────────────┘
  output.on_tool_use()    output.on_streaming_text()
  output.on_diff()        output.on_stream_start()
  output.confirm()        output.on_stream_end()
```

要添加新的输出模式（如 MCP Server），只需实现 `AgentOutput` trait 即可，无需修改 Agent 逻辑。

---

## 🧰 内置工具一览

| 工具 | 图标 | 用途 | 需确认 |
|------|------|------|--------|
| `read_file` | 📖 | 读取文件内容（支持行范围选择） | ❌ |
| `batch_read_files` | 📚 | 一次读取多个文件 | ❌ |
| `write_file` | ✏️ | 创建或覆盖写入文件 | ✅ |
| `edit_file` | 🔧 | 精确 find & replace 编辑 | ✅ |
| `multi_edit_file` | 🔧 | 单文件多处批量编辑 | ✅ |
| `run_command` | ⚡ | 执行 Shell 命令（含超时控制） | ✅ |
| `grep_search` | 🔍 | 按正则搜索文件内容 | ❌ |
| `file_search` | 📁 | 按 glob 搜索文件名 | ❌ |
| `list_directory` | 📂 | 列出目录内容（含大小/权限） | ❌ |
| `think` | 💭 | 内部推理，无副作用 | ❌ |
| `read_pdf` | 📄 | PDF 文本提取 | ❌ |
| `call_node` | 🤖 | 委派任务给其他 Agent 节点（按名/URL/标签路由） | ✅ |
| `list_nodes` | 📶 | 列出当前可用的 Agent 节点 | ❌ |
| `load_skill` | 📚 | 加载项目技能（.agent/skills/） | ❌ |
| `create_skill` | ✍️ | 创建或更新项目技能 | ✅ |
| `browser` | 🌐 | 浏览器自动化（Chrome DevTools Protocol） | ✅ |
| `connect_service` | 🔌 | 注册外部服务（WebSocket/REST） | ❌ |
| `query_service` | ❓ | 查询已注册的外部服务 | ❌ |
| `subscribe_service` | 📡 | 订阅服务推送通知 | ❌ |
| `unsubscribe_service` | 📡 | 取消订阅服务 | ❌ |
| `list_services` | 📋 | 列出所有已注册服务 | ❌ |

### 外部依赖（可选）

部分工具依赖系统命令，Agent 会自动按优先级尝试：

| 工具 | 后端 | 安装方式 |
|------|------|----------|
| `read_pdf` | marker_single → pdftotext → mutool | `pip install marker-pdf` / `apt install poppler-utils` / `apt install mupdf-tools` |

---

## � MCP（Model Context Protocol）

Agent 同时支持两个方向的 MCP 集成：

### 方向 A：作为 MCP 服务器（`--mode mcp`）

见上文「MCP 服务器模式」章节。

### 方向 B：作为 MCP 客户端（接入外部 MCP 服务器）

通过配置文件连接外部 MCP 服务器，其工具会自动注册到 Agent 工具列表，LLM 可与内置工具一起透明调用。

**配置文件**：`.agent/mcp.toml`（项目级）或 `~/.config/rust_agent/mcp.toml`（用户级），项目级优先并合并。

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

**工具命名规则**：`<server_name>__<tool_name>`，例如 `filesystem__read_file`、`github__search_repositories`。每个字段说明：

| 字段 | 必填 | 说明 |
|------|------|------|
| `name` | ✅ | 服务器别名，用作工具前缀 |
| `command` | ✅ | 可执行文件路径（如 `npx`、`/usr/bin/my-mcp-server`） |
| `args` | ❌ | 命令行参数列表 |
| `env` | ❌ | 注入到子进程的额外环境变量 |

> Agent 启动时自动拉起所有配置的 MCP 子进程，通过 JSON-RPC 2.0 over stdio 通信。子进程随 Agent 退出而终止。

---

## �📋 Plan 模式

Plan 模式让 Agent 先分析后执行，避免盲目修改代码：

```
🤖 > /plan 重构所有 GPIO 初始化代码，统一使用 HAL 库

📋  Planning: 重构所有 GPIO 初始化代码...
（Agent 使用只读工具分析代码：read_file, grep_search, list_directory, run_command...）
✅  Plan generated. Use /plan show to review, /plan run to execute.

🤖 > /plan show
（显示计划内容）

🤖 > /plan run
（Agent 开始执行计划，使用全部工具包括写入和编辑）
```

Plan 阶段允许使用只读工具和**只读 shell 命令**（`git status/log/diff`、`find` 等），确保不产生任何副作用。

---

## ⚡ Pipeline 执行干预

使用自动 Pipeline（Planner → Executor → Checker）时，有两个时机可以向执行器注入你的知识：

### 1. Approve 时附带背景（最推荐）

计划审核输入 `y` 后，系统会追加询问背景信息：

```
   Review: [y] approve  [n] reject  [type feedback to refine]
   > y
   Context: add background info for the executor (Enter to skip)
   > 新分支已将 module.rs 重构为 foo/mod.rs + foo/types.rs + foo/handler.rs，旧路径已删除
```

这段内容以最高优先级注入 Executor 的初始 prompt，LLM 在第一步就能感知到。

### 2. 执行中随时打断（Ctrl+\）

Executor 运行期间，在任意 LLM 迭代之间按 `Ctrl+\` 暂停并追加指导：

```
⚡ Guidance: type a note for the executor (or press Enter to continue)
   > 等一下，那个文件已经被删了，应该去看 src/driver/new_gpio.c
💡 Guidance injected into executor context.
```

> `Ctrl+C` 中断执行 | `Ctrl+\` 暂停注入后继续

---

## ✏️ 自定义系统提示词

可以通过 Markdown 文件自定义 LLM 的系统提示词：

| 文件位置 | 作用域 | 优先级 |
|----------|--------|--------|
| `~/.config/rust_agent/system_prompt.md` | 全局（所有项目） | 低 |
| `<项目>/.agent/system_prompt.md` | 当前项目 | 高 |

**追加模式**（默认）— 直接写内容，追加到默认提示词之后：
```markdown
你是一个嵌入式 C 开发专家，擅长 ARM Cortex-M。
请用中文回复。
代码注释使用英文。
```

**替换模式** — 第一行写 `# OVERRIDE`，完全替换默认提示词：
```markdown
# OVERRIDE
你是一个 Rust 系统编程专家。
当前项目路径已在环境中设置。
按照项目 .editorconfig 的代码风格编写。
```

**加载顺序**：
```
默认提示词 → 全局 system_prompt.md → 项目 system_prompt.md → summary → skills → memory
```

---

## �🛠️ 开发者：扩展工具

要添加新工具，需要以下步骤：

1. 在 `src/tools/` 下创建新文件
2. 实现 `Tool` trait：
```rust
pub struct MyNewTool;

#[async_trait::async_trait]
impl Tool for MyNewTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "my_tool".to_string(),
            description: "What this tool does".to_string(),
            parameters: serde_json::json!({ ... }),
        }
    }
    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        // 实现逻辑
        ToolResult::success("done")
    }
}
```
3. 在 `src/tools/mod.rs` 中：
   - 添加 `pub mod my_tool;`
   - 在 `ToolExecutor::new()` 中 `executor.register(Box::new(my_tool::MyNewTool));`
   - 如果是只读工具，加入 `readonly_definitions()` 的 `READONLY_TOOLS` 列表
4. 在 `src/agent.rs` 的 `record_tool_to_memory()` 中添加记忆记录分支
5. 在 `src/ui.rs` 的 `print_tool_use()` 中添加图标和输入显示
6. 如果需要确认，在 `needs_confirmation()` 和 `build_confirm_action()` 中添加

参考实现：`src/tools/think.rs`（最简单）、`src/tools/browser.rs`（中等）、`src/tools/edit_file.rs`（复杂）

---

## `.agent/` 目录结构

Agent 在项目下自动管理 `.agent/` 目录：

```
your-project/
├── AGENT.md                        # 全局项目指令（自动加载，原生格式）
├── SKILL.md                        # 等效入口（OpenClaw AgentSkills 兼容格式，可选）
└── .agent/
    ├── memory.md                   # 持久记忆（自动维护）
    ├── summary.md                  # 项目摘要（/summary 生成）
    ├── system_prompt.md            # 自定义系统提示词（可选）
    ├── roles/                      # 角色提示词覆盖（可选）
    │   ├── planner.md              # 覆盖或追加 Planner 的系统提示词
    │   ├── executor.md             # 覆盖或追加 Executor 的系统提示词
    │   └── checker.md              # 覆盖或追加 Checker 的系统提示词
    └── skills/                     # 项目级 Skills
        ├── modify-dts-gpio.md
        └── cross-compile.md
```

建议在 `.gitignore` 中添加：
```
.agent/memory.md
.agent/sessions/
```

---

## 📄 License
