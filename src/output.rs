//! Output abstraction layer.
//!
//! Decouples the agent's I/O from terminal-specific code. Each mode
//! (CLI, stdio, MCP, WebSocket server) provides its own `AgentOutput`
//! implementation.

use crate::confirm::ConfirmAction;
use crate::tools::ToolResult;

/// Abstraction over all user-facing output and confirmation prompts.
///
/// The agent calls these methods instead of writing to stdout directly,
/// allowing the same logic to drive a terminal UI, a JSON-over-stdio
/// protocol, or an MCP server.
#[allow(dead_code)]
#[async_trait::async_trait]
pub trait AgentOutput: Send + Sync {
    // ── Progress ────────────────────────────────────────────────
    /// The LLM is processing (spinner / "thinking …").
    fn on_thinking(&self);

    // ── Text output ─────────────────────────────────────────────
    /// A full text block from a non-streaming provider.
    fn on_assistant_text(&self, text: &str);

    /// A single streaming token (Anthropic SSE text delta).
    fn on_streaming_text(&self, token: &str);

    /// Mark the beginning of a streamed text response.
    fn on_stream_start(&self);

    /// Mark the end of a streamed text response.
    fn on_stream_end(&self);

    // ── Tools ───────────────────────────────────────────────────
    /// About to execute a tool.
    fn on_tool_use(&self, name: &str, input: &serde_json::Value);

    /// Tool execution finished.
    fn on_tool_result(&self, name: &str, result: &ToolResult);

    // ── Diff preview ────────────────────────────────────────────
    /// Show a diff for a file modification.
    fn on_diff(&self, path: &str, old: &str, new: &str);

    // ── Confirmation ────────────────────────────────────────────
    /// Ask the user to approve a dangerous action.
    /// Returns `true` if the action should proceed.
    fn confirm(&self, action: &ConfirmAction) -> bool;

    // ── Diagnostics ─────────────────────────────────────────────
    /// Non-fatal warning (e.g. "max iterations reached").
    fn on_warning(&self, msg: &str);

    /// Fatal / display error.
    fn on_error(&self, msg: &str);

    /// Context window pressure notification.
    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize);
}

// ═══════════════════════════════════════════════════════════════════
//  CLI output — wraps existing ui::*, confirm::*, diff::* functions
// ═══════════════════════════════════════════════════════════════════

/// Terminal (CLI) output: colored text, interactive confirmation, diffs.
pub struct CliOutput;

impl CliOutput {
    pub fn new() -> Self {
        CliOutput
    }
}

#[async_trait::async_trait]
impl AgentOutput for CliOutput {
    fn on_thinking(&self) {
        crate::ui::print_thinking();
    }

    fn on_assistant_text(&self, text: &str) {
        crate::ui::print_assistant_text(text);
    }

    fn on_streaming_text(&self, token: &str) {
        use std::io::Write;
        print!("{}", token);
        std::io::stdout().flush().ok();
    }

    fn on_stream_start(&self) {
        println!("\n{}", "─".repeat(60));
    }

    fn on_stream_end(&self) {
        println!("\n{}", "─".repeat(60));
    }

    fn on_tool_use(&self, name: &str, input: &serde_json::Value) {
        crate::ui::print_tool_use(name, input);
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        crate::ui::print_tool_result(name, result);
    }

    fn on_diff(&self, path: &str, old: &str, new: &str) {
        crate::diff::print_diff(path, old, new);
    }

    fn confirm(&self, action: &ConfirmAction) -> bool {
        crate::confirm::should_proceed(action)
    }

    fn on_warning(&self, msg: &str) {
        crate::ui::print_warning(msg);
    }

    fn on_error(&self, msg: &str) {
        crate::ui::print_error(msg);
    }

    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize) {
        crate::ui::print_context_warning(usage_percent, estimated, max);
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Stdio output — JSON messages over stdout / stdin
// ═══════════════════════════════════════════════════════════════════

/// JSON-over-stdio output for non-terminal consumers (VS Code, scripts).
///
/// Every event is a single JSON line written to stdout.
/// Confirmations read a JSON response from stdin.
pub struct StdioOutput;

impl StdioOutput {
    pub fn new() -> Self {
        StdioOutput
    }

    /// Write a JSON event line to stdout.
    fn emit(&self, event_type: &str, data: serde_json::Value) {
        let msg = serde_json::json!({
            "type": event_type,
            "data": data,
        });
        let line = serde_json::to_string(&msg).unwrap_or_default();
        println!("{}", line);
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
}

#[async_trait::async_trait]
impl AgentOutput for StdioOutput {
    fn on_thinking(&self) {
        self.emit("thinking", serde_json::json!({}));
    }

    fn on_assistant_text(&self, text: &str) {
        self.emit("assistant_text", serde_json::json!({ "text": text }));
    }

    fn on_streaming_text(&self, token: &str) {
        self.emit("streaming_token", serde_json::json!({ "token": token }));
    }

    fn on_stream_start(&self) {
        self.emit("stream_start", serde_json::json!({}));
    }

    fn on_stream_end(&self) {
        self.emit("stream_end", serde_json::json!({}));
    }

    fn on_tool_use(&self, name: &str, input: &serde_json::Value) {
        self.emit("tool_use", serde_json::json!({
            "tool": name,
            "input": input,
        }));
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        self.emit("tool_result", serde_json::json!({
            "tool": name,
            "output": result.output,
            "is_error": result.is_error,
        }));
    }

    fn on_diff(&self, path: &str, old: &str, new: &str) {
        // Emit a plain-text unified diff
        let diff_text = crate::diff::diff_string(path, old, new);
        self.emit("diff", serde_json::json!({
            "path": path,
            "diff": diff_text,
        }));
    }

    fn confirm(&self, action: &ConfirmAction) -> bool {
        // In stdio mode, check auto-approve first
        if crate::confirm::is_auto_approve() {
            return true;
        }

        // Send a confirmation request and read the response
        let action_data = match action {
            ConfirmAction::WriteFile { path, lines } => serde_json::json!({
                "action": "write_file",
                "path": path,
                "lines": lines,
            }),
            ConfirmAction::EditFile { path } => serde_json::json!({
                "action": "edit_file",
                "path": path,
            }),
            ConfirmAction::RunCommand { command } => serde_json::json!({
                "action": "run_command",
                "command": command,
            }),
            ConfirmAction::DeleteFile { path } => serde_json::json!({
                "action": "delete_file",
                "path": path,
            }),
        };

        self.emit("confirm_request", action_data);

        // Read response from stdin
        let mut response = String::new();
        if std::io::stdin().read_line(&mut response).is_err() {
            return false;
        }

        // Parse JSON response: { "approved": true/false }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
            parsed
                .get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        } else {
            // Fall back to plain text
            matches!(response.trim().to_lowercase().as_str(), "y" | "yes" | "true")
        }
    }

    fn on_warning(&self, msg: &str) {
        self.emit("warning", serde_json::json!({ "message": msg }));
    }

    fn on_error(&self, msg: &str) {
        self.emit("error", serde_json::json!({ "message": msg }));
    }

    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize) {
        self.emit("context_warning", serde_json::json!({
            "usage_percent": usage_percent,
            "estimated_tokens": estimated,
            "max_tokens": max,
        }));
    }
}

// ═══════════════════════════════════════════════════════════════════
//  WebSocket output — JSON frames over a WebSocket connection
// ═══════════════════════════════════════════════════════════════════

use std::sync::Mutex;
use tokio::sync::mpsc;

/// Message type sent through the channel to the WebSocket writer task.
#[derive(Debug)]
pub enum WsCommand {
    /// Send a JSON event frame.
    Send(String),
}

/// WebSocket output for remote consumers (VS Code extension, Web UI, scripts).
///
/// Events are serialized to JSON and sent over a WebSocket connection.
/// Confirmations send a `confirm_request` frame and block until the
/// client replies with `{ "type": "confirm_response", "data": { "approved": true } }`.
pub struct WsOutput {
    /// Sends commands to the WebSocket writer task.
    tx: mpsc::UnboundedSender<WsCommand>,
    /// Receives confirm responses from the reader task.
    confirm_rx: Mutex<std::sync::mpsc::Receiver<bool>>,
    /// Sends confirm responses (held by the reader task).
    pub confirm_tx: std::sync::mpsc::Sender<bool>,
}

impl WsOutput {
    /// Create a new WsOutput.
    ///
    /// - `tx`: channel to the writer task that sends frames on the socket.
    /// - Returns `(WsOutput, confirm_tx)` — caller gives `confirm_tx` to the
    ///   reader task so it can forward confirm responses.
    pub fn new(tx: mpsc::UnboundedSender<WsCommand>) -> Self {
        let (confirm_tx, confirm_rx) = std::sync::mpsc::channel();
        WsOutput {
            tx,
            confirm_rx: Mutex::new(confirm_rx),
            confirm_tx,
        }
    }

    /// Serialize and send a JSON event to the WebSocket.
    fn emit(&self, event_type: &str, data: serde_json::Value) {
        let msg = serde_json::json!({
            "type": event_type,
            "data": data,
        });
        let text = serde_json::to_string(&msg).unwrap_or_default();
        // Ignore send errors (connection may have closed)
        let _ = self.tx.send(WsCommand::Send(text));
    }

    /// Public variant of emit for use from the server module.
    pub fn emit_public(&self, event_type: &str, data: serde_json::Value) {
        self.emit(event_type, data);
    }
}

#[async_trait::async_trait]
impl AgentOutput for WsOutput {
    fn on_thinking(&self) {
        self.emit("thinking", serde_json::json!({}));
    }

    fn on_assistant_text(&self, text: &str) {
        self.emit("assistant_text", serde_json::json!({ "text": text }));
    }

    fn on_streaming_text(&self, token: &str) {
        self.emit("streaming_token", serde_json::json!({ "token": token }));
    }

    fn on_stream_start(&self) {
        self.emit("stream_start", serde_json::json!({}));
    }

    fn on_stream_end(&self) {
        self.emit("stream_end", serde_json::json!({}));
    }

    fn on_tool_use(&self, name: &str, input: &serde_json::Value) {
        self.emit("tool_use", serde_json::json!({
            "tool": name,
            "input": input,
        }));
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        self.emit("tool_result", serde_json::json!({
            "tool": name,
            "output": result.output,
            "is_error": result.is_error,
        }));
    }

    fn on_diff(&self, path: &str, old: &str, new: &str) {
        let diff_text = crate::diff::diff_string(path, old, new);
        self.emit("diff", serde_json::json!({
            "path": path,
            "diff": diff_text,
        }));
    }

    fn confirm(&self, action: &ConfirmAction) -> bool {
        if crate::confirm::is_auto_approve() {
            return true;
        }

        let action_data = match action {
            ConfirmAction::WriteFile { path, lines } => serde_json::json!({
                "action": "write_file",
                "path": path,
                "lines": lines,
            }),
            ConfirmAction::EditFile { path } => serde_json::json!({
                "action": "edit_file",
                "path": path,
            }),
            ConfirmAction::RunCommand { command } => serde_json::json!({
                "action": "run_command",
                "command": command,
            }),
            ConfirmAction::DeleteFile { path } => serde_json::json!({
                "action": "delete_file",
                "path": path,
            }),
        };

        // Send confirm_request over WebSocket
        self.emit("confirm_request", action_data);

        // Block and wait for the reader task to forward the response
        let rx = self.confirm_rx.lock().unwrap();
        rx.recv().unwrap_or(false)
    }

    fn on_warning(&self, msg: &str) {
        self.emit("warning", serde_json::json!({ "message": msg }));
    }

    fn on_error(&self, msg: &str) {
        self.emit("error", serde_json::json!({ "message": msg }));
    }

    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize) {
        self.emit("context_warning", serde_json::json!({
            "usage_percent": usage_percent,
            "estimated_tokens": estimated,
            "max_tokens": max,
        }));
    }
}
