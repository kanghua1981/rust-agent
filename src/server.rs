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
pub async fn run(config: Config, host: &str, port: u16) -> Result<()> {
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;

    println!("🤖 Agent WebSocket server listening on ws://{}", addr);
    println!("   Provider: {:?}  Model: {}", config.provider, config.model);
    println!("   Press Ctrl+C to stop.\n");

    loop {
        let (stream, peer) = listener.accept().await?;
        let config = config.clone();

        tokio::spawn(async move {
            tracing::info!("New connection from {}", peer);
            if let Err(e) = handle_connection(stream, config).await {
                tracing::warn!("Connection {} ended with error: {}", peer, e);
            } else {
                tracing::info!("Connection {} closed", peer);
            }
        });
    }
}

/// Handle a single WebSocket connection.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    config: Config,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Channel for the Agent (WsOutput) to send frames to the writer task
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

    // Build WsOutput and get the confirm_tx for the reader side
    let ws_output = Arc::new(WsOutput::new(cmd_tx));
    let confirm_tx = ws_output.confirm_tx.clone();

    // Create the Agent with this connection's output
    let mut agent = Agent::new(config, ws_output.clone());

    // ── Writer task: forwards outgoing frames to the WebSocket ──
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

    // ── Reader loop: process incoming messages ──
    // Send a "ready" event so the client knows the connection is live
    ws_output.emit_public("ready", serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
    }));

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
                let text_str: &str = text.as_ref();
                handle_text_message(text_str, &mut agent, &ws_output, &confirm_tx).await;
            }
            Message::Close(_) => break,
            Message::Ping(_payload) => {
                // Respond with pong event
                ws_output.emit_public("pong", serde_json::json!({}));
            }
            _ => {} // ignore binary, pong, etc.
        }
    }

    // Clean up
    writer_handle.abort();
    Ok(())
}

/// Process a single text message from the client.
async fn handle_text_message(
    text: &str,
    agent: &mut Agent,
    output: &Arc<WsOutput>,
    confirm_tx: &std::sync::mpsc::Sender<bool>,
) {
    // Parse the JSON message
    let msg: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            output.emit_public("error", serde_json::json!({
                "message": format!("Invalid JSON: {}", e),
            }));
            return;
        }
    };

    let msg_type = msg
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match msg_type {
        "user_message" => {
            let user_text = msg
                .get("data")
                .and_then(|d| d.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if user_text.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "Empty user_message text",
                }));
                return;
            }

            // Extract optional request id for correlation
            let req_id = msg.get("id").cloned();

            // Process through the agent loop
            match agent.process_message(user_text).await {
                Ok(final_text) => {
                    let mut done_data = serde_json::json!({ "text": final_text });
                    if let Some(id) = req_id {
                        done_data["id"] = id;
                    }
                    output.emit_public("done", done_data);
                }
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("{:#}", e),
                    }));
                }
            }
        }

        "confirm_response" => {
            let approved = msg
                .get("data")
                .and_then(|d| d.get("approved"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // Forward to the WsOutput's confirm channel
            let _ = confirm_tx.send(approved);
        }

        other => {
            output.emit_public("error", serde_json::json!({
                "message": format!("Unknown message type: '{}'", other),
            }));
        }
    }
}
