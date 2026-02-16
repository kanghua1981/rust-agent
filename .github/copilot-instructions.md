# Rust Coding Agent Development Instructions

## Core Architecture
- **Agent Loop**: Located in [src/agent.rs](src/agent.rs). Manages the `loop` of `LLM -> Tool -> Loop`. It tracks tokens and handles `Memory` updates.
- **Tool System**: Tools implement the `Tool` trait in [src/tools/mod.rs](src/tools/mod.rs).
- **LLM Abstraction**: Generic `LlmClient` trait in [src/llm/mod.rs](src/llm/mod.rs) for Anthropic, OpenAI, and Compatible providers.
- **Memory & Skills**: 
  - **Memory**: [src/memory.rs](src/memory.rs) persists project facts and file history to `.agent/memory.md`. It includes an **automatic truncation summary** feature that compresses conversation history into the session log when context limits are reached.
  - **Skills**: [src/skills.rs](src/skills.rs) loads instructions from `AGENT.md` and `.agent/skills/*.md`.
- **Context Management**: [src/context.rs](src/context.rs) uses heuristic tokenization (CJK awareness) and middle-truncation to manage window limits.

## Key Patterns & Conventions

### 1. Adding New Tools
1. Create `src/tools/name.rs`.
2. Implement `Tool` trait (`definition` for JSON schema, `execute` for logic).
3. Use `serde_json` for parameters and `anyhow::Result` for errors.
4. **Important**: Always use absolute paths via `std::fs::canonicalize` or `fs_err` equivalents to ensure the agent works across directories.
5. Register in `ToolExecutor::new` in [src/tools/mod.rs](src/tools/mod.rs).

### 2. Error Handling & Async
- Use `anyhow::Result` for general errors.
- Tool errors should return `ToolResult::Error(msg)` to allow the LLM to recover, rather than returning `Err`.
- Use `#[async_trait]` and `tokio::fs` for all awaitable I/O.

### 3. UI & Output
- Use `ui::print_*` functions in [src/ui.rs](src/ui.rs) for all terminal output:
  - `print_tool_use`: Before executing a tool.
  - `print_tool_result`: After tool execution.
  - `print_thinking`: To show progress.
- Maintain consistent symbols: 📖 (read), ✏️ (edit), 🔨 (cmd), 📂 (dir).

### 4. Conversation Models
- History lives in `Conversation` [src/conversation.rs](src/conversation.rs).
- Messages consist of `ContentBlock`s: `Text`, `ToolUse`, or `ToolResult`.

## Developer Workflow
- **Build**: `cargo build --release`
- **Run**: `cargo run --release`
- **Debug**: `cargo run --release -- --verbose` (enables `debug!` tracing).
- **Environment**: API keys in `.env` (see `.env.example`).
- **Dependencies**: Uses `reqwest` with `rustls` (no system SSL dependency) and `termimad` for Markdown UI rendering.

## Critical Files
- [src/main.rs](src/main.rs): Entry/CLI parsing.
- [src/agent.rs](src/agent.rs): The core iteration loop logic.
- [src/tools/mod.rs](src/tools/mod.rs): Tool registry and executor.
- [src/context.rs](src/context.rs): Token estimation and truncation strategy.
- [src/ui.rs](src/ui.rs): Centralized terminal UI management.
