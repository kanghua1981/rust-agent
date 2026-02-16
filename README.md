# 🤖 Rust Coding Agent

一个用 Rust 编写的 AI 编码助手 CLI 工具，类似 Claude Code。它可以读写文件、执行命令、搜索代码，并通过 LLM 进行智能交互。

## ✨ 特性

- **🔧 工具系统**: 内置 7 种工具 - 文件读写、编辑、命令执行、代码搜索、文件搜索、目录列表
- **🔄 Agent 循环**: 自动编排 LLM 调用与工具执行，多轮迭代直到任务完成
- **🎨 终端 UI**: 彩色输出、Markdown 渲染、Diff 预览、友好的交互界面
- **🌐 多 Provider 支持**: Anthropic Claude、OpenAI GPT、以及任何兼容的 API
- **📜 对话持久化**: 支持上下文保持、会话保存与恢复
- **📚 Skills 系统**: 通过 Markdown 文件注入项目级别的专家知识
- **🔒 安全确认**: 文件写入和命令执行前需用户确认
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

Agent 需要一个 LLM API Key 才能工作。支持三种方式：

```bash
# 方式一：环境变量（推荐）
export ANTHROPIC_API_KEY=sk-ant-xxxxx

# 方式二：创建 .env 文件（持久化）
cp .env.example .env
vim .env  # 填入你的 API Key

# 方式三：使用 OpenAI 或兼容 API
export OPENAI_API_KEY=sk-xxxxx
# 或
export LLM_API_KEY=your-key
export LLM_BASE_URL=http://your-server:8080
```

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

# 开启详细日志（调试用）
./target/release/agent --verbose
```

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
| `/quit` | 退出（会自动保存会话） |
| `/memory` | 显示当前会话的内存使用情况 |

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

## 📚 Skills 系统

Skills 是项目级别的知识文件，让 Agent "理解"你的项目规范。

### 目录结构

```
your-project/
├── AGENT.md                    # 全局项目指令（自动加载）
└── .agent/
    └── skills/
        ├── modify-dts-gpio.md  # 设备树修改规范
        ├── cross-compile.md    # 交叉编译流程
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

---

## 🔌 支持的 LLM Provider

| Provider | 启动参数 | 环境变量 |
|----------|----------|----------|
| Anthropic Claude（默认） | `--provider anthropic` | `ANTHROPIC_API_KEY` |
| OpenAI | `--provider openai` | `OPENAI_API_KEY` |
| 兼容 API（Ollama 等） | `--provider compatible` | `LLM_API_KEY` + `LLM_BASE_URL` |

---

## 🛡️ 安全机制

Agent 在执行以下操作前会要求确认：

- **写入文件** (`write_file`)：显示文件路径和行数
- **编辑文件** (`edit_file`)：显示文件路径，执行后展示 Diff
- **执行命令** (`run_command`)：显示完整命令内容

```
⚠️  Write file: src/driver.c (45 lines)
   Proceed? [y]es / [n]o / [a]lways:
```

- 输入 `y`：确认本次操作
- 输入 `n`：拒绝（Agent 会知道你拒绝了，并调整策略）
- 输入 `a`：本次会话内跳过所有确认

也可以通过 `--yes` 启动参数或 `/yesall` 命令全局跳过。

---

## 🏗️ 架构

```
src/
├── main.rs          # 入口：CLI 参数解析 (clap)
├── config.rs        # 配置管理（API Key、Provider、模型参数）
├── cli.rs           # 交互式 REPL 循环 (rustyline)
├── agent.rs         # Agent 核心：LLM 调用 + Tool 编排 + 迭代循环
├── conversation.rs  # 对话历史：Message / ContentBlock 数据模型
├── context.rs       # 上下文窗口管理与自动截断
├── streaming.rs     # Anthropic SSE 流式输出解析
├── confirm.rs       # 危险操作的用户确认机制
├── persistence.rs   # 会话保存与恢复 (JSON)
├── diff.rs          # 文件修改的 Diff 展示
├── skills.rs        # Skills 加载系统
├── ui.rs            # 终端 UI 输出（颜色、Markdown 渲染）
├── llm/
│   ├── mod.rs       # LlmClient trait 定义
│   ├── anthropic.rs # Anthropic Claude API 实现
│   └── openai.rs    # OpenAI 兼容 API 实现
└── tools/
    ├── mod.rs       # Tool trait + ToolExecutor 注册中心
    ├── read_file.rs # 读取文件（支持行范围）
    ├── write_file.rs# 写入/创建文件
    ├── edit_file.rs # 精确替换文件内容
    ├── run_command.rs# 执行 Shell 命令（含超时控制）
    ├── search.rs    # Grep 搜索 + 文件名搜索
    └── list_dir.rs  # 列出目录内容（含文件大小、权限）
```

---

## 🛠️ 开发者：扩展工具

要添加新工具，只需三步：

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
    async fn execute(&self, input: &serde_json::Value) -> ToolResult {
        // 实现逻辑
        ToolResult::success("done")
    }
}
```
3. 在 `src/tools/mod.rs` 的 `ToolExecutor::new()` 中注册

参考实现：`src/tools/read_file.rs`（简单）、`src/tools/edit_file.rs`（复杂）

---

## 📄 License

MIT
