//! MCP (Model Context Protocol) server mode — Direction A.
//!
//! When started with `--mode mcp`, this agent acts as a pure MCP **tool server**
//! over stdin/stdout JSON-RPC 2.0.  External hosts (Claude Desktop, Cursor, etc.)
//! can list and invoke all built-in tools (read_file, write_file, run_command, …)
//! without running any LLM on our side.
//!
//! Supported methods:
//!   initialize                   → handshake, return capabilities
//!   notifications/initialized    → acknowledged, no reply needed
//!   tools/list                   → return all ToolDefinitions in MCP format
//!   tools/call                   → execute a tool, return content blocks
//!
//! Transport: newline-delimited JSON over stdio (one JSON object per line).
//!
//! Config example (Claude Desktop claude_desktop_config.json):
//! ```json
//! {
//!   "mcpServers": {
//!     "rust-agent": {
//!       "command": "/path/to/agent",
//!       "args": ["--mode", "mcp", "--workdir", "/your/project"]
//!     }
//!   }
//! }
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::output::{PlanReview};
use crate::tools::{ToolExecutor, ToolResult};

// ── Null output: discards all display events ──────────────────────────────────
// In MCP server mode we're a pure tool executor — no LLM, no terminal output.

struct NullOutput;

impl crate::output::AgentOutput for NullOutput {
    fn on_thinking(&self) {}
    fn on_role_header(&self, _label: &str, _model: &str) {}
    fn on_stage_end(&self, _label: &str) {}
    fn on_assistant_text(&self, _text: &str) {}
    fn on_streaming_text(&self, _token: &str) {}
    fn on_stream_start(&self) {}
    fn on_stream_end(&self) {}
    fn on_tool_use(&self, _name: &str, _input: &Value) {}
    fn on_tool_result(&self, _name: &str, _result: &ToolResult) {}
    fn on_diff(&self, _path: &str, _old: &str, _new: &str) {}
    fn confirm(&self, _action: &crate::confirm::ConfirmAction) -> crate::confirm::ConfirmResult {
        crate::confirm::ConfirmResult::Yes // auto-approve in server mode
    }
    fn ask_user(&self, _question: &str) -> String {
        String::new()
    }
    fn review_plan(&self, _plan_text: &str) -> PlanReview {
        PlanReview::Approve
    }
    fn inject_guidance(&self) -> Option<String> {
        None
    }
    fn on_warning(&self, _msg: &str) {}
    fn on_error(&self, _msg: &str) {}
    fn on_context_warning(&self, _usage_percent: f32, _estimated: usize, _max: usize) {}
}

// ── Main server loop ──────────────────────────────────────────────────────────

/// Run the MCP JSON-RPC 2.0 server loop over stdin/stdout.
pub async fn run(project_dir: PathBuf) -> Result<()> {
    let output: Arc<dyn crate::output::AgentOutput> = Arc::new(NullOutput);
    let executor = ToolExecutor::new(project_dir, output);

    let mut reader = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // silently skip malformed input
        };

        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");

        if let Some(resp) = handle_message(method, &msg, id, &executor).await {
            stdout.write_all(resp.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

async fn handle_message(
    method: &str,
    msg: &Value,
    id: Option<Value>,
    executor: &ToolExecutor,
) -> Option<String> {
    match method {
        // ── Handshake ─────────────────────────────────────────────────────────
        "initialize" => Some(json_rpc_ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "rust-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),

        // Notifications need no response
        "notifications/initialized" => None,

        // ── Tool discovery ────────────────────────────────────────────────────
        "tools/list" => {
            let tools: Vec<Value> = executor
                .definitions()
                .into_iter()
                .map(|def| {
                    json!({
                        "name": def.name,
                        "description": def.description,
                        "inputSchema": def.parameters
                    })
                })
                .collect();

            Some(json_rpc_ok(id, json!({ "tools": tools })))
        }

        // ── Tool invocation ───────────────────────────────────────────────────
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or_default();
            let name = params["name"].as_str().unwrap_or("").to_string();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            let result = executor.execute(&name, &arguments).await;

            Some(json_rpc_ok(
                id,
                json!({
                    "content": [{ "type": "text", "text": result.output }],
                    "isError": result.is_error
                }),
            ))
        }

        // ── Unknown method ────────────────────────────────────────────────────
        _ => id.map(|id| json_rpc_err(id, -32601, "Method not found")),
    }
}

// ── JSON-RPC helpers ──────────────────────────────────────────────────────────

fn json_rpc_ok(id: Option<Value>, result: Value) -> String {
    let msg = match id {
        Some(id) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        None     => json!({ "jsonrpc": "2.0", "result": result }),
    };
    serde_json::to_string(&msg).unwrap()
}

fn json_rpc_err(id: Value, code: i32, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    }))
    .unwrap()
}
