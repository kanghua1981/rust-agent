//! Tool: call_node
//!
//! The single unified tool for delegating tasks to another agent instance.
//! Replaces the old `call_sub_agent` (WS direct URL) and `spawn_sub_agent`
//! (local stdio process) tools, combining them under one clear interface.
//!
//! # Target formats
//!   "gpu-box"              → look up [[remote]] entry in workspaces.toml
//!   "ws://1.2.3.4:9527"   → direct WebSocket URL (one-off / no config needed)
//!   "any:gpu"              → (Phase 2) pick first online node with tag
//!   "all:embedded"         → (Phase 2) broadcast to all nodes with tag
//!
//! # Configuration (workspaces.toml — the single config file)
//! Both outbound remotes ([[remote]]) and inbound workspaces ([[workspace]])
//! live in the same file, alongside the cluster token ([cluster]).
//! Explicit parameters passed to this tool always override config defaults.
//! They are forwarded to the server as URL query parameters:
//!   ws://host:9527/?workdir=%2Fhome%2Fuser%2Fproject&sandbox=1&token=xxx

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::confirm::{ConfirmAction, ConfirmResult};
use crate::output::AgentOutput;
use crate::tools::{Tool, ToolDefinition, ToolResult};
use crate::agent::is_interrupted;

const TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
const PING_INTERVAL: Duration = Duration::from_secs(15);
const INACTIVITY_WARN: Duration = Duration::from_secs(60);
const INACTIVITY_ABORT: Duration = Duration::from_secs(180);

// ── workspaces.toml parsing is handled by crate::workspaces ──────────────────
use crate::workspaces;

fn load_workspaces_file(project_dir: &Path) -> workspaces::WorkspacesFile {
    workspaces::load(project_dir)
}

// ── Tool input ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct CallNodeInput {
    /// Node name (from nodes.toml), direct `ws://` URL, or future `any:<tag>`.
    target: String,
    /// Task description for the remote agent.
    prompt: String,
    /// Override the node's default working directory.
    #[serde(default)]
    workdir: Option<String>,
    /// Override the node's default sandbox setting.
    #[serde(default)]
    sandbox: Option<bool>,
    /// Auto-approve all tool confirmations from the remote agent.
    #[serde(default)]
    auto_approve: bool,
    /// Total timeout in seconds (default 600).
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 { 600 }

// ── URL builder ───────────────────────────────────────────────────────────────

/// Append `workdir`, `sandbox` and `token` as URL query parameters.
fn build_url(base: &str, workdir: Option<&str>, sandbox: Option<bool>, token: Option<&str>) -> String {
    let mut params: Vec<String> = Vec::new();

    if let Some(wd) = workdir {
        params.push(format!("workdir={}", url_encode(wd)));
    }
    if let Some(sb) = sandbox {
        params.push(format!("sandbox={}", if sb { "1" } else { "0" }));
    }
    if let Some(tok) = token {
        if !tok.is_empty() {
            params.push(format!("token={}", url_encode(tok)));
        }
    }

    if params.is_empty() {
        base.to_string()
    } else {
        // Append to existing query string if present.
        let sep = if base.contains('?') { '&' } else { '?' };
        format!("{}{}{}", base, sep, params.join("&"))
    }
}

fn url_encode(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '/') {
            vec![c]
        } else {
            format!("%{:02X}", c as u32).chars().collect()
        }
    }).collect()
}

// ── WebSocket event types (same as call_sub_agent) ───────────────────────────

enum NodeEvent {
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
    Ready { workdir: Option<String>, sandbox: Option<bool> },
    Unknown,
}

fn parse_event(ev: &Value) -> NodeEvent {
    match ev["type"].as_str() {
        Some("stream_start")    => NodeEvent::StreamStart,
        Some("stream_end")      => NodeEvent::StreamEnd,
        Some("streaming_token") => {
            let token = ev["data"]["token"].as_str().or_else(|| ev["token"].as_str()).unwrap_or("").to_string();
            NodeEvent::StreamingToken(token)
        }
        Some("assistant_text") => {
            let text = ev["data"]["text"].as_str().or_else(|| ev["content"].as_str()).unwrap_or("").to_string();
            NodeEvent::AssistantText(text)
        }
        Some("thinking") | Some("thought") => {
            let thought = ev["data"]["thought"].as_str()
                .or_else(|| ev["data"]["content"].as_str())
                .or_else(|| ev["content"].as_str())
                .unwrap_or("").to_string();
            NodeEvent::Thinking(thought)
        }
        Some("tool_use") => {
            let name = ev["data"]["name"].as_str().or_else(|| ev["data"]["tool"].as_str()).unwrap_or("unknown").to_string();
            let input = ev["data"]["input"].clone();
            NodeEvent::ToolUse { name, input }
        }
        Some("tool_result") => {
            let name = ev["data"]["name"].as_str().or_else(|| ev["data"]["tool"].as_str()).unwrap_or("unknown").to_string();
            let output = ev["data"]["output"].as_str().or_else(|| ev["data"]["result"].as_str()).unwrap_or("").to_string();
            NodeEvent::ToolResult { name, output }
        }
        Some("confirm_request") => {
            let action = ev["data"]["action"].as_str().unwrap_or("").to_string();
            let details = if let Some(cmd) = ev["data"]["command"].as_str() {
                Some(cmd.to_string())
            } else if let Some(path) = ev["data"]["path"].as_str() {
                let lines = ev["data"]["lines"].as_u64();
                Some(match lines {
                    Some(n) => format!("{} ({} lines)", path, n),
                    None => path.to_string(),
                })
            } else {
                ev["data"]["details"].as_str().map(|s| s.to_string())
            };
            NodeEvent::ConfirmRequest { action, details }
        }
        Some("done") | Some("final_response") => {
            let text = ev["data"]["text"].as_str().or_else(|| ev["text"].as_str()).unwrap_or("").to_string();
            NodeEvent::Done(text)
        }
        Some("error") => {
            let msg = ev["data"]["message"].as_str().or_else(|| ev["message"].as_str()).unwrap_or("Unknown error").to_string();
            NodeEvent::Error(msg)
        }
        Some("ready") => {
            let workdir = ev["data"]["workdir"].as_str().map(|s| s.to_string());
            let sandbox = ev["data"]["sandbox"].as_bool();
            NodeEvent::Ready { workdir, sandbox }
        }
        _ => NodeEvent::Unknown,
    }
}

// ── Tool impl ─────────────────────────────────────────────────────────────────

pub struct CallNodeTool {
    output: Arc<dyn AgentOutput>,
}

impl CallNodeTool {
    pub fn new(output: Arc<dyn AgentOutput>) -> Self {
        CallNodeTool { output }
    }
}

#[async_trait]
impl Tool for CallNodeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "call_node".to_string(),
            description: "Delegate a task to another agent instance running on a remote machine \
                or a different project directory. This is the ONLY tool for agent-to-agent \
                collaboration — do NOT use connect_service or query_service for agent servers.\n\
                \n\
                `target` can be:\n\
                  - A node name defined in nodes.toml (e.g. \"upper-pc\", \"build-server\")\n\
                  - A direct WebSocket URL (e.g. \"ws://192.168.1.10:9527\")\n\
                  - Tag-based routing (Phase 2): \"any:<tag>\" or \"all:<tag>\"\n\
                \n\
                The remote agent runs autonomously, uses its own tools and LLM, then returns \
                the final result. All intermediate tool calls are shown in real time."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Node name from nodes.toml, direct ws:// URL, or tag routing like 'any:gpu'."
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Task description for the remote agent."
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Override the working directory on the remote node. \
                            If omitted, uses the node's configured default."
                    },
                    "sandbox": {
                        "type": "boolean",
                        "description": "Override whether the remote agent runs in sandbox mode. \
                            If omitted, uses the node's configured default."
                    },
                    "auto_approve": {
                        "type": "boolean",
                        "description": "Auto-approve all tool confirmations from the remote agent.",
                        "default": false
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Maximum seconds to wait (default 600).",
                        "default": 600
                    }
                },
                "required": ["target", "prompt"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let input: CallNodeInput = match serde_json::from_value(input.clone()) {
            Ok(i) => i,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // ── Resolve target → base_url + defaults ──────────────────────────────
        let is_direct_url = input.target.starts_with("ws://") || input.target.starts_with("wss://");

        let (base_url, node_workdir, node_sandbox, cluster_token) = if is_direct_url {
            (input.target.clone(), None::<String>, None::<bool>, None::<String>)
        } else {
            let cfg = load_workspaces_file(project_dir);

            // Phase 2 tag routing stubs.
            if input.target.starts_with("any:") || input.target.starts_with("all:") || input.target.starts_with("best:") {
                return ToolResult::error(format!(
                    "Tag-based routing ('{}') is a Phase 2 feature and not yet implemented. \
                     Use a named remote or a direct ws:// URL.",
                    input.target
                ));
            }

            // Look up [[remote]] first, then legacy [[servers]] / [[nodes]].
            let token = cfg.cluster.token.clone();
            let all_remotes = cfg.all_remotes();
            if let Some(entry) = all_remotes.iter().find(|e| e.name == input.target) {
                // workdir/sandbox defaults come from the server's ready frame,
                // not from this config — so we pass None here.
                (entry.url.clone(), None::<String>, None::<bool>, token)
            } else {
                let names: Vec<&str> = all_remotes.iter().map(|e| e.name.as_str()).collect();
                return ToolResult::error(format!(
                    "Remote '{}' not found in workspaces.toml [[remote]] entries. \
                     Known remotes: [{}]. \
                     Add a [[remote]] entry or use a direct ws:// URL.",
                    input.target,
                    names.join(", ")
                ));
            }
        };

        // Explicit parameters override nodes.toml defaults.
        let effective_workdir = input.workdir.as_deref().or(node_workdir.as_deref());
        let effective_sandbox = input.sandbox.or(node_sandbox);

        // ── Build final URL ───────────────────────────────────────────────────
        let url = build_url(&base_url, effective_workdir, effective_sandbox, cluster_token.as_deref());

        // ── Connect ───────────────────────────────────────────────────────────
        self.output.on_warning(&format!("[call_node] connecting to {} (target={})", url, input.target));

        let (ws_stream, _) = match connect_async(&url).await {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!(
                "Failed to connect to node '{}' at {}: {}", input.target, url, e
            )),
        };
        let (mut write, mut read) = ws_stream.split();

        // ── Send initial task ─────────────────────────────────────────────────
        let initial_msg = json!({
            "type": "user_message",
            "data": { "text": input.prompt }
        });
        if let Err(e) = write.send(Message::Text(initial_msg.to_string().into())).await {
            return ToolResult::error(format!("Failed to send task to node: {}", e));
        }

        // ── Event loop ────────────────────────────────────────────────────────
        let mut streaming_answer = String::new();
        // Set when the Ready frame arrives; prepended to every ToolResult so
        // the manager LLM sees where the worker is and what mode it runs in.
        let mut connect_header = String::new();
        let total_timeout = Duration::from_secs(input.timeout_secs);
        let start_time = Instant::now();
        let mut last_msg_at = Instant::now();
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
                                NodeEvent::StreamStart => {
                                    self.output.on_stream_start();
                                }
                                NodeEvent::StreamEnd => {
                                    self.output.on_stream_end();
                                }
                                NodeEvent::StreamingToken(token) if !token.is_empty() => {
                                    self.output.on_streaming_text(&token);
                                    streaming_answer.push_str(&token);
                                }
                                NodeEvent::AssistantText(text) if !text.is_empty() => {
                                    self.output.on_assistant_text(&text);
                                    streaming_answer = text;
                                }
                                NodeEvent::Thinking(thought) if !thought.is_empty() => {
                                    self.output.on_warning(&format!("[node:{}] {}", input.target, thought));
                                }
                                NodeEvent::ToolUse { name, input: tool_input } => {
                                    self.output.on_tool_use(&format!("↳ {}", name), &tool_input);
                                }
                                NodeEvent::ToolResult { name, output: tool_out } => {
                                    self.output.on_tool_result(
                                        &format!("↳ {}", name),
                                        &ToolResult::success(tool_out),
                                    );
                                }
                                NodeEvent::ConfirmRequest { action, details } => {
                                    let approved = if input.auto_approve || crate::confirm::is_auto_approve() {
                                        true
                                    } else {
                                        let confirm_action = match action.as_str() {
                                            "write_file" => ConfirmAction::WriteFile {
                                                path: details.clone().unwrap_or_default(),
                                                lines: 0,
                                            },
                                            "edit_file" => ConfirmAction::EditFile {
                                                path: details.clone().unwrap_or_default(),
                                            },
                                            "delete_file" => ConfirmAction::DeleteFile {
                                                path: details.clone().unwrap_or_default(),
                                            },
                                            _ => ConfirmAction::RunCommand {
                                                command: match &details {
                                                    Some(d) => format!("[node:{}] {} — {}", input.target, action, d),
                                                    None => format!("[node:{}] {}", input.target, action),
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
                                NodeEvent::Done(text) => {
                                    let final_text = if !text.is_empty() { text } else { streaming_answer };
                                    let _ = write.send(Message::Close(None)).await;
                                    self.output.on_stage_end(&format!("Node:{}", input.target));
                                    return ToolResult::success(format!(
                                        "{}Node '{}' completed.\n{}", connect_header, input.target, final_text
                                    ));
                                }
                                NodeEvent::Error(msg) => {
                                    let _ = write.send(Message::Close(None)).await;
                                    self.output.on_stage_end(&format!("Node:{}", input.target));
                                    return ToolResult::error(format!(
                                        "Node '{}' error: {}", input.target, msg
                                    ));
                                }
                                NodeEvent::Ready { workdir, sandbox } => {
                                    let wd = workdir.as_deref().unwrap_or("(server default)");
                                    let sb = match sandbox {
                                        Some(true)  => "on",
                                        Some(false) => "off",
                                        None        => "unknown",
                                    };
                                    connect_header = format!(
                                        "[Node '{}' connected — workdir: {}  sandbox: {}]\n\n",
                                        input.target, wd, sb
                                    );
                                    self.output.on_warning(&format!(
                                        "[call_node] node '{}' ready — workdir={} sandbox={}",
                                        input.target, wd, sb
                                    ));
                                }
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(e)) => {
                            self.output.on_stage_end(&format!("Node:{}", input.target));
                            return ToolResult::error(format!("WebSocket error from node '{}': {}", input.target, e));
                        }
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(PING_INTERVAL) => {
                    let elapsed = start_time.elapsed();
                    let idle    = last_msg_at.elapsed();
                    let elapsed_min = elapsed.as_secs() / 60;

                    if is_interrupted() {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end(&format!("Node:{}", input.target));
                        return ToolResult::error("call_node interrupted by user (Ctrl-C).".to_string());
                    }

                    if elapsed >= total_timeout {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end(&format!("Node:{}", input.target));
                        return ToolResult::error(format!(
                            "Node '{}' timed out after {}s.", input.target, elapsed.as_secs()
                        ));
                    }

                    if idle >= INACTIVITY_ABORT {
                        let _ = write.send(Message::Close(None)).await;
                        self.output.on_stage_end(&format!("Node:{}", input.target));
                        return ToolResult::error(format!(
                            "Node '{}' appears stuck: no activity for {}s.",
                            input.target, idle.as_secs()
                        ));
                    }

                    let _ = write.send(Message::Ping(vec![].into())).await;

                    if idle >= INACTIVITY_WARN && idle < INACTIVITY_WARN + PING_INTERVAL {
                        self.output.on_warning(&format!(
                            "[node:{}] no activity for {}s — waiting (Ctrl-C to abort)",
                            input.target, idle.as_secs()
                        ));
                    }

                    if elapsed_min > last_reported_min {
                        last_reported_min = elapsed_min;
                        self.output.on_warning(&format!(
                            "[node:{}] {}m{}s elapsed (Ctrl-C to abort)",
                            input.target, elapsed_min, elapsed.as_secs() % 60
                        ));
                    }
                }
            }
        }

        self.output.on_stage_end(&format!("Node:{}", input.target));
        ToolResult::success(format!(
            "{}Node '{}' connection closed.\n{}", connect_header, input.target, streaming_answer
        ))
    }
}
