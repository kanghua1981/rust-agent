//! Spawn a sub-agent as a stdio child process.
//!
//! Unlike `call_sub_agent` (which requires a pre-running WebSocket server),
//! this tool spawns a fresh `agent --mode stdio --yes` process on demand,
//! feeds it the task via stdin, streams its JSON output back to the parent's
//! `AgentOutput` with a `[sub:{id}]` prefix, and waits for it to finish.
//!
//! The child process exits naturally when it completes its single-prompt
//! session, so there is nothing to clean up.
//!
//! # Protocol
//!
//! The child is launched with `-p "<prompt>"` so it executes immediately
//! without needing an extra stdin write.  All output is JSON lines on stdout
//! (the existing `StdioOutput` format):
//!
//! ```json
//! {"type":"stream_start","data":{}}
//! {"type":"streaming_token","data":{"token":"..."}}
//! {"type":"tool_use","data":{"tool":"read_file","input":{...}}}
//! {"type":"tool_result","data":{"tool":"read_file","output":"..."}}
//! {"type":"stream_end","data":{}}
//! {"type":"done","data":{"text":"<final answer>"}}
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

use crate::agent::is_interrupted;
use crate::output::{AgentOutput, SubAgentOutputEvent};
use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Total wall-clock time a sub-agent is allowed to run.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub struct SpawnSubAgentTool {
    output: Arc<dyn AgentOutput>,
}

impl SpawnSubAgentTool {
    pub fn new(output: Arc<dyn AgentOutput>) -> Self {
        SpawnSubAgentTool { output }
    }
}

#[derive(Serialize, Deserialize)]
struct SpawnSubAgentInput {
    /// The task description to hand to the sub-agent.
    prompt: String,
    /// Optional working directory for the sub-agent.  When set the child is
    /// launched with `--workdir <path>` so its file tools are scoped there.
    #[serde(default)]
    target_dir: Option<String>,
    /// When true all sub-agent tool confirmations are auto-approved (--yes).
    /// Defaults to true since we're spawning a non-interactive process.
    #[serde(default = "default_auto_approve")]
    auto_approve: bool,
    /// Maximum seconds to wait for the sub-agent to finish (default 300).
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_auto_approve() -> bool { true }
fn default_timeout() -> u64 { DEFAULT_TIMEOUT_SECS }

#[async_trait]
impl Tool for SpawnSubAgentTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_sub_agent".to_string(),
            description: "Spawns a sub-agent as an isolated stdio child process to handle a \
                          specific sub-task.  The child runs `agent --mode stdio` and exits \
                          automatically when done — no server required.  All output is streamed \
                          back with a [sub:id] prefix so it is visually distinct from the main \
                          agent's output.  Use this instead of call_sub_agent when you do not \
                          have a pre-running agent server."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Full task description for the sub-agent."
                    },
                    "target_dir": {
                        "type": "string",
                        "description": "Optional directory to scope the sub-agent's work.  \
                                        Passed as --workdir so file tools are limited to it."
                    },
                    "auto_approve": {
                        "type": "boolean",
                        "description": "Auto-approve all sub-agent tool calls (default true).",
                        "default": true
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Maximum seconds to wait before aborting (default 300).",
                        "default": 300
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let params: SpawnSubAgentInput = match serde_json::from_value(input.clone()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // ── Locate the agent binary ───────────────────────────────────────────
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!(
                "Cannot locate agent binary: {}", e
            )),
        };

        // ── Build command ─────────────────────────────────────────────────────
        let workdir = params.target_dir
            .as_deref()
            .map(|d| {
                let p = std::path::Path::new(d);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    project_dir.join(p)
                }
            })
            .unwrap_or_else(|| project_dir.to_path_buf());

        let mut cmd = Command::new(&exe);
        cmd.arg("--mode").arg("stdio")
           .arg("--workdir").arg(&workdir)
           .arg("--prompt").arg(&params.prompt);

        if params.auto_approve || crate::confirm::is_auto_approve() {
            cmd.arg("--yes");
        }

        // Set AGENT_ROLE=worker so the child doesn't register `spawn_sub_agent`
        // itself (prevents infinite recursion).
        cmd.env("AGENT_ROLE", "worker");

        // Pipe stdin/stdout; let stderr inherit so any panics are visible.
        cmd.stdin(std::process::Stdio::null())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::inherit());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!(
                "Failed to spawn sub-agent: {}", e
            )),
        };

        // ── Generate short task ID ────────────────────────────────────────────
        let task_id = Uuid::new_v4().to_string()[..4].to_string();

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => return ToolResult::error("Sub-agent stdout unavailable"),
        };

        self.output.on_sub_agent_event(&task_id, &SubAgentOutputEvent::StreamStart);

        // ── Stream events ─────────────────────────────────────────────────────
        let mut reader = BufReader::new(stdout).lines();
        let mut final_answer = String::new();
        let mut error_msg: Option<String> = None;

        let run_result = timeout(Duration::from_secs(params.timeout_secs), async {
            loop {
                if is_interrupted() {
                    return Err("interrupted".to_string());
                }

                match reader.next_line().await {
                    Ok(Some(line)) if !line.trim().is_empty() => {
                        let ev: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => continue, // skip non-JSON lines (e.g. tracing output)
                        };
                        dispatch_event(&ev, &task_id, &self.output, &mut final_answer);
                        if is_terminal_event(&ev) {
                            break;
                        }
                    }
                    Ok(Some(_)) => {} // blank line
                    Ok(None) => break, // EOF
                    Err(e) => {
                        return Err(format!("Read error: {}", e));
                    }
                }
            }
            Ok(())
        })
        .await;

        // ── Cleanup ───────────────────────────────────────────────────────────
        match run_result {
            Err(_timeout) => {
                child.kill().await.ok();
                self.output.on_sub_agent_event(&task_id, &SubAgentOutputEvent::Error(
                    format!("Timed out after {}s", params.timeout_secs)
                ));
                return ToolResult::error(format!(
                    "Sub-agent timed out after {}s", params.timeout_secs
                ));
            }
            Ok(Err(msg)) if msg == "interrupted" => {
                child.kill().await.ok();
                self.output.on_sub_agent_event(&task_id, &SubAgentOutputEvent::Error(
                    "Interrupted by user (Ctrl-C)".to_string()
                ));
                return ToolResult::error("Sub-agent interrupted".to_string());
            }
            Ok(Err(msg)) => {
                error_msg = Some(msg);
            }
            Ok(Ok(())) => {}
        }

        // Wait for the child to fully exit (should be quick at this point).
        child.wait().await.ok();

        if let Some(err) = error_msg {
            self.output.on_sub_agent_event(&task_id, &SubAgentOutputEvent::Error(err.clone()));
            return ToolResult::error(format!("Sub-agent error: {}", err));
        }

        self.output.on_sub_agent_event(&task_id, &SubAgentOutputEvent::Done(final_answer.clone()));

        ToolResult::success(if final_answer.is_empty() {
            "Sub-agent completed (no output).".to_string()
        } else {
            format!("Sub-agent [{}] completed:\n{}", task_id, final_answer)
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Dispatch a parsed JSON event line to the parent's output.
fn dispatch_event(ev: &Value, task_id: &str, output: &Arc<dyn AgentOutput>, final_answer: &mut String) {
    match ev["type"].as_str() {
        Some("stream_start") => {
            output.on_sub_agent_event(task_id, &SubAgentOutputEvent::StreamStart);
        }
        Some("stream_end") => {
            output.on_sub_agent_event(task_id, &SubAgentOutputEvent::StreamEnd);
        }
        Some("streaming_token") => {
            let token = ev["data"]["token"].as_str().unwrap_or("").to_string();
            if !token.is_empty() {
                final_answer.push_str(&token);
                output.on_sub_agent_event(task_id, &SubAgentOutputEvent::Token(token));
            }
        }
        Some("assistant_text") => {
            let text = ev["data"]["text"].as_str().unwrap_or("").to_string();
            if !text.is_empty() {
                *final_answer = text.clone();
                output.on_sub_agent_event(task_id, &SubAgentOutputEvent::Token(text));
            }
        }
        Some("tool_use") => {
            let name = ev["data"]["tool"].as_str()
                .or_else(|| ev["data"]["name"].as_str())
                .unwrap_or("unknown")
                .to_string();
            output.on_sub_agent_event(task_id, &SubAgentOutputEvent::ToolUse { name });
        }
        Some("tool_result") => {
            let name = ev["data"]["tool"].as_str()
                .or_else(|| ev["data"]["name"].as_str())
                .unwrap_or("unknown")
                .to_string();
            let is_error = ev["data"]["is_error"].as_bool().unwrap_or(false);
            output.on_sub_agent_event(task_id, &SubAgentOutputEvent::ToolDone { name, is_error });
        }
        Some("done") | Some("final_response") => {
            let text = ev["data"]["text"].as_str().unwrap_or("").to_string();
            if !text.is_empty() {
                *final_answer = text;
            }
        }
        Some("error") => {
            let msg = ev["data"]["message"].as_str().unwrap_or("unknown error").to_string();
            output.on_sub_agent_event(task_id, &SubAgentOutputEvent::Error(msg));
        }
        _ => {}
    }
}

/// Returns true when the event signals the sub-agent has finished.
fn is_terminal_event(ev: &Value) -> bool {
    matches!(ev["type"].as_str(), Some("done") | Some("final_response") | Some("error"))
}
