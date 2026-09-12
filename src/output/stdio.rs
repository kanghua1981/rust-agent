use super::*;

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
    /// Monotonically increasing event sequence number for reconnection recovery.
    seq: std::sync::Mutex<u64>,
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
            seq: std::sync::Mutex::new(0),
        }
    }

    /// Write a JSON event line to stdout.
    fn emit(&self, event_type: &str, data: serde_json::Value) {
        self.emit_with_id(event_type, data, None);
    }

    /// Write a JSON event line to stdout, including an optional `id` in the envelope.
    fn emit_with_id(&self, event_type: &str, data: serde_json::Value, id: Option<&str>) {
        let mut seq = self.seq.lock().unwrap();
        let current = *seq;
        *seq += 1;
        drop(seq);
        let mut msg = serde_json::json!({
            "type": event_type,
            "seq": current,
            "data": data,
        });
        if let Some(ref_id) = id {
            msg["id"] = serde_json::Value::String(ref_id.to_string());
        }
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
        let (inner_type, inner_data) = match event {
            SubAgentOutputEvent::StreamStart => ("stream_start", serde_json::json!({})),
            SubAgentOutputEvent::StreamEnd   => ("stream_end",   serde_json::json!({})),
            SubAgentOutputEvent::Token(t) => (
                "streaming_token", serde_json::json!({ "token": t })
            ),
            SubAgentOutputEvent::ToolUse { name } => (
                "tool_use", serde_json::json!({ "tool": name })
            ),
            SubAgentOutputEvent::ToolDone { name, is_error } => (
                "tool_result", serde_json::json!({ "tool": name, "is_error": is_error })
            ),
            SubAgentOutputEvent::Done(text) => (
                "done", serde_json::json!({ "text": text })
            ),
            SubAgentOutputEvent::Error(msg) => (
                "error", serde_json::json!({ "message": msg })
            ),
        };
        self.emit("agent_event", serde_json::json!({
            "agent_id": task_id,
            "event": {
                "type": inner_type,
                "data": inner_data,
            }
        }));
    }

    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        self.emit("service_notification", serde_json::json!({
            "source": source,
            "level": level.as_str(),
            "message": message,
        }));
    }
}
