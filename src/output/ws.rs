use super::*;

// ═══════════════════════════════════════════════════════════════════
//  WebSocket output — JSON frames over a WebSocket connection
// ═══════════════════════════════════════════════════════════════════

use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Monotonically increasing event sequence number for reconnection recovery.
    seq: AtomicU64,
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
            seq: AtomicU64::new(0),
        }
    }

    /// Serialize and send a JSON event to the WebSocket.
    fn emit(&self, event_type: &str, data: serde_json::Value) {
        self.emit_with_id(event_type, data, None);
    }

    /// Serialize and send a JSON event to the WebSocket, including an optional `id` in the envelope.
    fn emit_with_id(&self, event_type: &str, data: serde_json::Value, id: Option<&str>) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let mut msg = serde_json::json!({
            "type": event_type,
            "seq": seq,
            "data": data,
        });
        if let Some(ref_id) = id {
            msg["id"] = serde_json::Value::String(ref_id.to_string());
        }
        let text = serde_json::to_string(&msg).unwrap_or_default();
        // Ignore send errors (connection may have closed)
        let _ = self.tx.send(WsCommand::Send(text));
    }

    /// Public variant of emit for use from the server module.
    pub fn emit_public(&self, event_type: &str, data: serde_json::Value) {
        self.emit(event_type, data);
    }

    /// Public variant that includes an `id` in the envelope (e.g. request correlation).
    pub fn emit_public_with_id(&self, event_type: &str, data: serde_json::Value, id: &str) {
        self.emit_with_id(event_type, data, Some(id));
    }

    /// Return the last emitted sequence number.
    pub fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed).saturating_sub(1)
    }
}

#[async_trait::async_trait]
impl AgentOutput for WsOutput {
    fn on_thinking(&self) {
        self.emit("thinking", serde_json::json!({}));
    }

    fn on_thinking_start(&self) {
        self.emit("thinking_start", serde_json::json!({}));
    }

    fn on_thinking_token(&self, token: &str) {
        self.emit("thinking_token", serde_json::json!({ "token": token }));
    }

    fn on_thinking_end(&self) {
        self.emit("thinking_end", serde_json::json!({}));
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

    fn on_tool_use(&self, name: &str, input: &serde_json::Value, tool_id: &str) {
        self.emit_with_id("tool_use", serde_json::json!({
            "tool": name,
            "input": input,
        }), Some(tool_id));
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        self.emit("tool_result", serde_json::json!({
            "tool": name,
            "output": result.output,
            "is_error": result.is_error,
        }));
    }

    fn on_file_created(&self, path: &str) {
        self.emit("file", serde_json::json!({ "path": path }));
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
    fn on_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        self.emit("notification", serde_json::json!({
            "source": source,
            "level": level.as_str(),
            "message": message,
        }));
    }
}
