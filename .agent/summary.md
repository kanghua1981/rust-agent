**项目名称与目的**  
Rust Coding Agent 是一个用 Rust 编写的 AI 编码助手 CLI 工具，类似 Claude Code。它通过 LLM 驱动，能够读写文件、执行命令、搜索代码，并提供交互式智能编程辅助。

**技术栈**  
- 语言：Rust  
- 主要依赖：tokio（异步运行时）、reqwest（HTTP 客户端）、clap（CLI 解析）、crossterm（终端 UI）、serde（序列化）、tracing（日志）、tokio-tungstenite（WebSocket）  
- LLM 支持：Anthropic Claude、OpenAI GPT 及兼容 API（如 Ollama、通义千问）

**目录结构概览**  
```
src/
├── main.rs          # 入口点，CLI 参数解析与模式选择
├── agent.rs         # Agent 核心循环，LLM 调用与工具编排
├── cli.rs           # 交互式 REPL 与斜杠命令处理
├── output.rs        # 输出抽象层（CLI/Stdio/WebSocket）
├── llm/             # LLM 客户端实现（anthropic.rs、openai.rs）
├── tools/           # 工具集（文件读写、命令执行、搜索等 13 种工具）
├── pipeline.rs      # 多角色流水线（Planner/Executor/Checker）
├── conversation.rs  # 对话历史管理
├── context.rs       # 上下文窗口管理与智能截断
├── config.rs        # 配置管理（API Key、模型参数）
├── model_manager.rs # 模型配置（models.toml）与角色管理
├── ui.rs            # 终端 UI 渲染（颜色、Markdown、Diff）
├── memory.rs        # 持久记忆系统（.agent/memory.md）
├── summary.rs       # 项目摘要生成与加载
├── skills.rs        # Skills 系统（项目级知识注入）
├── sandbox.rs       # 沙盒模式（OverlayFS/快照）
└── server.rs        # WebSocket 服务器
docs/                # 设计文档与用户指南
vscode-extension/    # VS Code 扩展（TypeScript 实现）
```

**构建与运行命令**  
- 构建：`cargo build --release`（产物位于 `target/release/agent`）  
- 运行交互模式：`./target/release/agent`  
- 带初始提示：`./target/release/agent --prompt "任务描述"`  
- 指定工作目录：`./target/release/agent --workdir /path/to/project`  
- Stdio 模式（JSON 协议）：`./target/release/agent --mode stdio`  
- WebSocket 服务器：`./target/release/agent --mode server`  
- 沙盒模式：`./target/release/agent --sandbox`

**工具集**  
- **文件操作**：read_file、write_file、edit_file、multi_edit_file  
- **命令执行**：run_command、git（新增）  
- **搜索与浏览**：grep_search、file_search、list_directory、batch_read_files  
- **文档处理**：read_pdf、read_ebook  
- **思考与规划**：think  
- **技能管理**：load_skill、create_skill  
- **多代理协作**：call_sub_agent  

**Git 工具特性**  
- 支持常见 Git 操作：status、branch、checkout、commit、log、diff、add、push、pull 等  
- 可在修改代码前自动创建分支，实现隔离开发  
- 集成到规划阶段，允许 LLM 查看版本状态  
- 支持自定义工作目录和参数传递