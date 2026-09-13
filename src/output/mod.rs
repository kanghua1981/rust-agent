//! Output abstraction layer.
//!
//! Decouples the agent's I/O from terminal-specific code. Each mode
//! (CLI, stdio, MCP, WebSocket server) provides its own `AgentOutput`
//! implementation.

use crate::confirm::ConfirmAction;
use crate::tools::ToolResult;

/// Severity level for notifications pushed from outside the tool loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    Info,
    Warning,
}

impl NotifyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotifyLevel::Info    => "info",
            NotifyLevel::Warning => "warning",
        }
    }
}

/// Result of interactive plan review.
#[derive(Debug, Clone)]
pub enum PlanReview {
    /// User approves the plan — proceed to execution.
    Approve,
    /// User approves the plan and provides background context for the executor.
    ApproveWithContext(String),
    /// User rejects the plan.
    Reject,
    /// User provides feedback — regenerate the plan with this guidance.
    Refine(String),
}

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

    /// Called when the model begins a thinking / reasoning block.
    fn on_thinking_start(&self) {}

    /// A single streaming token from a thinking / reasoning block.
    fn on_thinking_token(&self, _token: &str) {}

    /// Called when the thinking / reasoning block ends.
    fn on_thinking_end(&self) {}

    /// Show which role/model is about to respond.
    /// `label`  — e.g. "🤖 Agent", "🧠 Planner", "⚙️  Executor", "🔍 Checker".
    /// `model`  — display name of the model being called.
    fn on_role_header(&self, label: &str, model: &str);

    /// Signal that a stage has finished.
    /// `label`  — e.g. "Executor", "Checker".
    fn on_stage_end(&self, label: &str);

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
    /// About to execute a tool. `tool_id` will be placed in the envelope `id` field
    /// (not in `data`) so clients can correlate tool_use ↔ tool_result without drilling.
    fn on_tool_use(&self, name: &str, input: &serde_json::Value, tool_id: &str);

    /// Tool execution finished.
    fn on_tool_result(&self, name: &str, result: &ToolResult);

    /// A file was created or modified by a tool (write_file / edit_file / multi_edit_file).
    /// This notifies output backends (e.g. WeChat Bridge) that a file is available.
    fn on_file_created(&self, _path: &str) {}

    // ── Diff preview ────────────────────────────────────────────
    /// Show a diff for a file modification.
    fn on_diff(&self, path: &str, old: &str, new: &str);

    // ── Confirmation ────────────────────────────────────────────
    /// Ask the user to approve a dangerous action.
    /// Returns the user's decision (Yes, No, AlwaysYes, or Clarify).
    fn confirm(&self, action: &ConfirmAction) -> crate::confirm::ConfirmResult;

    // ── Interactive input ───────────────────────────────────────
    /// Ask the user a question (called by the ask_user tool).
    /// Returns the user's free-text answer.
    fn ask_user(&self, question: &str) -> String;

    /// Present a plan for interactive review.
    /// Returns Approve, Reject, or Refine(feedback).
    fn review_plan(&self, plan_text: &str) -> PlanReview;

    /// Prompt the user for mid-execution guidance (triggered by Ctrl-\).
    /// Called when the user presses Ctrl-\ to inject context the LLM is missing.
    /// Returns `Some(text)` if the user typed something, `None` to continue silently.
    fn inject_guidance(&self) -> Option<String>;

    // ── Diagnostics ─────────────────────────────────────────────
    /// Non-fatal warning (e.g. "max iterations reached").
    fn on_warning(&self, msg: &str);

    /// Fatal / display error.
    fn on_error(&self, msg: &str);

    /// Context window pressure notification.
    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize);

    // ── SubAgent events ─────────────────────────────────────────
    /// An event forwarded from a stdio sub-agent.
    /// `task_id` is a short identifier (e.g. first 4 chars of UUID) used as prefix.
    /// Default implementation falls back to existing output methods with a prefix so
    // ── External notifications ──────────────────────────────────
    /// A notification pushed from outside the tool loop (e.g. an MCP server message).
    /// Rendered separately from the main conversation stream (status bar / side panel).
    /// Default implementation prints a prefixed warning line so old implementations work.
    fn on_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        let icon = match level {
            NotifyLevel::Info    => "ℹ",
            NotifyLevel::Warning => "⚠",
        };
        self.on_warning(&format!("[{}] {} {}", source, icon, message));
    }
}

mod cli;
mod silent;
mod stdio;
mod ws;

pub use cli::CliOutput;
pub use silent::SilentOutput;
pub use stdio::StdioOutput;
pub use ws::{WsCommand, WsOutput};
