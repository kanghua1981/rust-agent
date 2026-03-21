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

/// Control commands sent to the agent loop outside of normal user messages.
enum ControlCmd {
    LoadSession,
    NewSession,
}

/// Build a `session_info` JSON payload from the local `.agent/session.json`.
fn session_info_json(workdir: &std::path::Path) -> serde_json::Value {
    match crate::persistence::load_local_session(workdir) {
        Ok(Some(session)) => serde_json::json!({
            "exists": true,
            "message_count": session.meta.message_count,
            "updated_at": session.meta.updated_at,
            "summary": session.meta.summary,
            "working_dir": session.meta.working_dir,
        }),
        _ => serde_json::json!({ "exists": false }),
    }
}

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

    // Shared working directory: updated by `set_workdir` messages between turns.
    let shared_workdir: Arc<std::sync::Mutex<Option<PathBuf>>> = Arc::new(std::sync::Mutex::new(None));
    let shared_workdir_reader = shared_workdir.clone();

    // Shared execution mode: None = auto (router decides), or forced mode.
    type SharedMode = Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>>;
    let shared_mode: SharedMode = Arc::new(std::sync::Mutex::new(None));
    let shared_mode_reader = shared_mode.clone();

    // Control channel: load_session and future control commands that bypass
    // the serialised user_message queue.
    let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<ControlCmd>();
    let ctrl_tx_reader = ctrl_tx;  // moves into reader task

    // Channel carrying (user_text, request_id, workdir) from reader → agent loop.
    // workdir is taken from user_message.data.workdir (overrides shared_workdir for that turn).
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
                        &shared_workdir_reader,
                        &shared_mode_reader,
                        &ctrl_tx_reader,
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

    // Send a "ready" event so the client knows the connection is live, plus
    // any existing local session info so the UI can show a "restore" option.
    ws_output.emit_public("ready", serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
    }));
    ws_output.emit_public("session_info", session_info_json(&project_dir));

    // ── Agent loop ───────────────────────────────────────────────────────────
    // Uses tokio::select! so control commands (LoadSession) are handled even
    // while the agent is idle between user messages.
    loop {
        tokio::select! {
            // ── Control commands (load_session, etc.) ──────────────────────
            Some(ctrl) = ctrl_rx.recv() => {
                match ctrl {
                    ControlCmd::LoadSession => {
                        match crate::persistence::load_local_session(&agent.project_dir) {
                            Ok(Some(session)) => {
                                let history: Vec<serde_json::Value> = session.messages.iter()
                                    .filter_map(|m| {
                                        let text = m.text_content();
                                        if text.is_empty() { return None; }
                                        let role = match m.role {
                                            crate::conversation::Role::User      => "user",
                                            crate::conversation::Role::Assistant => "assistant",
                                            crate::conversation::Role::System    => "system",
                                        };
                                        Some(serde_json::json!({
                                            "id": m.id, "role": role, "content": text
                                        }))
                                    })
                                    .collect();
                                agent.conversation = crate::persistence::restore_conversation(&session);
                                ws_output.emit_public("session_restored", serde_json::json!({
                                    "message_count": history.len(),
                                    "messages": history,
                                }));
                            }
                            Ok(None) => {
                                ws_output.emit_public("warning", serde_json::json!({
                                    "message": "当前目录没有保存的会话",
                                }));
                            }
                            Err(e) => {
                                ws_output.emit_public("error", serde_json::json!({
                                    "message": format!("加载会话失败: {:#}", e),
                                }));
                            }
                        }
                    }
                    ControlCmd::NewSession => {
                        // Clear the current conversation and start fresh.
                        agent.conversation = crate::conversation::Conversation::new(&agent.project_dir);
                        ws_output.emit_public("session_cleared", serde_json::json!({
                            "message": "New session started"
                        }));
                        // Also emit updated session_info (should show no saved session)
                        let session_info = session_info_json(&agent.project_dir);
                        ws_output.emit_public("session_info", session_info);
                    }
                }
            }

            // ── User messages ──────────────────────────────────────────────
            msg = user_rx.recv() => {
                let (user_text, req_id, msg_workdir) = match msg {
                    Some(m) => m,
                    None => break, // channel closed → connection dropped
                };

                // If the message carries an explicit workdir, use it; otherwise fall back
                // to the last value set via `set_workdir`.
                let effective_workdir = msg_workdir.or_else(|| {
                    shared_workdir.lock().ok().and_then(|g| g.clone().map(|p| p.to_string_lossy().into_owned()))
                });
                let workdir_changed = if let Some(ref dir) = effective_workdir {
                    let p = PathBuf::from(dir);
                    if p.is_dir() {
                        let changed = agent.project_dir != p;
                        agent.project_dir = p.clone();
                        agent.set_allowed_dir(Some(p));
                        changed
                    } else { false }
                } else { false };

                // If workdir just changed, send fresh session_info for the new dir.
                if workdir_changed {
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }

                // Apply execution mode override (auto if None).
                let mode = shared_mode.lock().ok().and_then(|g| *g);
                agent.set_force_mode(mode);
                let process_result = agent.process_message(&user_text).await;
                agent.set_allowed_dir(None);

                match process_result {
                    Ok(final_text) => {
                        let mut done_data = serde_json::json!({ "text": final_text });
                        if let Some(id) = req_id { done_data["id"] = id; }
                        ws_output.emit_public("done", done_data);
                        // Save conversation and broadcast updated session info.
                        if let Err(e) = crate::persistence::save_local_session(
                            &agent.conversation, &agent.project_dir
                        ) {
                            tracing::warn!("Failed to save local session: {}", e);
                        }
                        ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                    }
                    Err(e) => {
                        ws_output.emit_public("error", serde_json::json!({
                            "message": format!("{:#}", e),
                        }));
                    }
                }
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
    shared_workdir: &std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
    shared_mode: &std::sync::Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>>,
    ctrl_tx: &mpsc::UnboundedSender<ControlCmd>,
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
            // Prefer workdir from data.workdir (sent by web client), fall back to
            // legacy top-level allowed_dir field.
            let workdir = msg
                .get("data").and_then(|d| d.get("workdir")).and_then(|v| v.as_str())
                .or_else(|| msg.get("allowed_dir").and_then(|v| v.as_str()))
                .map(|s| s.to_string());

            // try_send: non-blocking; fails if agent is still processing the
            // previous message (channel capacity = 1).
            if user_tx.try_send((user_text, req_id, workdir)).is_err() {
                output.emit_public("error", serde_json::json!({
                    "message": "Agent is busy processing a previous request",
                }));
            }
        }

        "set_workdir" => {
            // Update the shared working directory for subsequent turns.
            if let Some(dir) = msg.get("data").and_then(|d| d.get("workdir")).and_then(|v| v.as_str()) {
                let p = PathBuf::from(dir);
                if p.is_dir() {
                    if let Ok(mut guard) = shared_workdir.lock() {
                        *guard = Some(p);
                    }
                    tracing::info!("Working directory updated to: {}", dir);
                } else {
                    output.emit_public("warning", serde_json::json!({
                        "message": format!("set_workdir: '{}' is not a valid directory", dir),
                    }));
                }
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

        // set_model is informational — silently acknowledge.
        "set_model" => {}

        "load_session" => {
            let _ = ctrl_tx.send(ControlCmd::LoadSession);
        }

        "new_session" => {
            let _ = ctrl_tx.send(ControlCmd::NewSession);
        }

        "set_mode" => {
            use crate::router::ExecutionMode;
            let mode_str = msg
                .get("data").and_then(|d| d.get("mode")).and_then(|v| v.as_str())
                .unwrap_or("auto");
            let mode = match mode_str {
                "simple"   => Some(ExecutionMode::BasicLoop),
                "plan"     => Some(ExecutionMode::PlanAndExecute),
                "pipeline" => Some(ExecutionMode::FullPipeline),
                _          => None, // "auto"
            };
            if let Ok(mut guard) = shared_mode.lock() {
                *guard = mode;
            }
            tracing::info!("Execution mode set to: {}", mode_str);
        }

        other => {
            tracing::debug!("Ignoring unknown WebSocket message type: '{}'", other);
        }
    }
}
