use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::confirm::{ConfirmAction, ConfirmResult};
use crate::output::AgentOutput;
use crate::tools::{Tool, ToolDefinition, ToolResult};
use crate::agent::is_interrupted;

/// How long the sub-agent may run in total before we forcibly close the connection.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
/// Keep-alive ping interval.
const PING_INTERVAL: Duration = Duration::from_secs(15);
/// Warn if no WebSocket message has been received for this long.
/// Normal LLM calls take up to ~60s; anything beyond that is suspicious.
const INACTIVITY_WARN: Duration = Duration::from_secs(60);
/// Abort if completely silent for this long — the sub-agent is likely stuck.
const INACTIVITY_ABORT: Duration = Duration::from_secs(180);

/// Delegates a sub-task to an agent running in `--mode server` (WebSocket).
/// All events (streaming tokens, tool uses, confirmations) are proxied through
/// the parent agent's `AgentOutput`, so they work correctly in CLI, stdio,
/// and server modes without ever touching stdout/stderr directly.
pub struct CallSubAgentTool {
    output: Arc<dyn AgentOutput>,
}

impl CallSubAgentTool {
    pub fn new(output: Arc<dyn AgentOutput>) -> Self {
        CallSubAgentTool { output }
    }
}

#[derive(Serialize, Deserialize)]
struct CallSubAgentInput {
    /// The specific task for the sub-agent.
    prompt: String,
    /// WebSocket URL of the sub-agent server. Defaults to ws://localhost:9527.
    #[serde(default = "default_server_url")]
    server_url: String,
    /// Optional sub-directory hint passed as part of the prompt.
    #[serde(default)]
    target_dir: Option<String>,
    /// When true, sub-agent tool uses are auto-approved without prompting the user.
    #[serde(default)]
    auto_approve: bool,
}

fn default_server_url() -> String {
    "ws://localhost:9527".to_string()
}

// ── Typed event enum ──────────────────────────────────────────────────────────

enum SubAgentEvent {
    StreamStart,
    StreamEnd,
    StreamingToken(String),
    AssistantText(String),
    Thinking(String),
    ToolUse { name: String, input: Value },
    ToolResult { name: String, output: String },
    ConfirmRequest { action: String, details: Option<String> },
    Done(String),
    Error(String),
    /// Sub-agent server sent a "ready" handshake (includes reported version).
    Ready(String),
    Unknown,
}

/// Map a raw JSON event frame to a `SubAgentEvent`.
/// The server always uses `{"type": "...", "data": {...}}`, so we prefer
/// `data.*` fields and fall back to top-level fields for forward compatibility.
fn parse_event(ev: &Value) -> SubAgentEvent {
    match ev["type"].as_str() {
        Some("stream_start") => SubAgentEvent::StreamStart,
        Some("stream_end")   => SubAgentEvent::StreamEnd,
        Some("streaming_token") => {
            let token = ev["data"]["token"].as_str()
                .or_else(|| ev["token"].as_str())
                .unwrap_or("")
                .to_string();
            SubAgentEvent::StreamingToken(token)
        }
        Some("assistant_text") => {
            let text = ev["data"]["text"].as_str()
                .or_else(|| ev["content"].as_str())
                .unwrap_or("")
                .to_string();
            SubAgentEvent::AssistantText(text)
        }
        Some("thinking") | Some("thought") => {
            let thought = ev["data"]["thought"].as_str()
                .or_else(|| ev["data"]["content"].as_str())
                .or_else(|| ev["content"].as_str())
                .unwrap_or("")
                .to_string();
            SubAgentEvent::Thinking(thought)
        }
        Some("tool_use") => {
            let name = ev["data"]["name"].as_str()
                .or_else(|| ev["data"]["tool"].as_str())
                .or_else(|| ev["tool"].as_str())
                .unwrap_or("unknown")
                .to_string();
            let input = ev["data"]["input"].clone();
            SubAgentEvent::ToolUse { name, input }
        }
        Some("tool_result") => {
            let name = ev["data"]["name"].as_str()
                .or_else(|| ev["data"]["tool"].as_str())
                .or_else(|| ev["tool"].as_str())
                .unwrap_or("unknown")
                .to_string();
            let output = ev["data"]["output"].as_str()
                .or_else(|| ev["data"]["result"].as_str())
                .or_else(|| ev["output"].as_str())
                .unwrap_or("")
                .to_string();
            SubAgentEvent::ToolResult { name, output }
        }
        Some("confirm_request") => {
            // Carry the full `data` object so the handler can reconstruct
            // the exact ConfirmAction variant with all relevant fields.
            let action = ev["data"]["action"].as_str()
                .or_else(|| ev["action"].as_str())
                .unwrap_or("")
                .to_string();
            // Build a human-readable details string from whichever specific
            // fields the server included (path, command, lines, etc.).
            let details = if let Some(cmd) = ev["data"]["command"].as_str() {
                Some(cmd.to_string())
            } else if let Some(path) = ev["data"]["path"].as_str() {
                let lines = ev["data"]["lines"].as_u64();
                Some(match lines {
                    Some(n) => format!("{} ({} lines)", path, n),
                    None    => path.to_string(),
                })
            } else if let Some(d) = ev["data"]["details"].as_str() {
                Some(d.to_string())
            } else {
                None
            };
            SubAgentEvent::ConfirmRequest { action, details }
        }
        Some("done") | Some("final_response") => {
            let text = ev["data"]["text"].as_str()
                .or_else(|| ev["text"].as_str())
                .unwrap_or("")
                .to_string();
            SubAgentEvent::Done(text)
        }
        Some("error") => {
            let msg = ev["data"]["message"].as_str()
                .or_else(|| ev["message"].as_str())
                .unwrap_or("Unknown sub-agent error")
                .to_string();
            SubAgentEvent::Error(msg)
        }
        Some("ready") => {
            let version = ev["data"]["version"].as_str().unwrap_or("?").to_string();
            SubAgentEvent::Ready(version)
        }
        _ => SubAgentEvent::Unknown,
    }
}

// \u2500\u2500 Tool impl \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

#[async_trait]
impl Tool for CallSubAgentTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "call_sub_agent".to_string(),
            description: "Delegates a complex sub-task to another agent instance running in server \
                mode. All events are proxied through the parent agent's output channel, so this \
                works correctly in CLI, stdio, and server modes."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The specific task description for the sub-agent."
                    },
                    "server_url": {
                        "type": "string",
                        "description": "WebSocket URL of the sub-agent server.",
                        "default": "ws://localhost:9527"
                    },
                    "target_dir": {
                        "type": "string",
                        "description": "Optional sub-directory hint included in the prompt so \
                            the sub-agent focuses there."
                    },
                    "auto_approve": {
                        "type": "boolean",
                        "description": "When true all sub-agent tool confirmations are auto-approved. \
                            Use with caution.",
                        "default": false
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let input: CallSubAgentInput = match serde_json::from_value(input.clone()) {
            Ok(i) => i,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // \u2500\u2500 Connect \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
        let (ws_stream, _) = match connect_async(&input.server_url).await {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!(
                "Failed to connect to sub-agent at {}: {}", input.server_url, e
            )),
        };

        let (mut write, mut read) = ws_stream.split();

        // \u2500\u2500 Send initial task \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
        let full_prompt = match &input.target_dir {
            Some(dir) => format!("Please work in the directory: {}. Task: {}", dir, input.prompt),
            None => input.prompt.clone(),
        };

        let initial_msg = json!({
            "type": "user_message",
            "data": { "text": full_prompt },
            // Pass target_dir as allowed_dir so the sub-agent's tool executor
            // rejects any write/edit outside this directory.
            "allowed_dir": input.target_dir,
        });

        if let Err(e) = write.send(Message::Text(initial_msg.to_string().into())).await {
            return ToolResult::error(format!("Failed to send initial message: {}", e));
        }

        // \u2500\u2500 Event loop \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
        // `streaming_answer` accumulates tokens as the sub-agent streams.
        // It is only used as a fallback when the "done" frame carries no text.
        let mut streaming_answer = String::new();
        let start_time = Instant::now();
        // Track when we last received any WS message to detect silent hangs.
        let mut last_msg_at = Instant::now();
        // Track the last minute at which we printed a progress note.
        let mut last_reported_min: u64 = 0;

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            last_msg_at = Instant::now();
                            let event: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };

                            match parse_event(&event) {
                                SubAgentEvent::StreamStart => {
                                    self.output.on_stream_start();
                                }
                                SubAgentEvent::StreamEnd => {
                                    self.output.on_stream_end();
                                }
                                SubAgentEvent::StreamingToken(token) if !token.is_empty() => {
                                    self.output.on_streaming_text(&token);
                                    streaming_answer.push_str(&token);
                                }
                                SubAgentEvent::AssistantText(text) if !text.is_empty() => {
                                    // A full non-streaming text block replaces accumulated tokens.
                                    self.output.on_assistant_text(&text);
                                    streaming_answer = text;
                                }
                                SubAgentEvent::Thinking(thought) if !thought.is_empty() => {
                                    // Show thinking as a prefixed warning so it's visible
                                    // but distinct from normal output.
                                    self.output.on_warning(&format!("[sub-agent] {}", thought));
                                }
                                SubAgentEvent::ToolUse { name, input: tool_input } => {
                                    // Prefix with ↳ so nested tool calls are visually distinct
                                    // from the main agent's tool calls.
                                    self.output.on_tool_use(&format!("↳ {}", name), &tool_input);
                                }
                                SubAgentEvent::ToolResult { name, output: tool_out } => {
                                    self.output.on_tool_result(&format!("↳ {}", name), &ToolResult::success(tool_out));
                                }
                                SubAgentEvent::ConfirmRequest { action, details } => {
                                    let approved = if input.auto_approve || crate::confirm::is_auto_approve() {
                                        true
                                    } else {
                                        // Reconstruct the proper ConfirmAction variant so
                                        // the prompt shows the right icon and detail text.
                                        let confirm_action = match action.as_str() {
                                            "write_file" => ConfirmAction::WriteFile {
                                                path: details.clone().unwrap_or_default(),
                                                lines: 0, // lines already embedded in details string
                                            },
                                            "edit_file" => ConfirmAction::EditFile {
                                                path: details.clone().unwrap_or_default(),
                                            },
                                            "delete_file" => ConfirmAction::DeleteFile {
                                                path: details.clone().unwrap_or_default(),
                                            },
                                            _ => ConfirmAction::RunCommand {
                                                command: match &details {
                                                    Some(d) => format!("[sub-agent] {} — {}", action, d),
                                                    None    => format!("[sub-agent] {}", action),
                                                },
                                            },
                                        };
                                        matches!(
                                            self.output.confirm(&confirm_action),
                                            ConfirmResult::Yes | ConfirmResult::AlwaysYes
                                        )
                                    };
                                    let response = json!({
                                        "type": "confirm_response",
                                        "data": { "approved": approved }
                                    });
                                    let _ = write.send(Message::Text(response.to_string().into())).await;
                                }
                                SubAgentEvent::Done(text) => {
                                    // Prefer the "done" frame's text; fall back to streamed tokens.
                                    let final_text = if !text.is_empty() { text } else { streaming_answer };
                                    let _ = write.send(Message::Close(None)).await;
                                    self.output.on_stage_end("Sub-Agent");
                                    self.note_server_running(&input.server_url);
                                    return ToolResult::success(
                                        format!("Sub-agent completed.\n{}", final_text)
                                    );
                                }
                                SubAgentEvent::Error(msg) => {
                                    let _ = write.send(Message::Close(None)).await;
                                    self.output.on_stage_end("Sub-Agent");
                                    self.note_server_running(&input.server_url);
                                    return ToolResult::error(format!("Sub-agent error: {}", msg));
                                }
                                SubAgentEvent::Ready(version) => {
                                    tracing::debug!("Sub-agent ready (version {})", version);
                                }
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(e)) => {
                            self.output.on_stage_end("Sub-Agent");
                            // Don't note server running here — WS error means the server
                            // may be gone or in an unknown state.
                            return ToolResult::error(format!("WebSocket error: {}", e));
                        }
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(PING_INTERVAL) => {
                    let elapsed     = start_time.elapsed();
                    let idle        = last_msg_at.elapsed();
                    let elapsed_min = elapsed.as_secs() / 60;

                    // Ctrl-C: abort immediately.
                    if is_interrupted() {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end("Sub-Agent");
                        return ToolResult::error(
                            "Sub-agent call interrupted by user (Ctrl-C).".to_string()
                        );
                    }

                    // Total wall-clock timeout.
                    if elapsed >= TOTAL_TIMEOUT {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end("Sub-Agent");
                        return ToolResult::error(format!(
                            "Sub-agent timed out after {}m — no response.",
                            elapsed.as_secs() / 60
                        ));
                    }

                    // Inactivity abort: no WS message for INACTIVITY_ABORT.
                    if idle >= INACTIVITY_ABORT {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end("Sub-Agent");
                        return ToolResult::error(format!(
                            "Sub-agent appears stuck: no activity for {}s. \
                             The sub-agent process may have crashed. \
                             Press Ctrl-C or restart the sub-agent server.",
                            idle.as_secs()
                        ));
                    }

                    // Keep-alive ping.
                    let _ = write.send(Message::Ping(vec![].into())).await;

                    // Inactivity warning (once, when first crossing INACTIVITY_WARN).
                    if idle >= INACTIVITY_WARN && idle < INACTIVITY_WARN + PING_INTERVAL {
                        self.output.on_warning(&format!(
                            "[sub-agent] no activity for {}s — waiting for LLM response \
                             (Ctrl-C to abort)",
                            idle.as_secs()
                        ));
                    }

                    // Periodic progress note: print once per elapsed minute.
                    if elapsed_min > last_reported_min {
                        last_reported_min = elapsed_min;
                        self.output.on_warning(&format!(
                            "[sub-agent] {}m{}s elapsed{} (Ctrl-C to abort)",
                            elapsed_min,
                            elapsed.as_secs() % 60,
                            if idle.as_secs() > 10 {
                                format!(", idle {}s", idle.as_secs())
                            } else {
                                String::new()
                            }
                        ));
                    }
                }
            }
        }

        // Server closed the connection cleanly.
        self.output.on_stage_end("Sub-Agent");
        self.note_server_running(&input.server_url);
        ToolResult::success(format!("Sub-agent connection closed.\n{}", streaming_answer))
    }
}

impl CallSubAgentTool {
    /// Print a one-line note reminding the user that the sub-agent server process is
    /// still running. This is informative for servers that were started manually;
    /// auto-spawned servers are killed by `main()` when the main agent exits.
    fn note_server_running(&self, url: &str) {
        self.output.on_warning(&format!(
            "ℹ  Sub-agent server at {} is still running \
             (auto-spawned: stopped on exit · manually started: stop it manually when done)",
            url
        ));
    }
}
