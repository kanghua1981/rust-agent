//! Output abstraction layer.
//!
//! Decouples the agent's I/O from terminal-specific code. Each mode
//! (CLI, stdio, MCP, WebSocket server) provides its own `AgentOutput`
//! implementation.

use crate::confirm::ConfirmAction;
use crate::tools::ToolResult;

// ── SubAgent / Service event types ──────────────────────────────────────────

/// Events forwarded from a stdio sub-agent to the parent's output layer.
/// The parent prefixes each event with `[sub:{task_id}]` so multiple concurrent
/// sub-agents remain visually distinguishable in the output stream.
#[derive(Debug, Clone)]
pub enum SubAgentOutputEvent {
    StreamStart,
    StreamEnd,
    Token(String),
    ToolUse { name: String },
    ToolDone { name: String, is_error: bool },
    Done(String),
    Error(String),
}

/// Severity level for notifications pushed by an external Service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    Info,
    Warning,
    /// Requires user attention — rendered more prominently.
    Alert,
}

impl NotifyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotifyLevel::Info    => "info",
            NotifyLevel::Warning => "warning",
            NotifyLevel::Alert   => "alert",
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
    /// User rejects the plan — abort pipeline.
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

    /// Show which role/model is about to respond.
    /// `label`  — e.g. "🤖 Agent", "🧠 Planner", "⚙️  Executor", "🔍 Checker".
    /// `model`  — display name of the model being called.
    fn on_role_header(&self, label: &str, model: &str);

    /// Signal that a pipeline stage has finished.
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
    /// About to execute a tool.
    fn on_tool_use(&self, name: &str, input: &serde_json::Value);

    /// Tool execution finished.
    fn on_tool_result(&self, name: &str, result: &ToolResult);

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

    /// Present a pipeline plan for interactive review.
    /// Returns Approve, Reject, or Refine(feedback).
    fn review_plan(&self, plan_text: &str) -> PlanReview;

    /// Prompt the user for mid-execution guidance (triggered by Ctrl-\).
    /// Called only during Executor/Checker pipeline stages when the user
    /// presses Ctrl-\ to inject context the LLM is missing.
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
    /// implementations that don't override this still produce readable output.
    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        let prefix = format!("[sub:{}]", task_id);
        match event {
            SubAgentOutputEvent::StreamStart => {}
            SubAgentOutputEvent::StreamEnd   => {}
            SubAgentOutputEvent::Token(t) => {
                self.on_streaming_text(&format!("{} {}", prefix, t));
            }
            SubAgentOutputEvent::ToolUse { name } => {
                self.on_warning(&format!("{} ⚙  {}", prefix, name));
            }
            SubAgentOutputEvent::ToolDone { name, is_error } => {
                if *is_error {
                    self.on_warning(&format!("{} ✗  {}", prefix, name));
                } else {
                    self.on_warning(&format!("{} ✓  {}", prefix, name));
                }
            }
            SubAgentOutputEvent::Done(text) => {
                if !text.is_empty() {
                    self.on_warning(&format!("{} ✅ 完成: {}", prefix, crate::ui::truncate_str(text, 120)));
                } else {
                    self.on_warning(&format!("{} ✅ 完成", prefix));
                }
            }
            SubAgentOutputEvent::Error(msg) => {
                self.on_warning(&format!("{} ❌ {}", prefix, msg));
            }
        }
    }

    // ── Service notifications ───────────────────────────────────
    /// A notification pushed by an external Service (e.g. CI alert, model response).
    /// Rendered separately from the main conversation stream (status bar / side panel).
    /// Default implementation prints a prefixed warning line so old implementations work.
    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        let icon = match level {
            NotifyLevel::Info    => "ℹ",
            NotifyLevel::Warning => "⚠",
            NotifyLevel::Alert   => "🔔",
        };
        self.on_warning(&format!("[svc:{}] {} {}", source, icon, message));
    }
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

    fn on_role_header(&self, label: &str, model: &str) {
        crate::ui::print_role_header(label, model);
    }

    fn on_stage_end(&self, label: &str) {
        crate::ui::print_stage_end(label);
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

    fn confirm(&self, action: &ConfirmAction) -> crate::confirm::ConfirmResult {
        crate::confirm::confirm(action)
    }

    fn ask_user(&self, question: &str) -> String {
        use std::io::{self, Write};
        use colored::Colorize;
        println!("\n{}  {}", "❓", question.bright_cyan());
        print!("   {} ", "Your answer:".bright_white().bold());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        input.trim().to_string()
    }

    fn review_plan(&self, plan_text: &str) -> PlanReview {
        use std::io::{self, Write};
        use colored::Colorize;
        println!("\n{}  {}", "📋", "Pipeline Plan:".yellow().bold());
        println!("{}", "─".repeat(60));
        // Print the plan with termimad (or plain text)
        println!("{}", plan_text);
        println!("{}", "─".repeat(60));
        println!(
            "   {} {}",
            "Review:".bright_cyan().bold(),
            "[y] approve  [n] reject  [type feedback to refine]".dimmed()
        );
        // Flush any keystrokes that were buffered while the plan was streaming
        // (e.g. an accidental Enter press), so we read fresh user intent only.
        #[cfg(unix)]
        unsafe {
            libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
        }
        print!("   {} ", ">".bright_white());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let trimmed = input.trim();
        match trimmed.to_lowercase().as_str() {
            "y" | "yes" => {
                // Offer a one-time chance to add background context before execution.
                println!(
                    "   {} {}",
                    "Context:".bright_cyan(),
                    "add background info for the executor (Enter to skip)".dimmed()
                );
                #[cfg(unix)]
                unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH); }
                print!("   {} ", ">".bright_white());
                io::stdout().flush().ok();
                let mut ctx = String::new();
                io::stdin().read_line(&mut ctx).ok();
                let ctx = ctx.trim().to_string();
                if ctx.is_empty() {
                    PlanReview::Approve
                } else {
                    PlanReview::ApproveWithContext(ctx)
                }
            }
            "n" | "no" => PlanReview::Reject,
            _ if trimmed.is_empty() => PlanReview::Reject,
            _ => PlanReview::Refine(trimmed.to_string()),
        }
    }

    fn inject_guidance(&self) -> Option<String> {
        use std::io::{self, Write};
        use colored::Colorize;
        // Start on a fresh line after any streaming output
        println!();
        println!(
            "{}  {} {}",
            "⚡",
            "Guidance:".yellow().bold(),
            "type a note for the executor (or press Enter to continue)".dimmed()
        );
        #[cfg(unix)]
        unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH); }
        print!("   {} ", ">".bright_white());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let text = input.trim().to_string();
        if text.is_empty() { None } else { Some(text) }
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

    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        use colored::Colorize;
        use std::io::Write;
        let prefix = format!("[sub:{}]", task_id).cyan().bold().to_string();
        match event {
            SubAgentOutputEvent::StreamStart => {}
            SubAgentOutputEvent::StreamEnd   => { println!(); }
            SubAgentOutputEvent::Token(t) => {
                print!("{}", t);
                std::io::stdout().flush().ok();
            }
            SubAgentOutputEvent::ToolUse { name } => {
                println!("  {} ⚙  {}", prefix, name.bright_white());
            }
            SubAgentOutputEvent::ToolDone { name, is_error } => {
                if *is_error {
                    println!("  {} ✗  {}", prefix, name.red());
                } else {
                    println!("  {} ✓  {}", prefix, name.green());
                }
            }
            SubAgentOutputEvent::Done(text) => {
                let preview = crate::ui::truncate_str(text, 100);
                println!("  {} ✅ {}", prefix, preview.dimmed());
            }
            SubAgentOutputEvent::Error(msg) => {
                println!("  {} ❌ {}", prefix, msg.red());
            }
        }
    }

    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        use colored::Colorize;
        let (icon, msg_colored) = match level {
            NotifyLevel::Info    => ("ℹ", message.white().to_string()),
            NotifyLevel::Warning => ("⚠", message.yellow().to_string()),
            NotifyLevel::Alert   => ("🔔", message.red().bold().to_string()),
        };
        println!("  {} {} {}", format!("[svc:{}]", source).magenta().bold(), icon, msg_colored);
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Stdio output — JSON messages over stdout / stdin
// ═══════════════════════════════════════════════════════════════════

/// JSON-over-stdio output for non-terminal consumers (VS Code, scripts).
///
/// Every event is a single JSON line written to stdout.
/// Confirmations read a JSON response from stdin.
pub struct StdioOutput {
    /// Buffer for streaming tokens to reduce fragmentation
    buffer: std::sync::Mutex<String>,
    /// Whether buffering is enabled (default: true)
    buffering_enabled: bool,
}

impl StdioOutput {
    pub fn new() -> Self {
        // Check if buffering should be disabled via environment variable
        let buffering_enabled = !std::env::var("AGENT_NO_STDIO_BUFFER")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false);
        
        StdioOutput {
            buffer: std::sync::Mutex::new(String::new()),
            buffering_enabled,
        }
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

    /// Flush the buffer if it contains content
    fn flush_buffer(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        if !buffer.is_empty() {
            self.emit("streaming_token", serde_json::json!({ "token": &*buffer }));
            buffer.clear();
        }
    }

    /// Check if we should flush the buffer based on content
    fn should_flush(&self, token: &str, buffer: &str) -> bool {
        if !self.buffering_enabled {
            return true;
        }

        // Always flush on newline (but we'll handle newlines specially in on_streaming_text)
        if token.contains('\n') {
            return true;
        }

        // Flush on sentence boundaries
        if token.ends_with('.') || token.ends_with('!') || token.ends_with('?') {
            return true;
        }

        // Flush if buffer is getting too large (100 chars)
        if buffer.len() + token.len() > 100 {
            return true;
        }

        false
    }
}

#[async_trait::async_trait]
impl AgentOutput for StdioOutput {
    fn on_thinking(&self) {
        self.emit("thinking", serde_json::json!({}));
    }

    fn on_role_header(&self, label: &str, model: &str) {
        self.emit("role_header", serde_json::json!({ "label": label, "model": model }));
    }

    fn on_stage_end(&self, label: &str) {
        self.emit("stage_end", serde_json::json!({ "label": label }));
    }

    fn on_assistant_text(&self, text: &str) {
        self.emit("assistant_text", serde_json::json!({ "text": text }));
    }

    fn on_streaming_text(&self, token: &str) {
        // Ignore empty tokens
        if token.is_empty() {
            return;
        }
        
        if !self.buffering_enabled {
            self.emit("streaming_token", serde_json::json!({ "token": token }));
            return;
        }

        let mut buffer = self.buffer.lock().unwrap();
        
        // If token contains newline, add it to buffer and flush
        if token.contains('\n') {
            buffer.push_str(token);
            if !buffer.is_empty() {
                self.emit("streaming_token", serde_json::json!({ "token": &*buffer }));
                buffer.clear();
            }
            return;
        }
        
        // Check if we should flush before adding this token
        if self.should_flush(token, &buffer) {
            if !buffer.is_empty() {
                self.emit("streaming_token", serde_json::json!({ "token": &*buffer }));
                buffer.clear();
            }
            // Always emit the current token (it triggered the flush)
            self.emit("streaming_token", serde_json::json!({ "token": token }));
        } else {
            buffer.push_str(token);
        }
    }

    fn on_stream_start(&self) {
        // Clear buffer when starting a new stream
        self.buffer.lock().unwrap().clear();
        self.emit("stream_start", serde_json::json!({}));
    }

    fn on_stream_end(&self) {
        // Flush any remaining buffered content
        self.flush_buffer();
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

    fn confirm(&self, action: &ConfirmAction) -> crate::confirm::ConfirmResult {
        use crate::confirm::ConfirmResult;
        // In stdio mode, check auto-approve first
        if crate::confirm::is_auto_approve() {
            return ConfirmResult::Yes;
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
            ConfirmAction::ReviewPlan { preview } => serde_json::json!({
                "action": "review_plan",
                "preview": preview,
            }),
        };

        self.emit("confirm_request", action_data);

        // Read response from stdin
        let mut response = String::new();
        if std::io::stdin().read_line(&mut response).is_err() {
            return ConfirmResult::No;
        }

        // Parse JSON response: { "approved": true/false } or { "clarify": "question" }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
            if let Some(clarify) = parsed.get("clarify").and_then(|v| v.as_str()) {
                return ConfirmResult::Clarify(clarify.to_string());
            }
            if parsed.get("approved").and_then(|v| v.as_bool()).unwrap_or(false) {
                ConfirmResult::Yes
            } else {
                ConfirmResult::No
            }
        } else {
            // Fall back to plain text
            match response.trim().to_lowercase().as_str() {
                "y" | "yes" | "true" => ConfirmResult::Yes,
                "n" | "no" | "false" | "" => ConfirmResult::No,
                _ => ConfirmResult::Clarify(response.trim().to_string()),
            }
        }
    }

    fn ask_user(&self, question: &str) -> String {
        self.emit("ask_user", serde_json::json!({ "question": question }));
        let mut response = String::new();
        std::io::stdin().read_line(&mut response).ok();
        // Try JSON: { "answer": "..." }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
            if let Some(answer) = parsed.get("answer").and_then(|v| v.as_str()) {
                return answer.to_string();
            }
        }
        // Fallback: raw text
        response.trim().to_string()
    }

    fn review_plan(&self, plan_text: &str) -> PlanReview {
        self.emit("review_plan", serde_json::json!({ "plan": plan_text }));
        let mut response = String::new();
        std::io::stdin().read_line(&mut response).ok();
        // Expect: { "action": "approve" | "approve_with_context" | "reject" | "refine",
        //           "context": "...", "feedback": "..." }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
            match parsed.get("action").and_then(|v| v.as_str()).unwrap_or("") {
                "approve" | "y" => PlanReview::Approve,
                "approve_with_context" => {
                    let ctx = parsed.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if ctx.is_empty() { PlanReview::Approve } else { PlanReview::ApproveWithContext(ctx) }
                }
                "reject" | "n" => PlanReview::Reject,
                "refine" => {
                    let fb = parsed.get("feedback").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    PlanReview::Refine(fb)
                }
                _ => PlanReview::Reject,
            }
        } else {
            match response.trim().to_lowercase().as_str() {
                "y" | "yes" | "approve" => PlanReview::Approve,
                "n" | "no" | "reject" => PlanReview::Reject,
                _ => PlanReview::Refine(response.trim().to_string()),
            }
        }
    }

    fn inject_guidance(&self) -> Option<String> {
        self.emit("guidance_request", serde_json::json!({}));
        let mut response = String::new();
        std::io::stdin().read_line(&mut response).ok();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
            if let Some(text) = parsed.get("guidance").and_then(|v| v.as_str()) {
                let t = text.trim().to_string();
                return if t.is_empty() { None } else { Some(t) };
            }
        }
        let t = response.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
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

    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        let (kind, data) = match event {
            SubAgentOutputEvent::StreamStart => ("sub_stream_start", serde_json::json!({})),
            SubAgentOutputEvent::StreamEnd   => ("sub_stream_end",   serde_json::json!({})),
            SubAgentOutputEvent::Token(t) => (
                "sub_token", serde_json::json!({ "token": t })
            ),
            SubAgentOutputEvent::ToolUse { name } => (
                "sub_tool_use", serde_json::json!({ "tool": name })
            ),
            SubAgentOutputEvent::ToolDone { name, is_error } => (
                "sub_tool_done", serde_json::json!({ "tool": name, "is_error": is_error })
            ),
            SubAgentOutputEvent::Done(text) => (
                "sub_done", serde_json::json!({ "text": text })
            ),
            SubAgentOutputEvent::Error(msg) => (
                "sub_error", serde_json::json!({ "message": msg })
            ),
        };
        self.emit(kind, serde_json::json!({ "task_id": task_id, "data": data }));
    }

    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        self.emit("service_notification", serde_json::json!({
            "source": source,
            "level": level.as_str(),
            "message": message,
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
    confirm_rx: Mutex<std::sync::mpsc::Receiver<crate::confirm::ConfirmResult>>,
    /// Sends confirm responses (held by the reader task).
    pub confirm_tx: std::sync::mpsc::Sender<crate::confirm::ConfirmResult>,
    /// Receives ask_user responses from the reader task.
    ask_user_rx: Mutex<std::sync::mpsc::Receiver<String>>,
    /// Sends ask_user responses (held by the reader task).
    pub ask_user_tx: std::sync::mpsc::Sender<String>,
}

impl WsOutput {
    /// Create a new WsOutput.
    ///
    /// - `tx`: channel to the writer task that sends frames on the socket.
    /// - Returns `(WsOutput, confirm_tx)` — caller gives `confirm_tx` to the
    ///   reader task so it can forward confirm responses.
    pub fn new(tx: mpsc::UnboundedSender<WsCommand>) -> Self {
        let (confirm_tx, confirm_rx) = std::sync::mpsc::channel();
        let (ask_user_tx, ask_user_rx) = std::sync::mpsc::channel();
        WsOutput {
            tx,
            confirm_rx: Mutex::new(confirm_rx),
            confirm_tx,
            ask_user_rx: Mutex::new(ask_user_rx),
            ask_user_tx,
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

    fn on_role_header(&self, label: &str, model: &str) {
        self.emit("role_header", serde_json::json!({ "label": label, "model": model }));
    }

    fn on_stage_end(&self, label: &str) {
        self.emit("stage_end", serde_json::json!({ "label": label }));
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

    fn confirm(&self, action: &ConfirmAction) -> crate::confirm::ConfirmResult {
        use crate::confirm::ConfirmResult;
        if crate::confirm::is_auto_approve() {
            return ConfirmResult::Yes;
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
            ConfirmAction::ReviewPlan { preview } => serde_json::json!({
                "action": "review_plan",
                "preview": preview,
            }),
        };

        // Send confirm_request over WebSocket
        self.emit("confirm_request", action_data);

        // Block the current thread until the reader task forwards the response.
        // block_in_place tells the tokio multi-thread scheduler to move other
        // tasks off this thread, so the reader task can keep running and deliver
        // the response without deadlocking.
        let rx = self.confirm_rx.lock().unwrap();
        tokio::task::block_in_place(|| rx.recv().unwrap_or(ConfirmResult::No))
    }

    fn ask_user(&self, question: &str) -> String {
        self.emit("ask_user", serde_json::json!({ "question": question }));
        let rx = self.ask_user_rx.lock().unwrap();
        tokio::task::block_in_place(|| rx.recv().unwrap_or_default())
    }

    fn review_plan(&self, plan_text: &str) -> PlanReview {
        self.emit("review_plan", serde_json::json!({ "plan": plan_text }));
        // Reuse ask_user channel for the response
        let rx = self.ask_user_rx.lock().unwrap();
        let response = tokio::task::block_in_place(|| rx.recv().unwrap_or_default());
        // Try JSON: {"action": "approve"|"approve_with_context"|"reject"|"refine",
        //            "context": "...", "feedback": "..."}
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
            match parsed.get("action").and_then(|v| v.as_str()).unwrap_or("") {
                "approve" | "y" => PlanReview::Approve,
                "approve_with_context" => {
                    let ctx = parsed.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if ctx.is_empty() { PlanReview::Approve } else { PlanReview::ApproveWithContext(ctx) }
                }
                "reject" | "n" => PlanReview::Reject,
                "refine" => {
                    let fb = parsed.get("feedback").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    PlanReview::Refine(fb)
                }
                _ => PlanReview::Reject,
            }
        } else {
            match response.trim().to_lowercase().as_str() {
                "y" | "yes" | "approve" => PlanReview::Approve,
                "n" | "no" | "reject" | "" => PlanReview::Reject,
                _ => PlanReview::Refine(response.trim().to_string()),
            }
        }
    }

    fn inject_guidance(&self) -> Option<String> {
        self.emit("guidance_request", serde_json::json!({}));
        let rx = self.ask_user_rx.lock().unwrap();
        let response = rx.recv().unwrap_or_default();
        let t = response.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
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

    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        let (kind, data) = match event {
            SubAgentOutputEvent::StreamStart => ("sub_stream_start", serde_json::json!({})),
            SubAgentOutputEvent::StreamEnd   => ("sub_stream_end",   serde_json::json!({})),
            SubAgentOutputEvent::Token(t) => (
                "sub_token", serde_json::json!({ "token": t })
            ),
            SubAgentOutputEvent::ToolUse { name } => (
                "sub_tool_use", serde_json::json!({ "tool": name })
            ),
            SubAgentOutputEvent::ToolDone { name, is_error } => (
                "sub_tool_done", serde_json::json!({ "tool": name, "is_error": is_error })
            ),
            SubAgentOutputEvent::Done(text) => (
                "sub_done", serde_json::json!({ "text": text })
            ),
            SubAgentOutputEvent::Error(msg) => (
                "sub_error", serde_json::json!({ "message": msg })
            ),
        };
        self.emit(kind, serde_json::json!({ "task_id": task_id, "data": data }));
    }

    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        self.emit("service_notification", serde_json::json!({
            "source": source,
            "level": level.as_str(),
            "message": message,
        }));
    }
}
