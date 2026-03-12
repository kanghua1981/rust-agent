//! WebSocket server mode.
//!
//! Starts a WebSocket server on `host:port`. Each connection gets its
//! own `Agent` with a `WsOutput` backend. The client sends JSON
//! messages (user prompts, confirm responses) and receives a stream
//! of JSON event frames.
//!
//! ## Client → Server messages
//!
//! ```json
//! {"type": "user_message", "data": {"text": "帮我重构 main.rs"}}
//! {"type": "confirm_response", "data": {"approved": true}}
//! ```
//!
//! ## Server → Client messages
//!
//! Same event types as `--mode stdio`: `thinking`, `stream_start`,
//! `streaming_token`, `stream_end`, `tool_use`, `tool_result`,
//! `diff`, `confirm_request`, `warning`, `error`, `context_warning`,
//! plus `done` when a user_message has been fully processed.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::agent::Agent;
use crate::config::Config;
use crate::output::{WsCommand, WsOutput};

/// Start the WebSocket server and listen forever.
pub async fn run(config: Config, project_dir: PathBuf, host: &str, port: u16) -> Result<()> {
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;

    println!("🤖 Agent WebSocket server listening on ws://{}", addr);
    println!("   Provider: {:?}  Model: {}", config.provider, config.model);
    println!("   Press Ctrl+C to stop.\n");

    loop {
        let (stream, peer) = listener.accept().await?;
        let config = config.clone();
        let project_dir = project_dir.clone();

        tokio::spawn(async move {
            tracing::info!("New connection from {}", peer);
            if let Err(e) = handle_connection(stream, config, project_dir).await {
                tracing::warn!("Connection {} ended with error: {}", peer, e);
            } else {
                tracing::info!("Connection {} closed", peer);
            }
        });
    }
}

/// Handle a single WebSocket connection.
///
/// Architecture: three concurrent tasks to avoid deadlocks when the agent
/// blocks waiting for confirm/ask_user/review_plan responses:
///
/// 1. **Reader task** — permanently reads `ws_read`, dispatches every frame
///    to the right channel immediately (never blocks on agent work).
/// 2. **Writer task** — forwards WsOutput frames to `ws_write`.
/// 3. **Agent loop** (current task) — awaits `user_rx`, runs
///    `agent.process_message()`, repeats.
///
/// Because the reader task is always running, `confirm_response` /
/// `ask_user_response` / `review_plan_response` frames arrive at
/// `confirm_tx` / `ask_user_tx` even while the agent loop is blocked
/// inside `WsOutput::confirm()` / `ask_user()` / `review_plan()`.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    config: Config,
    project_dir: PathBuf,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Channel for the Agent (WsOutput) to send frames to the writer task
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

    // Build WsOutput — confirm_tx / ask_user_tx are wired to the reader task
    let ws_output = Arc::new(WsOutput::new(cmd_tx));
    let confirm_tx = ws_output.confirm_tx.clone();
    let ask_user_tx = ws_output.ask_user_tx.clone();

    // Create the Agent (sandbox disabled for server mode)
    let mut agent = Agent::new(
        config,
        project_dir.clone(),
        ws_output.clone(),
        crate::sandbox::Sandbox::disabled(&project_dir),
    );

    // ── Writer task ──────────────────────────────────────────────────────────
    let writer_handle = tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                WsCommand::Send(text) => {
                    if ws_write.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Channel carrying (user_text, request_id, allowed_dir) from reader → agent loop.
    // Capacity 1: the agent processes messages serially; a second message
    // while one is in-flight gets an "agent busy" error.
    let (user_tx, mut user_rx) = mpsc::channel::<(String, Option<serde_json::Value>, Option<String>)>(1);

    // ── Reader task ──────────────────────────────────────────────────────────
    // This task only dispatches — it never awaits the agent, so it can always
    // receive confirm/ask_user/review_plan responses while the agent is blocked.
    let ws_output_reader = ws_output.clone();
    let reader_handle = tokio::spawn(async move {
        while let Some(msg) = ws_read.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!("WebSocket read error: {}", e);
                    break;
                }
            };
            match msg {
                Message::Text(text) => {
                    dispatch_ws_message(
                        text.as_ref(),
                        &user_tx,
                        &confirm_tx,
                        &ask_user_tx,
                        &ws_output_reader,
                    );
                }
                Message::Close(_) => break,
                Message::Ping(_) => {
                    ws_output_reader.emit_public("pong", serde_json::json!({}));
                }
                _ => {}
            }
        }
        // Dropping user_tx signals the agent loop to exit cleanly.
    });

    // Send a "ready" event so the client knows the connection is live
    ws_output.emit_public("ready", serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
    }));

    // ── Agent loop ───────────────────────────────────────────────────────────
    // Processes one user_message at a time.  Runs on the current task so
    // `agent` (which is !Send) doesn't need to cross thread boundaries.
    while let Some((user_text, req_id, allowed_dir)) = user_rx.recv().await {
        // Apply directory restriction for this request, then clear it when done.
        agent.set_allowed_dir(allowed_dir.map(std::path::PathBuf::from));
        let process_result = agent.process_message(&user_text).await;
        agent.set_allowed_dir(None);
        match process_result {
            Ok(final_text) => {
                let mut done_data = serde_json::json!({ "text": final_text });
                if let Some(id) = req_id {
                    done_data["id"] = id;
                }
                ws_output.emit_public("done", done_data);
            }
            Err(e) => {
                ws_output.emit_public("error", serde_json::json!({
                    "message": format!("{:#}", e),
                }));
            }
        }
    }

    reader_handle.abort();
    writer_handle.abort();
    Ok(())
}

/// Synchronously dispatch one WebSocket text frame to the correct channel.
///
/// Called from the reader task — must never block or await.
fn dispatch_ws_message(
    text: &str,
    user_tx: &mpsc::Sender<(String, Option<serde_json::Value>, Option<String>)>,
    confirm_tx: &std::sync::mpsc::Sender<crate::confirm::ConfirmResult>,
    ask_user_tx: &std::sync::mpsc::Sender<String>,
    output: &Arc<WsOutput>,
) {
    let msg: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            output.emit_public("error", serde_json::json!({
                "message": format!("Invalid JSON: {}", e),
            }));
            return;
        }
    };

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "user_message" => {
            let user_text = msg
                .get("data")
                .and_then(|d| d.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if user_text.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "Empty user_message text",
                }));
                return;
            }

            let req_id = msg.get("id").cloned();
            let allowed_dir = msg
                .get("allowed_dir")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // try_send: non-blocking; fails if agent is still processing the
            // previous message (channel capacity = 1).
            if user_tx.try_send((user_text, req_id, allowed_dir)).is_err() {
                output.emit_public("error", serde_json::json!({
                    "message": "Agent is busy processing a previous request",
                }));
            }
        }

        "confirm_response" => {
            use crate::confirm::ConfirmResult;
            let data = msg.get("data");
            if let Some(clarify) = data.and_then(|d| d.get("clarify")).and_then(|v| v.as_str()) {
                let _ = confirm_tx.send(ConfirmResult::Clarify(clarify.to_string()));
            } else {
                let approved = data
                    .and_then(|d| d.get("approved"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let _ = confirm_tx.send(if approved { ConfirmResult::Yes } else { ConfirmResult::No });
            }
        }

        "ask_user_response" => {
            let answer = msg
                .get("data")
                .and_then(|d| d.get("answer"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let _ = ask_user_tx.send(answer);
        }

        "review_plan_response" => {
            // Forward raw JSON so WsOutput::review_plan can parse it
            let data = msg.get("data").cloned().unwrap_or(serde_json::json!({}));
            let _ = ask_user_tx.send(data.to_string());
        }

        other => {
            output.emit_public("error", serde_json::json!({
                "message": format!("Unknown message type: '{}'", other),
            }));
        }
    }
}
