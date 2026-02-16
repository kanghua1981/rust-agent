# Rust Coding Agent Development Instructions

## Core Architecture
- **Agent Loop**: Located in [src/agent.rs](src/agent.rs). Manages the `loop` of `LLM -> Tool -> Loop`. It tracks tokens and handles `Memory` updates. The `Agent` struct accepts `Arc<dyn AgentOutput>` to decouple I/O from business logic.
- **Output Abstraction**: [src/output.rs](src/output.rs) defines the `AgentOutput` trait. All user-facing output (text, tool events, diffs, confirmations) goes through this trait. Three implementations:
  - `CliOutput` — colored terminal output (wraps `ui::*`, `confirm::*`, `diff::*`).
  - `StdioOutput` — JSON-over-stdio for non-terminal consumers (VS Code, scripts).
  - `WsOutput` — JSON frames over WebSocket for remote consumers (VS Code extension, Web UI).
- **Tool System**: Tools implement the `Tool` trait in [src/tools/mod.rs](src/tools/mod.rs).
- **LLM Abstraction**: Generic `LlmClient` trait in [src/llm/mod.rs](src/llm/mod.rs) for Anthropic, OpenAI, and Compatible providers.
- **Streaming**: [src/streaming.rs](src/streaming.rs) handles Anthropic SSE. It accepts `&dyn AgentOutput` and calls `on_streaming_text()` / `on_stream_start()` / `on_stream_end()` instead of direct `print!`.
- **Memory & Skills**: 
  - **Memory**: [src/memory.rs](src/memory.rs) persists project facts and file history to `.agent/memory.md`. It includes an **automatic truncation summary** feature that compresses conversation history into the session log when context limits are reached.
  - **Summary**: [src/summary.rs](src/summary.rs) manages `.agent/summary.md` — a standalone project overview generated via `/summary` command.
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

### 3. Output & UI
- **Agent internals** must use `self.output.*` methods from the `AgentOutput` trait — never direct `println!`, `print!`, or `ui::*` calls from `agent.rs` or `streaming.rs`.
- **CLI-only code** (banners, help, slash commands in `cli.rs`) can still use `ui::print_*` directly since it only runs in CLI mode.
- Key `AgentOutput` methods: `on_thinking()`, `on_assistant_text()`, `on_streaming_text()`, `on_tool_use()`, `on_tool_result()`, `on_diff()`, `confirm()`, `on_warning()`, `on_context_warning()`.
- Maintain consistent symbols: 📖 (read), ✏️ (edit), 🔨 (cmd), 📂 (dir).

### 4. Run Modes
- `--mode cli` (default): Interactive terminal with colored output, rustyline REPL.
- `--mode stdio`: JSON-over-stdio protocol. Each event is a single JSON line. Confirmations send `confirm_request` and read `{ "approved": true/false }` from stdin.
- `--mode server`: WebSocket server on `--host` / `--port`. Each connection spawns its own `Agent` with a `WsOutput`. Client sends `user_message` and `confirm_response` JSON frames.
- The mode is selected in `main.rs` and injected as `Arc<dyn AgentOutput>` into `cli::run()` → `Agent::new()` (or `server::run()` for server mode).

### 5. Conversation Models
- History lives in `Conversation` [src/conversation.rs](src/conversation.rs).
- Messages consist of `ContentBlock`s: `Text`, `ToolUse`, or `ToolResult`.

## Developer Workflow
- **Build**: `cargo build --release`
- **Run**: `cargo run --release`
- **Stdio mode**: `cargo run --release -- --mode stdio --yes -p "your prompt"`
- **Server mode**: `cargo run --release -- --mode server --port 9527`
- **Debug**: `cargo run --release -- --verbose` (enables `debug!` tracing).
- **Environment**: API keys in `.env` (see `.env.example`).
- **Dependencies**: Uses `reqwest` with `rustls` (no system SSL dependency) and `termimad` for Markdown UI rendering.

## Critical Files
- [src/main.rs](src/main.rs): Entry/CLI parsing, `--mode` flag, output backend selection.
- [src/output.rs](src/output.rs): `AgentOutput` trait + `CliOutput` + `StdioOutput` + `WsOutput`.
- [src/agent.rs](src/agent.rs): The core iteration loop logic (I/O-decoupled).
- [src/streaming.rs](src/streaming.rs): Anthropic SSE streaming (I/O-decoupled via `&dyn AgentOutput`).
- [src/server.rs](src/server.rs): WebSocket server — per-connection Agent lifecycle.
- [src/tools/mod.rs](src/tools/mod.rs): Tool registry and executor.
- [src/context.rs](src/context.rs): Token estimation and truncation strategy.
- [src/cli.rs](src/cli.rs): REPL loop, slash commands, session management.
- [src/ui.rs](src/ui.rs): CLI-specific terminal rendering (used by `CliOutput` and `cli.rs`).
