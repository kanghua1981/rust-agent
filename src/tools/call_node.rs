//! Tool: call_node
//!
//! The single unified tool for delegating tasks to another agent instance.
//! Replaces the old `call_sub_agent` (WS direct URL) and `spawn_sub_agent`
//! (local stdio process) tools, combining them under one clear interface.
//!
//! # Target formats
//!   "gpu-box"              → named node (resolved via parent server's /nodes)
//!   "ws://1.2.3.4:9527"   → direct WebSocket URL (one-off / no config needed)
//!   "any:gpu"              → pick first online node with tag (from route table)
//!   "all:embedded"         → sequential broadcast to all nodes with tag
//!
//! # How node names are resolved
//! When running as a worker (forked by the server), the parent server exposes
//! a `GET /nodes` endpoint at `localhost:$AGENT_PARENT_PORT`.  call_node
//! queries that endpoint to resolve names and obtain the cluster token.
//! No filesystem access needed — this works inside a container namespace.
//!
//! Before using a named target, call `list_nodes` to see what's available.

use std::path::Path;
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
use crate::workspaces;

const TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
const PING_INTERVAL: Duration = Duration::from_secs(15);
const INACTIVITY_WARN: Duration = Duration::from_secs(60);
const INACTIVITY_ABORT: Duration = Duration::from_secs(180);

// ── Tool input ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct CallNodeInput {
    /// Node name (from list_nodes output), direct `ws://` URL, or `any:<tag>`.
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
    Ready {
        workdir: Option<String>,
        sandbox: Option<bool>,
        caps: Option<Value>,
        virtual_nodes: Vec<crate::workspaces::VirtualNodeInfo>,
    },
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
            let caps = if ev["data"]["caps"].is_object() {
                Some(ev["data"]["caps"].clone())
            } else {
                None
            };
            let virtual_nodes: Vec<crate::workspaces::VirtualNodeInfo> =
                if let Some(arr) = ev["data"]["virtual_nodes"].as_array() {
                    arr.iter()
                        .filter_map(|v| serde_json::from_value(v.clone()).ok())
                        .collect()
                } else {
                    vec![]
                };
            NodeEvent::Ready { workdir, sandbox, caps, virtual_nodes }
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
                collaboration.\n\
                \n\
                IMPORTANT: Call `list_nodes` first to see available node names. \
                Do not invent node names.\n\
                \n\
                `target` can be:\n\
                  - A node name returned by list_nodes (e.g. \"upper-pc\", \"build-server\")\n\
                  - A direct WebSocket URL (e.g. \"ws://192.168.1.10:9527\") — no lookup needed\n\
                  - Tag routing: \"any:<tag>\" picks the first node with that tag\n\
                \n\
                The remote agent runs autonomously and returns the final result. \
                All intermediate tool calls are shown in real time."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Node name from list_nodes output, a direct ws:// URL, or tag routing like 'any:gpu'. Call list_nodes first if you don't know the available names."
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
            let tok = std::env::var("AGENT_CLUSTER_TOKEN").ok();
            (input.target.clone(), None::<String>, None::<bool>, tok)
        } else {
            // Cluster token from env var injected by parent server before fork.
            let cluster_token = std::env::var("AGENT_CLUSTER_TOKEN").ok();

            // Query parent server's /nodes endpoint — works inside a filesystem
            // container because the network namespace is shared.
            let parent_port: u16 = std::env::var("AGENT_PARENT_PORT")
                .ok().and_then(|s| s.parse().ok()).unwrap_or(9527);
            let nodes_url = match &cluster_token {
                Some(tok) => format!("http://localhost:{}/nodes?token={}", parent_port, url_encode(tok)),
                None      => format!("http://localhost:{}/nodes", parent_port),
            };
            let nodes: Vec<serde_json::Value> = match reqwest::Client::new()
                .get(&nodes_url)
                .timeout(Duration::from_secs(5))
                .send().await
            {
                Ok(resp) => resp.json::<serde_json::Value>().await
                    .ok()
                    .and_then(|v| v["nodes"].as_array().cloned())
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("call_node: /nodes unreachable ({}), node list empty", e);
                    vec![]
                }
            };

            if input.target.starts_with("any:") || input.target.starts_with("best:") {
                let tag = input.target.splitn(2, ':').nth(1).unwrap_or("");
                let found = nodes.iter().find(|n| {
                    n["source"].as_str() == Some("route_table") &&
                    n["tags"].as_array()
                        .map(|a| a.iter().any(|t| t.as_str() == Some(tag)))
                        .unwrap_or(false)
                });
                match found {
                    Some(n) => (
                        n["url"].as_str().unwrap_or("").to_string(),
                        n["workdir"].as_str().map(|s| s.to_string()),
                        n["sandbox"].as_bool(),
                        cluster_token,
                    ),
                    None => return ToolResult::error(format!(
                        "No node found with tag '{}'. \
                         Call list_nodes to see available nodes, or connect to a server first \
                         (direct URL) to populate the route table.",
                        tag
                    )),
                }
            } else if input.target.starts_with("all:") {
                let tag = input.target.splitn(2, ':').nth(1).unwrap_or("");
                let matching: Vec<&serde_json::Value> = nodes.iter().filter(|n| {
                    n["tags"].as_array()
                        .map(|a| a.iter().any(|t| t.as_str() == Some(tag)))
                        .unwrap_or(false)
                }).collect();
                if matching.is_empty() {
                    return ToolResult::error(format!(
                        "No nodes found with tag '{}'. Call list_nodes to see available nodes.",
                        tag
                    ));
                }
                if matching.len() > 1 {
                    let rest: Vec<_> = matching[1..].iter()
                        .map(|n| n["name"].as_str().unwrap_or("?").to_string())
                        .collect();
                    self.output.on_warning(&format!(
                        "[call_node] all:{} matched {} nodes — running first; \
                         parallel broadcast planned for Phase 3. Skipped: {}",
                        tag, matching.len(), rest.join(", ")
                    ));
                }
                let first = matching[0];
                (
                    first["url"].as_str().unwrap_or("").to_string(),
                    first["workdir"].as_str().map(|s| s.to_string()),
                    first["sandbox"].as_bool(),
                    cluster_token,
                )
            } else {
                // Named target — look up in entries returned by parent's /nodes.
                let found = nodes.iter().find(|n| n["name"].as_str() == Some(input.target.as_str()));
                match found {
                    Some(n) => (n["url"].as_str().unwrap_or("").to_string(), None, None, cluster_token),
                    None => {
                        let names: Vec<&str> = nodes.iter()
                            .filter_map(|n| n["name"].as_str())
                            .collect();
                        return ToolResult::error(format!(
                            "Node '{}' not found. Call list_nodes to see available nodes. \
                             Known: [{}]",
                            input.target, names.join(", ")
                        ));
                    }
                }
            }
        };

        // Explicit parameters override node defaults.
        let effective_workdir = input.workdir.as_deref().or(node_workdir.as_deref());
        let effective_sandbox = input.sandbox.or(node_sandbox);

        // ── Build final URL ───────────────────────────────────────────────────
        // Ensure the URL targets /agent (not the root path).
        let base_url = crate::workspaces::with_path(&base_url, "/agent");
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
                                NodeEvent::Ready { workdir, sandbox, caps, virtual_nodes } => {
                                    let wd = workdir.as_deref().unwrap_or("(server default)");
                                    let sb = match sandbox {
                                        Some(true)  => "on",
                                        Some(false) => "off",
                                        None        => "unknown",
                                    };

                                    // Build caps summary line.
                                    let caps_summary = if let Some(ref c) = caps {
                                        let arch = c["arch"].as_str().unwrap_or("?");
                                        let os   = c["os"].as_str().unwrap_or("?");
                                        let cpu  = c["cpu_cores"].as_u64().unwrap_or(0);
                                        let ram  = c["ram_gb"].as_u64().unwrap_or(0);
                                        let gpus_arr = c["gpus"].as_array().map(|a| a.len()).unwrap_or(0);
                                        let gpu_str = if gpus_arr > 0 {
                                            let names: Vec<&str> = c["gpus"].as_array().unwrap()
                                                .iter().filter_map(|g| g["name"].as_str()).collect();
                                            format!("  GPU: {}", names.join(", "))
                                        } else {
                                            String::new()
                                        };
                                        format!(
                                            "  Arch: {}/{}  CPU: {} cores  RAM: {} GiB{}",
                                            os, arch, cpu, ram, gpu_str
                                        )
                                    } else {
                                        String::new()
                                    };

                                    // Build virtual-nodes table.
                                    let vnode_table = if !virtual_nodes.is_empty() {
                                        let mut t = String::from("\n  Virtual nodes:\n");
                                        t.push_str(&format!(
                                            "  {:<22} {:<8} {}\n",
                                            "NAME", "SANDBOX", "WORKDIR / TAGS"
                                        ));
                                        t.push_str(&format!("  {}\n", "─".repeat(72)));
                                        for vn in &virtual_nodes {
                                            let sb_str = if vn.sandbox { "on " } else { "off" };
                                            let tags_str = if vn.tags.is_empty() {
                                                String::new()
                                            } else {
                                                format!("  [{}]", vn.tags.join(", "))
                                            };
                                            t.push_str(&format!(
                                                "  {:<22} {:<8} {}{}\n",
                                                vn.name, sb_str, vn.workdir, tags_str
                                            ));
                                        }
                                        t
                                    } else {
                                        String::new()
                                    };

                                    connect_header = format!(
                                        "[Node '{}' connected — workdir: {}  sandbox: {}]\n{}{}\n",
                                        input.target, wd, sb, caps_summary, vnode_table
                                    );

                                    // Update the in-process route table so future any:tag calls work.
                                    if !virtual_nodes.is_empty() {
                                        let raw_url = if url.contains('?') {
                                            url.splitn(2, '?').next().unwrap_or(&url).to_string()
                                        } else {
                                            url.clone()
                                        };
                                        workspaces::update_route_table(
                                            &input.target, &raw_url, &virtual_nodes,
                                        );
                                    }

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
