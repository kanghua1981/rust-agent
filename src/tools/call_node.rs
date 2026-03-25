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
use crate::output::{AgentOutput, PlanReview};
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
    /// Override the node's default isolation mode ("normal" | "container" | "sandbox").
    #[serde(default)]
    isolation: Option<String>,
    /// Override the node's default execution mode.
    /// "simple" | "plan" | "pipeline" | "auto" (= node/router default).
    #[serde(default)]
    exec_mode: Option<String>,
    /// Auto-approve all tool confirmations from the remote agent.
    #[serde(default)]
    auto_approve: bool,
    /// Total timeout in seconds (default 600).
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 { 600 }

// ── URL builder ───────────────────────────────────────────────────────────────

/// Append `workdir`, `mode` (isolation) and `token` as URL query parameters.
fn build_url(base: &str, workdir: Option<&str>, isolation: Option<&str>, token: Option<&str>) -> String {
    let mut params: Vec<String> = Vec::new();

    if let Some(wd) = workdir {
        params.push(format!("workdir={}", url_encode(wd)));
    }
    // Only append mode when it is not the default (container).
    if let Some(mode) = isolation {
        if mode != "container" {
            params.push(format!("mode={}", mode));
        }
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
    s.bytes().flat_map(|b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            vec![b as char]
        } else {
            format!("%{:02X}", b).chars().collect()
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
    AskUser(String),
    ReviewPlan(String),
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
        Some("ask_user") => {
            let question = ev["data"]["question"].as_str().unwrap_or("").to_string();
            NodeEvent::AskUser(question)
        }
        Some("review_plan") => {
            let plan = ev["data"]["plan"].as_str().unwrap_or("").to_string();
            NodeEvent::ReviewPlan(plan)
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
                    "isolation": {
                        "type": "string",
                        "enum": ["normal", "container", "sandbox"],
                        "description": "Override the remote agent's isolation mode. \
                            If omitted, uses the node's configured default."
                    },
                    "exec_mode": {
                        "type": "string",
                        "enum": ["simple", "plan", "pipeline", "auto"],
                        "description": "Override the execution mode on the remote node. \
                            'simple' = basic loop (fast), 'plan' = plan+execute, \
                            'pipeline' = full Planner→Executor→Checker, \
                            'auto' = let the remote router decide (default)."
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

        // ── Block internal infrastructure endpoints ───────────────────────────
        // Endpoints like /probe, /nodes, /health are server-internal management
        // interfaces, not task endpoints. They must never be called by the LLM.
        // /probe is the server-to-server directory sync protocol; calling it from
        // a tool would bypass the NodeRegistry and cause undefined behaviour.
        if is_direct_url {
            let after_scheme = input.target
                .trim_start_matches("wss://")
                .trim_start_matches("ws://");
            // path starts after the first '/' that follows the host:port
            let path = after_scheme
                .splitn(2, '/')
                .nth(1)
                .map(|p| format!("/{}", p.split('?').next().unwrap_or("")))
                .unwrap_or_else(|| "/".to_string());
            let blocked: &[&str] = &["/probe", "/nodes", "/health", "/metrics"];
            if let Some(ep) = blocked.iter().find(|&&ep| path == ep || path.starts_with(&format!("{}/", ep))) {
                return ToolResult::error(format!(
                    "'{}' is a server-internal management endpoint and cannot be used as a task target.\n\
                     Use `list_nodes` to discover available agent nodes, then call with a node name or \
                     a plain ws://host:port URL.",
                    ep
                ));
            }
        }

        // ── Self-loop guard ───────────────────────────────────────────────────
        // When running as a worker (AGENT_PARENT_PORT is set), connecting back
        // to localhost:{AGENT_PARENT_PORT} with NO workdir (or the same workdir
        // as the current worker) creates an infinite fork chain.
        // Connecting to localhost:{AGENT_PARENT_PORT}?workdir=/other/path is
        // legitimate — it spawns a worker in a different [[workspace]].
        if is_direct_url {
            if let Ok(parent_port_str) = std::env::var("AGENT_PARENT_PORT") {
                let parent_port = parent_port_str.trim().to_string();
                let target = &input.target;
                let is_loopback = target.contains("localhost:") || target.contains("127.0.0.1:");
                if is_loopback {
                    // Extract port from the host:port portion.
                    let after_scheme = target
                        .trim_start_matches("wss://")
                        .trim_start_matches("ws://");
                    let host_port = after_scheme.split('/').next().unwrap_or("");
                    let port_in_url = host_port.split(':').nth(1).unwrap_or("");

                    if port_in_url == parent_port {
                        // Allow only if a workdir is explicitly set in the URL
                        // (connecting to a different [[workspace]] on the same server).
                        let has_workdir_in_url = target.contains("workdir=");
                        // Also allow if the call_node input has an explicit workdir override.
                        let has_workdir_input = input.workdir.is_some();

                        if !has_workdir_in_url && !has_workdir_input {
                            return ToolResult::error(format!(
                                "Self-loop detected: target '{}' points to the parent server \
                                 (localhost:{}) with no workdir specified.\n\
                                 This would spawn a worker with the same workspace, \
                                 causing an infinite delegation chain.\n\
                                 Use `list_nodes` to see available named nodes (including \
                                 local [[workspace]] entries), then call with the node name.",
                                target, parent_port
                            ));
                        }
                    }
                }
            }
        }

        let (base_url, node_workdir, node_isolation, node_exec_mode, cluster_token) = if is_direct_url {
            let tok = std::env::var("AGENT_CLUSTER_TOKEN").ok();
            (input.target.clone(), None::<String>, None::<String>, None::<String>, tok)
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
                    // Match local workspaces and dynamically-discovered route_table entries.
                    matches!(n["source"].as_str(), Some("local") | Some("route_table")) &&
                    n["tags"].as_array()
                        .map(|a| a.iter().any(|t| t.as_str() == Some(tag)))
                        .unwrap_or(false)
                });
                match found {
                    Some(n) => (
                        n["url"].as_str().unwrap_or("").to_string(),
                        n["workdir"].as_str().map(|s| s.to_string()),
                        n["isolation"].as_str().map(|s| s.to_string())
                            .or_else(|| n["sandbox"].as_bool().and_then(|b| if b { Some("sandbox".to_string()) } else { None })),
                        n["exec_mode"].as_str().map(|s| s.to_string()),
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
                    first["isolation"].as_str().map(|s| s.to_string())
                        .or_else(|| first["sandbox"].as_bool().and_then(|b| if b { Some("sandbox".to_string()) } else { None })),
                    first["exec_mode"].as_str().map(|s| s.to_string()),
                    cluster_token,
                )
            } else {
                // Named target — look up in entries returned by parent's /nodes.
                let found = nodes.iter().find(|n| n["name"].as_str() == Some(input.target.as_str()));
                match found {
                    Some(n) if n["offline"].as_bool().unwrap_or(false) => {
                        // Node is known but currently offline — trigger on-demand re-probe.
                        let peer_name = n["peer_name"].as_str().unwrap_or("").to_string();
                        if !peer_name.is_empty() {
                            self.output.on_warning(&format!(
                                "[call_node] node '{}' is offline — re-probing peer '{}'...",
                                input.target, peer_name
                            ));
                            let reprobe_url = match &cluster_token {
                                Some(tok) => format!(
                                    "http://localhost:{}/reprobe?peer={}&token={}",
                                    parent_port, url_encode(&peer_name), url_encode(tok)
                                ),
                                None => format!(
                                    "http://localhost:{}/reprobe?peer={}",
                                    parent_port, url_encode(&peer_name)
                                ),
                            };
                            // Wait for re-probe to complete (server probes synchronously in /reprobe).
                            let refreshed: Vec<serde_json::Value> = match reqwest::Client::new()
                                .get(&reprobe_url)
                                .timeout(Duration::from_secs(10))
                                .send().await
                            {
                                Ok(resp) => resp.json::<serde_json::Value>().await
                                    .ok()
                                    .and_then(|v| v["nodes"].as_array().cloned())
                                    .unwrap_or_default(),
                                Err(_) => vec![],
                            };
                            // Re-check the node in the updated list.
                            let refreshed_node = refreshed.iter()
                                .find(|n| n["name"].as_str() == Some(input.target.as_str()));
                            match refreshed_node {
                                Some(n) if !n["offline"].as_bool().unwrap_or(false) => (
                                    n["url"].as_str().unwrap_or("").to_string(),
                                    n["workdir"].as_str().map(|s| s.to_string()),
                                    n["isolation"].as_str().map(|s| s.to_string())
                                        .or_else(|| n["sandbox"].as_bool().and_then(|b| if b { Some("sandbox".to_string()) } else { None })),
                                    n["exec_mode"].as_str().map(|s| s.to_string()),
                                    cluster_token,
                                ),
                                _ => return ToolResult::error(format!(
                                    "Node '{}' (peer '{}') is offline and re-probe failed.\n\
                                     The remote server may be down. Try again later.",
                                    input.target, peer_name
                                )),
                            }
                        } else {
                            return ToolResult::error(format!(
                                "Node '{}' is offline.",
                                input.target
                            ));
                        }
                    }
                    Some(n) => (
                        n["url"].as_str().unwrap_or("").to_string(),
                        n["workdir"].as_str().map(|s| s.to_string()),
                        n["isolation"].as_str().map(|s| s.to_string())
                            .or_else(|| n["sandbox"].as_bool().and_then(|b| if b { Some("sandbox".to_string()) } else { None })),
                        n["exec_mode"].as_str().map(|s| s.to_string()),
                        cluster_token,
                    ),
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
        let effective_workdir   = input.workdir.as_deref().or(node_workdir.as_deref());
        let effective_isolation = input.isolation.as_deref().or(node_isolation.as_deref());
        // exec_mode: "auto" (or None) = let router decide; anything else is forced.
        let effective_exec_mode = input.exec_mode.as_deref()
            .filter(|m| *m != "auto")
            .or_else(|| node_exec_mode.as_deref().filter(|m| *m != "auto"));

        // ── Build final URL ───────────────────────────────────────────────────
        // Ensure the URL targets /agent (not the root path).
        let base_url = crate::workspaces::with_path(&base_url, "/agent");
        let url = build_url(&base_url, effective_workdir, effective_isolation, cluster_token.as_deref());

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
        // Force execution mode on the remote node BEFORE the user message so the
        // agent picks it up when processing this turn.
        if let Some(mode) = effective_exec_mode {
            let mode_msg = json!({ "type": "set_mode", "data": { "mode": mode } });
            let _ = write.send(Message::Text(mode_msg.to_string().into())).await;
        }
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
                                NodeEvent::AskUser(question) => {
                                    let output = self.output.clone();
                                    let answer = tokio::task::spawn_blocking(move || {
                                        output.ask_user(&question)
                                    }).await.unwrap_or_default();
                                    let response = json!({
                                        "type": "ask_user_response",
                                        "data": { "answer": answer }
                                    });
                                    let _ = write.send(Message::Text(response.to_string().into())).await;
                                }
                                NodeEvent::ReviewPlan(plan) => {
                                    let output = self.output.clone();
                                    let review = tokio::task::spawn_blocking(move || {
                                        output.review_plan(&plan)
                                    }).await.unwrap_or(PlanReview::Reject);
                                    let (approved, feedback, context) = match review {
                                        PlanReview::Approve              => (true,  String::new(), String::new()),
                                        PlanReview::ApproveWithContext(c) => (true,  String::new(), c),
                                        PlanReview::Reject               => (false, String::new(), String::new()),
                                        PlanReview::Refine(fb)           => (true,  fb,            String::new()),
                                    };
                                    let mut data = json!({ "approved": approved, "feedback": feedback });
                                    if !context.is_empty() {
                                        data["context"] = json!(context);
                                    }
                                    let response = json!({ "type": "review_plan_response", "data": data });
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
                                    // Translate legacy bool to isolation string for display.
                                    let sb = match sandbox {
                                        Some(true)  => "sandbox",
                                        Some(false) => "normal",
                                        None        => "container",
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
                                            "  {:<22} {:<12} {}\n",
                                            "NAME", "ISOLATION", "WORKDIR / TAGS"
                                        ));
                                        t.push_str(&format!("  {}\n", "─".repeat(72)));
                                        for vn in &virtual_nodes {
                                            let sb_str = match vn.isolation.as_deref() {
                                                Some(m) => m,
                                                None => if vn.sandbox { "sandbox" } else { "container" },
                                            };
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
                                        "[Node '{}' connected — workdir: {}  isolation: {}]\n{}{}\n",
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
