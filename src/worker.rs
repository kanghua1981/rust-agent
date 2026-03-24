//! Worker mode: one process per WebSocket connection.
//!
//! Spawned by `server::run()` with a pre-accepted TCP socket fd.
//! If sandbox is enabled the worker mounts fuse-overlayfs BEFORE
//! creating the tokio runtime (required: `unshare` must be called
//! single-threaded).  After the WebSocket connection closes the worker
//! process exits, automatically cleaning up the overlay mount.
//!
//! ## Invocation (internal, not user-facing)
//!
//! ```text
//! agent --mode worker
//!       --worker-fd   <raw_fd>
//!       --worker-id   <8-char uuid prefix>
//!       -d            <project_dir>
//!       [--sandbox]
//!       [--bind host_path:mount_path[:ro]] ...
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::agent::Agent;
use crate::config::Config;
use crate::output::{WsCommand, WsOutput};
use crate::sandbox::Sandbox;

// ═══════════════════════════════════════════════════════════════════
//  Extra bind-mount descriptor
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct BindMount {
    pub host: PathBuf,
    pub target: PathBuf,
    pub readonly: bool,
}

impl std::str::FromStr for BindMount {
    type Err = String;
    /// Parse "host_path:target_path[:ro]"
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() < 2 {
            return Err(format!("expected host:target[:ro], got '{}'", s));
        }
        Ok(BindMount {
            host: PathBuf::from(parts[0]),
            target: PathBuf::from(parts[1]),
            readonly: parts.get(2).map(|s| *s == "ro").unwrap_or(false),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Entry point  (called BEFORE tokio runtime exists)
// ═══════════════════════════════════════════════════════════════════

/// Run the worker.  Called synchronously from main() for `--mode worker`.
///
/// If `sandbox` is true this function sets up fuse-overlayfs while still
/// single-threaded, then creates the tokio runtime and runs the async loop.
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    fd: i32,
    sandbox_enabled: bool,
    _worker_id: &str,
    _extra_binds: Vec<BindMount>,
) -> Result<()> {
    run_async(config, project_dir, sandbox_enabled, fd).await
}

// ═══════════════════════════════════════════════════════════════════
//  Async agent loop
// ═══════════════════════════════════════════════════════════════════

async fn run_async(
    config: Config,
    project_dir: PathBuf,
    sandbox_enabled: bool,
    fd: i32,
) -> Result<()> {
    // Reconstruct TcpStream from the raw fd inherited from the server process.
    let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
    std_stream.set_nonblocking(true)?;
    let tcp_stream = tokio::net::TcpStream::from_std(std_stream)?;

    let ws_stream = tokio_tungstenite::accept_async(tcp_stream).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();
    let ws_output = Arc::new(WsOutput::new(cmd_tx));
    let confirm_tx  = ws_output.confirm_tx.clone();
    let ask_user_tx = ws_output.ask_user_tx.clone();

    let mut agent = Agent::new(
        config,
        project_dir.clone(),
        ws_output.clone(),
        Sandbox::disabled(&project_dir),
    );

    // Apply sandbox flag from server startup (--sandbox) or from initial
    // client set_sandbox message.
    //
    // Two paths:
    //  1. Container mode (server mode + sandbox): container.rs already mounted
    //     a kernel overlayfs before exec():
    //       lower  = /workspace-ro  (real project, read-only view)
    //       upper  = /tmp/overlay/upper  (writes land here, discarded on exit)
    //       merged = /workspace  (what tools should see)
    //     Detect this by checking for /workspace-ro; use from_overlay_dirs
    //     to tell the Sandbox struct about the pre-mounted dirs — no fuse-overlayfs
    //     subprocess needed.
    //  2. CLI / non-container mode: use set_sandbox_enabled() which tries
    //     fuse-overlayfs and falls back to snapshot.
    if sandbox_enabled {
        let workspace_ro = std::path::Path::new("/workspace-ro");
        if workspace_ro.exists() {
            // Container kernel overlay already mounted — wire it up directly.
            agent.sandbox = Sandbox::from_overlay_dirs(
                workspace_ro,
                std::path::Path::new("/tmp/overlay/upper"),
                std::path::Path::new("/tmp/overlay/work"),
                std::path::Path::new("/workspace"),
            );
            tracing::info!("Sandbox: using pre-mounted container kernel overlay");
        } else {
            agent.set_sandbox_enabled(true);
        }
        // If sandbox was requested but ended up disabled (fuse-overlayfs unavailable),
        // emit a warning to the frontend before the session starts so the user knows
        // they are NOT operating in isolation.
        if agent.sandbox.is_disabled {
            ws_output.emit_public("warning", serde_json::json!({
                "message": "⚠️  沙盒模式请求失败：fuse-overlayfs 不可用，沙盒已禁用。所有文件操作将直接作用于真实项目目录！请安装 fuse-overlayfs 后重新连接。"
            }));
        }
    }

    // ── Writer task ──────────────────────────────────────────────────────
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

    // Shared state (set by WS messages between turns)
    let shared_workdir: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let shared_workdir_reader = shared_workdir.clone();
    let shared_mode: Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>> =
        Arc::new(std::sync::Mutex::new(None));
    let shared_mode_reader = shared_mode.clone();

    let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<ControlCmd>();
    let ctrl_tx_reader = ctrl_tx.clone();

    // Capacity 1: agent processes messages serially
    let (user_tx, mut user_rx) =
        mpsc::channel::<(String, Option<serde_json::Value>, Option<String>)>(1);
    let ws_output_reader = ws_output.clone();

    // ── Reader task ──────────────────────────────────────────────────────
    let reader_handle = tokio::spawn(async move {
        while let Some(msg) = ws_read.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => { tracing::debug!("WS read error: {}", e); break; }
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
        // Dropping user_tx signals the agent loop to exit.
    });

    // Send ready event — include workdir and sandbox so remote managers (call_node)
    // can tell the LLM where they are and what mode they're running in.
    ws_output.emit_public("ready", serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "workdir": project_dir.display().to_string(),
        "sandbox": sandbox_enabled,
        "sandbox_backend": agent.sandbox.backend_label_sync(),
    }));
    ws_output.emit_public("session_info", session_info_json(&project_dir));
    // Always emit sandbox_status so the frontend reflects the actual state
    // (e.g. overlay enabled via URL param sandbox=1, which the client may not
    // know about until it receives this event).
    ws_output.emit_public("sandbox_status", serde_json::json!({
        "enabled": !agent.sandbox.is_disabled,
        "backend": agent.sandbox.backend_label_sync(),
        "pending_changes": 0,
    }));

    // ── Agent loop ───────────────────────────────────────────────────────
    loop {
        tokio::select! {
            Some(ctrl) = ctrl_rx.recv() => {
                handle_control_cmd(ctrl, &mut agent, &ws_output).await;
            }

            msg = user_rx.recv() => {
                let (user_text, req_id, msg_workdir) = match msg {
                    Some(m) => m,
                    None => break,
                };

                // Resolve effective workdir
                let effective_workdir = msg_workdir.or_else(|| {
                    shared_workdir.lock().ok()
                        .and_then(|g| g.clone().map(|p| p.to_string_lossy().into_owned()))
                });

                let workdir_changed = if let Some(ref dir) = effective_workdir {
                    let p = PathBuf::from(dir);
                    if p.is_dir() {
                        let changed = agent.project_dir != p;
                        agent.set_project_dir(p.clone());
                        agent.set_allowed_dir(Some(p));
                        changed
                    } else { false }
                } else { false };

                if workdir_changed {
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }

                let mode = shared_mode.lock().ok().and_then(|g| *g);
                agent.set_force_mode(mode);

                ws_output.emit_public("sandbox_status", serde_json::json!({
                    "enabled": !agent.sandbox.is_disabled,
                    "backend": agent.sandbox.backend_label_sync(),
                }));

                crate::agent::clear_interrupt();
                let process_result = agent.process_message(&user_text).await;
                agent.set_allowed_dir(None);

                match process_result {
                    Ok(final_text) => {
                        let pending = agent.sandbox.ops_count().await;
                        let mut done = serde_json::json!({ "text": final_text, "pending_changes": pending });
                        if let Some(id) = req_id { done["id"] = id; }
                        ws_output.emit_public("done", done);

                        // Notify frontend of updated sandbox state after every turn.
                        if !agent.sandbox.is_disabled {
                            ws_output.emit_public("sandbox_status", serde_json::json!({
                                "enabled": true,
                                "backend": agent.sandbox.backend_label_sync(),
                                "pending_changes": pending,
                            }));
                        }

                        if let Err(e) = crate::persistence::save_local_session(
                            &agent.conversation, &agent.project_dir,
                        ) { tracing::warn!("save_local_session: {}", e); }

                        if let Err(e) = crate::persistence::save_session_for_workdir(
                            &agent.conversation, &agent.project_dir,
                        ) { tracing::warn!("save_session_for_workdir: {}", e); }

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

    // ── Cleanup sandbox (unmount overlay if active) ───────────────────────
    agent.sandbox.cleanup().await;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
//  Control commands
// ═══════════════════════════════════════════════════════════════════

enum ControlCmd {
    LoadSession,
    NewSession,
    LoadSessionById(String),
    /// Sandbox toggle: in worker mode sandbox is fixed at startup.
    /// We respond with the current status and optionally a warning.
    SetSandbox(bool),
    SandboxListChanges,
    SandboxCommit,
    SandboxCommitFile(String),
    SandboxRollback,
}

async fn handle_control_cmd(
    ctrl: ControlCmd,
    agent: &mut Agent,
    ws_output: &Arc<WsOutput>,
) {
    match ctrl {
        ControlCmd::LoadSession => {
            match crate::persistence::load_local_session(&agent.project_dir) {
                Ok(Some(session)) => {
                    let history = messages_to_json(&session.messages);
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
            agent.conversation = crate::conversation::Conversation::new(&agent.project_dir);
            ws_output.emit_public("session_cleared", serde_json::json!({ "message": "New session started" }));
            ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
        }

        ControlCmd::LoadSessionById(id) => {
            match crate::persistence::load_session(&id) {
                Ok(session) => {
                    let new_dir = std::path::PathBuf::from(&session.meta.working_dir);
                    if new_dir.is_dir() {
                        agent.set_project_dir(new_dir.clone());
                        agent.set_allowed_dir(Some(new_dir));
                        ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                    }
                    let history = messages_to_json(&session.messages);
                    agent.conversation = crate::persistence::restore_conversation(&session);
                    ws_output.emit_public("session_restored", serde_json::json!({
                        "message_count": history.len(),
                        "messages": history,
                    }));
                }
                Err(e) => {
                    ws_output.emit_public("error", serde_json::json!({
                        "message": format!("加载会话失败: {:#}", e),
                    }));
                }
            }
        }

        ControlCmd::SetSandbox(enabled) => {
            // In container mode, sandbox is wired at startup via kernel overlay.
            // Dynamic toggle via set_sandbox message is only effective in CLI mode.
            // In container mode, if we already have the correct state, just report it.
            let already_correct = (enabled && !agent.sandbox.is_disabled)
                || (!enabled && agent.sandbox.is_disabled);
            if !already_correct {
                agent.set_sandbox_enabled(enabled);
            }
            let actual_enabled = !agent.sandbox.is_disabled;
            if enabled && !actual_enabled {
                ws_output.emit_public("warning", serde_json::json!({
                    "message": "沙盒模式需要在连接时通过 URL 参数 sandbox=1 启用（容器需要在启动前挂载 overlay）。请断开重连并在连接面板中勾选沙盒选项。"
                }));
            }
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": actual_enabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": agent.sandbox.ops_count().await,
            }));
        }

        ControlCmd::SandboxListChanges => {
            let changes = agent.sandbox.changed_files().await;
            let files: Vec<serde_json::Value> = changes.iter().map(|c| c.to_json()).collect();
            ws_output.emit_public("sandbox_changes_result", serde_json::json!({
                "files": files,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": changes.len(),
            }));
        }

        ControlCmd::SandboxCommit => {
            let result = agent.sandbox.commit().await;
            ws_output.emit_public("sandbox_commit_result", serde_json::json!({
                "modified": result.modified,
                "created": result.created,
            }));
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": 0,
            }));
        }
        
        ControlCmd::SandboxCommitFile(file_path) => {
            let result = agent.sandbox.commit_file(&file_path).await;
            ws_output.emit_public("sandbox_commit_file_result", serde_json::json!({
                "file_path": file_path,
                "modified": result.modified,
                "created": result.created,
            }));
            // Update pending changes count
            let changes = agent.sandbox.changed_files().await;
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": changes.len(),
            }));
        }

        ControlCmd::SandboxRollback => {
            let result = agent.sandbox.rollback().await;
            ws_output.emit_public("sandbox_rollback_result", serde_json::json!({
                "restored": result.restored,
                "deleted": result.deleted,
                "errors": result.errors,
            }));
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": 0,
            }));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  WebSocket message dispatcher
// ═══════════════════════════════════════════════════════════════════

fn dispatch_ws_message(
    text: &str,
    user_tx: &mpsc::Sender<(String, Option<serde_json::Value>, Option<String>)>,
    confirm_tx: &std::sync::mpsc::Sender<crate::confirm::ConfirmResult>,
    ask_user_tx: &std::sync::mpsc::Sender<String>,
    output: &Arc<WsOutput>,
    shared_workdir: &Arc<std::sync::Mutex<Option<PathBuf>>>,
    shared_mode: &Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>>,
    ctrl_tx: &mpsc::UnboundedSender<ControlCmd>,
) {
    let msg: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            output.emit_public("error", serde_json::json!({ "message": format!("Invalid JSON: {}", e) }));
            return;
        }
    };

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "user_message" => {
            let user_text = msg.get("data").and_then(|d| d.get("text"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            if user_text.is_empty() {
                output.emit_public("error", serde_json::json!({ "message": "Empty user_message text" }));
                return;
            }
            let req_id = msg.get("id").cloned();
            let workdir = msg.get("data").and_then(|d| d.get("workdir")).and_then(|v| v.as_str())
                .or_else(|| msg.get("allowed_dir").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
            if user_tx.try_send((user_text, req_id, workdir)).is_err() {
                output.emit_public("error", serde_json::json!({
                    "message": "Agent is busy processing a previous request"
                }));
            }
        }

        "set_workdir" => {
            if let Some(dir) = msg.get("data").and_then(|d| d.get("workdir")).and_then(|v| v.as_str()) {
                let p = PathBuf::from(dir);
                if p.is_dir() {
                    if let Ok(mut g) = shared_workdir.lock() { *g = Some(p); }
                } else {
                    output.emit_public("warning", serde_json::json!({
                        "message": format!("set_workdir: '{}' is not a valid directory", dir)
                    }));
                }
            }
        }

        "confirm_response" => {
            use crate::confirm::ConfirmResult;
            let data = msg.get("data");
            if let Some(c) = data.and_then(|d| d.get("clarify")).and_then(|v| v.as_str()) {
                let _ = confirm_tx.send(ConfirmResult::Clarify(c.to_string()));
            } else {
                let approved = data.and_then(|d| d.get("approved"))
                    .and_then(|v| v.as_bool()).unwrap_or(false);
                let _ = confirm_tx.send(if approved { ConfirmResult::Yes } else { ConfirmResult::No });
            }
        }

        "ask_user_response" => {
            let answer = msg.get("data").and_then(|d| d.get("answer"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let _ = ask_user_tx.send(answer);
        }

        "review_plan_response" => {
            let data = msg.get("data").cloned().unwrap_or(serde_json::json!({}));
            let approved = data.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
            let feedback = data.get("feedback").and_then(|v| v.as_str()).unwrap_or("");
            let action = if approved { if !feedback.is_empty() { "refine" } else { "approve" } } else { "reject" };
            let _ = ask_user_tx.send(serde_json::json!({ "action": action, "feedback": feedback }).to_string());
        }

        "set_model" => {} // informational

        "load_session"     => { let _ = ctrl_tx.send(ControlCmd::LoadSession); }
        "new_session"      => { let _ = ctrl_tx.send(ControlCmd::NewSession); }

        "load_session_by_id" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::LoadSessionById(id.to_string()));
            }
        }

        "list_sessions" => {
            match crate::persistence::list_sessions() {
                Ok(sessions) => {
                    let list: Vec<_> = sessions.iter().map(|s| serde_json::json!({
                        "id": s.id, "summary": s.summary, "updated_at": s.updated_at,
                        "message_count": s.message_count, "working_dir": s.working_dir,
                    })).collect();
                    output.emit_public("sessions_list", serde_json::json!({ "sessions": list }));
                }
                Err(e) => output.emit_public("error", serde_json::json!({
                    "message": format!("list_sessions failed: {:#}", e)
                })),
            }
        }

        "delete_session" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                match crate::persistence::delete_session(id) {
                    Ok(()) => output.emit_public("session_deleted", serde_json::json!({ "id": id })),
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("delete_session failed: {:#}", e)
                    })),
                }
            }
        }

        "set_mode" => {
            use crate::router::ExecutionMode;
            let mode_str = msg.get("data").and_then(|d| d.get("mode"))
                .and_then(|v| v.as_str()).unwrap_or("auto");
            let mode = match mode_str {
                "simple"   => Some(ExecutionMode::BasicLoop),
                "plan"     => Some(ExecutionMode::PlanAndExecute),
                "pipeline" => Some(ExecutionMode::FullPipeline),
                _          => None,
            };
            if let Ok(mut g) = shared_mode.lock() { *g = mode; }
        }

        "set_sandbox" => {
            let enabled = msg.get("data").and_then(|d| d.get("enabled"))
                .and_then(|v| v.as_bool()).unwrap_or(false);
            let _ = ctrl_tx.send(ControlCmd::SetSandbox(enabled));
        }

        "sandbox_list_changes" => { let _ = ctrl_tx.send(ControlCmd::SandboxListChanges); }
        "sandbox_commit"       => { let _ = ctrl_tx.send(ControlCmd::SandboxCommit); }
        "sandbox_commit_file"  => {
            if let Some(file_path) = msg.get("data").and_then(|d| d.get("file_path")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::SandboxCommitFile(file_path.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({ "message": "Missing file_path in sandbox_commit_file" }));
            }
        }
        "sandbox_rollback"     => { let _ = ctrl_tx.send(ControlCmd::SandboxRollback); }

        "cancel" => {
            crate::agent::request_interrupt();
            output.emit_public("cancelled", serde_json::json!({ "message": "中断请求已发送" }));
        }

        other => {
            tracing::debug!("Ignoring unknown WS message type: '{}'", other);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════

fn session_info_json(workdir: &Path) -> serde_json::Value {
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

fn messages_to_json(messages: &[crate::conversation::Message]) -> Vec<serde_json::Value> {
    messages.iter().filter_map(|m| {
        let text = m.text_content();
        if text.is_empty() { return None; }
        let role = match m.role {
            crate::conversation::Role::User      => "user",
            crate::conversation::Role::Assistant => "assistant",
            crate::conversation::Role::System    => "system",
        };
        Some(serde_json::json!({ "id": m.id, "role": role, "content": text }))
    }).collect()
}

// Required for TcpStream::from_raw_fd
use std::os::unix::io::FromRawFd;
