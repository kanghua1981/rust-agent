use super::*;

// ═══════════════════════════════════════════════════════════════════
//  WebSocket message dispatcher
// ═══════════════════════════════════════════════════════════════════

pub(super) async fn dispatch_ws_message(
    text: &str,
    user_tx: &mpsc::Sender<(String, Option<serde_json::Value>, Option<String>)>,
    confirm_tx: &std::sync::mpsc::Sender<crate::confirm::ConfirmResult>,
    ask_user_tx: &std::sync::mpsc::Sender<String>,
    output: &Arc<WsOutput>,
    shared_workdir: &Arc<std::sync::Mutex<Option<PathBuf>>>,
    shared_mode: &Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>>,
    ctrl_tx: &mpsc::UnboundedSender<ControlCmd>,
    global_db: &Arc<GlobalDb>,
    project_dir: &PathBuf,
    pty_handle: &Arc<std::sync::Mutex<Option<PtyHandle>>>,
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

        "set_model" => {
            if let Some(alias) = msg.get("data").and_then(|d| d.get("model")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::SetModel(alias.to_string()));
            }
        }

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

        // ── Local named session commands ──────────────────────────────

        "list_local_sessions" => {
            let _ = ctrl_tx.send(ControlCmd::ListLocalSessions);
        }

        "switch_local_session" => {
            if let Some(name) = msg.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::SwitchLocalSession(name.to_string()));
            }
        }

        "new_local_session" => {
            if let Some(name) = msg.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::NewLocalNamedSession(name.to_string()));
            }
        }

        "delete_local_session" => {
            if let Some(name) = msg.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::DeleteLocalNamedSession(name.to_string()));
            }
        }

        "rename_local_session" => {
            let old = msg.get("data").and_then(|d| d.get("old_name")).and_then(|v| v.as_str());
            let new = msg.get("data").and_then(|d| d.get("new_name")).and_then(|v| v.as_str());
            if let (Some(old), Some(new)) = (old, new) {
                let _ = ctrl_tx.send(ControlCmd::RenameLocalSession {
                    old: old.to_string(),
                    new: new.to_string(),
                });
            }
        }

        // ── Node CRUD (server-managed workspaces) ────────────────────────────
        "list_nodes" => {
            let cached = crate::workspaces::load_vnodes();
            output.emit_public("nodes_list", serde_json::json!({
                "virtual_nodes": cached,
            }));
        }

        "add_node" => {
            match msg.get("data").cloned() {
                Some(data) => match serde_json::from_value::<crate::db::models::Node>(data) {
                    Ok(node) => match global_db.save_node(&node) {
                        Ok(()) => {
                            // Rebuild virtual_nodes after mutation
                            let cached = crate::workspaces::load_vnodes();
                            output.emit_public("node_saved", serde_json::json!({
                                "node": node,
                                "virtual_nodes": cached,
                            }));
                        }
                        Err(e) => output.emit_public("error", serde_json::json!({
                            "message": format!("add_node failed: {:#}", e)
                        })),
                    },
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("add_node: invalid node data: {}", e)
                    })),
                },
                None => output.emit_public("error", serde_json::json!({
                    "message": "add_node: missing 'data' field"
                })),
            }
        }

        "update_node" => {
            match msg.get("data").cloned() {
                Some(data) => match serde_json::from_value::<crate::db::models::Node>(data) {
                    Ok(node) => match global_db.save_node(&node) {
                        Ok(()) => {
                            let cached = crate::workspaces::load_vnodes();
                            output.emit_public("node_saved", serde_json::json!({
                                "node": node,
                                "virtual_nodes": cached,
                            }));
                        }
                        Err(e) => output.emit_public("error", serde_json::json!({
                            "message": format!("update_node failed: {:#}", e)
                        })),
                    },
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("update_node: invalid node data: {}", e)
                    })),
                },
                None => output.emit_public("error", serde_json::json!({
                    "message": "update_node: missing 'data' field"
                })),
            }
        }

        "delete_node" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                match global_db.delete_node(id) {
                    Ok(()) => {
                        let cached = crate::workspaces::load_vnodes();
                        output.emit_public("node_deleted", serde_json::json!({
                            "id": id,
                            "virtual_nodes": cached,
                        }));
                    }
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("delete_node failed: {:#}", e)
                    })),
                }
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "delete_node: missing 'data.id'"
                }));
            }
        }

        // ── Peer CRUD (remote agent servers for discovery) ───────────────────
        "list_peers" => {
            match global_db.list_peers() {
                Ok(peers) => {
                    output.emit_public("peers_list", serde_json::json!({
                        "peers": peers,
                    }));
                }
                Err(e) => output.emit_public("error", serde_json::json!({
                    "message": format!("list_peers failed: {:#}", e)
                })),
            }
        }

        "add_peer" => {
            match msg.get("data").cloned() {
                Some(data) => match serde_json::from_value::<crate::db::models::Peer>(data) {
                    Ok(peer) => match global_db.save_peer(&peer) {
                        Ok(()) => {
                            let peers = global_db.list_peers().unwrap_or_default();
                            output.emit_public("peer_saved", serde_json::json!({
                                "peer": peer,
                                "peers": peers,
                            }));
                        }
                        Err(e) => output.emit_public("error", serde_json::json!({
                            "message": format!("add_peer failed: {:#}", e)
                        })),
                    },
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("add_peer: invalid peer data: {}", e)
                    })),
                },
                None => output.emit_public("error", serde_json::json!({
                    "message": "add_peer: missing 'data' field"
                })),
            }
        }

        "update_peer" => {
            match msg.get("data").cloned() {
                Some(data) => match serde_json::from_value::<crate::db::models::Peer>(data) {
                    Ok(peer) => match global_db.save_peer(&peer) {
                        Ok(()) => {
                            let peers = global_db.list_peers().unwrap_or_default();
                            output.emit_public("peer_saved", serde_json::json!({
                                "peer": peer,
                                "peers": peers,
                            }));
                        }
                        Err(e) => output.emit_public("error", serde_json::json!({
                            "message": format!("update_peer failed: {:#}", e)
                        })),
                    },
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("update_peer: invalid peer data: {}", e)
                    })),
                },
                None => output.emit_public("error", serde_json::json!({
                    "message": "update_peer: missing 'data' field"
                })),
            }
        }

        "delete_peer" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                match global_db.delete_peer(id) {
                    Ok(()) => {
                        let peers = global_db.list_peers().unwrap_or_default();
                        output.emit_public("peer_deleted", serde_json::json!({
                            "id": id,
                            "peers": peers,
                        }));
                    }
                    Err(e) => output.emit_public("error", serde_json::json!({
                        "message": format!("delete_peer failed: {:#}", e)
                    })),
                }
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "delete_peer: missing 'data.id'"
                }));
            }
        }

        // ── Directory & file browsing ─────────────────────────────────────
        "list_dir" => {
            let path_str = msg.get("data")
                .and_then(|d| d.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let path = std::path::Path::new(path_str);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            };
            // Path security: canonicalize and ensure within project_dir
            let canonical = match resolved.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("list_dir: cannot resolve path '{}': {}", path_str, e)
                    }));
                    return;
                }
            };
            let canonical_pd = project_dir.canonicalize().unwrap_or_else(|_| project_dir.clone());
            if !canonical.starts_with(&canonical_pd) {
                output.emit_public("error", serde_json::json!({
                    "message": format!("Access denied: '{}' is outside the project directory.", path_str)
                }));
                return;
            }
            match crate::tools::list_dir::ListDirTool::list_structured(&canonical, project_dir).await {
                Ok(entries) => {
                    output.emit_public("dir_list_result", serde_json::json!({
                        "path": path_str,
                        "entries": entries,
                    }));
                }
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("list_dir failed: {}", e)
                    }));
                }
            }
        }

        "read_file_content" => {
            let path_str = msg.get("data")
                .and_then(|d| d.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if path_str.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "read_file_content: missing 'data.path'"
                }));
                return;
            }
            let path = std::path::Path::new(path_str);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            };
            // Path security check
            let canonical = match resolved.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("read_file_content: cannot resolve path '{}': {}", path_str, e)
                    }));
                    return;
                }
            };
            let canonical_pd = project_dir.canonicalize().unwrap_or_else(|_| project_dir.clone());
            if !canonical.starts_with(&canonical_pd) {
                output.emit_public("error", serde_json::json!({
                    "message": format!("Access denied: '{}' is outside the project directory.", path_str)
                }));
                return;
            }
            // 2MB hard limit
            const MAX_SIZE: u64 = 2 * 1024 * 1024;
            match tokio::fs::metadata(&canonical).await {
                Ok(meta) => {
                    if meta.is_dir() {
                        output.emit_public("error", serde_json::json!({
                            "message": format!("'{}' is a directory, not a file.", path_str)
                        }));
                        return;
                    }
                    let size = meta.len();
                    if size > MAX_SIZE {
                        output.emit_public("file_content_result", serde_json::json!({
                            "path": path_str,
                            "content": format!("[File too large: {} (limit: 2MB)]", format_size(size)),
                            "size": size,
                            "truncated": true,
                        }));
                        return;
                    }
                    match tokio::fs::read_to_string(&canonical).await {
                        Ok(content) => {
                            output.emit_public("file_content_result", serde_json::json!({
                                "path": path_str,
                                "content": content,
                                "size": size,
                            }));
                        }
                        Err(e) => {
                            // Try as binary if UTF-8 fails
                            let content_preview = format!("[Binary file: {}]", format_size(size));
                            output.emit_public("file_content_result", serde_json::json!({
                                "path": path_str,
                                "content": content_preview,
                                "size": size,
                                "binary": true,
                                "error": format!("Failed to read as text: {}", e),
                            }));
                        }
                    }
                }
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("read_file_content: cannot access '{}': {}", path_str, e)
                    }));
                }
            }
        }

        // ── Open file with external editor ───────────────────────────────
        "open_file_external" => {
            let path_str = msg.get("data")
                .and_then(|d| d.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if path_str.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "open_file_external: missing 'data.path'"
                }));
                return;
            }
            let path = std::path::Path::new(path_str);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            };
            // Path security check
            let canonical = match resolved.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("open_file_external: cannot resolve path '{}': {}", path_str, e)
                    }));
                    return;
                }
            };
            let canonical_pd = project_dir.canonicalize().unwrap_or_else(|_| project_dir.clone());
            if !canonical.starts_with(&canonical_pd) {
                output.emit_public("error", serde_json::json!({
                    "message": format!("Access denied: '{}' is outside the project directory.", path_str)
                }));
                return;
            }
            // Resolve editor: EDITOR_COMMAND env → EDITOR env → platform default
            let editor = std::env::var("EDITOR_COMMAND")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| {
                    if cfg!(target_os = "macos") {
                        "open".to_string()
                    } else {
                        "xdg-open".to_string()
                    }
                });
            // Handle editor commands with arguments (e.g. "code --goto {path}:{line}")
            let task = if editor.contains("{path}") {
                let cmd_str = editor.replace("{path}", &canonical.display().to_string());
                let parts: Vec<&str> = if cfg!(windows) {
                    // Windows: use cmd /c for shell parsing
                    vec!["cmd", "/c", &cmd_str]
                } else {
                    vec!["sh", "-c", &cmd_str]
                };
                let (program, args) = parts.split_first().unwrap();
                std::process::Command::new(program)
                    .args(args)
                    .spawn()
            } else {
                std::process::Command::new(&editor)
                    .arg(&canonical)
                    .spawn()
            };
            match task {
                Ok(mut child) => {
                    // Don't wait — detach the child process
                    let _ = child.stdin.take();
                    output.emit_public("file_opened_external", serde_json::json!({
                        "path": path_str,
                        "editor": editor,
                    }));
                }
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("open_file_external: failed to launch editor '{}': {}", editor, e)
                    }));
                }
            }
        }

        // ── PTY Terminal ─────────────────────────────────────────────────
        "pty_open" => {
            let data = msg.get("data");
            let rows = data.and_then(|d| d.get("rows")).and_then(|v| v.as_u64()).unwrap_or(24) as u16;
            let cols = data.and_then(|d| d.get("cols")).and_then(|v| v.as_u64()).unwrap_or(80) as u16;
            // Use requested workdir or fall back to project directory
            let workdir = data
                .and_then(|d| d.get("workdir"))
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| (*project_dir).clone());

            // Close existing PTY if any
            {
                let mut guard = pty_handle.lock().unwrap();
                if let Some(ref mut old) = *guard {
                    let _ = old.close();
                }
                *guard = None;
            }

            match PtyHandle::spawn(workdir, rows, cols) {
                Ok(mut pty) => {
                    let mut output_rx = pty.take_output_rx();
                    let output_clone = output.clone();

                    // Spawn task to forward PTY output to WebSocket
                    tokio::spawn(async move {
                        while let Some(data) = output_rx.recv().await {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                            output_clone.emit_public("pty_output", serde_json::json!({
                                "output": b64,
                            }));
                        }
                        // Child exited
                        output_clone.emit_public("pty_exit", serde_json::json!({ "code": 0 }));
                    });

                    *pty_handle.lock().unwrap() = Some(pty);
                }
                Err(e) => {
                    output.emit_public("pty_error", serde_json::json!({
                        "message": format!("Failed to open terminal: {}", e),
                    }));
                }
            }
        }

        "pty_input" => {
            let b64 = msg.get("data")
                .and_then(|d| d.get("input"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap_or_default();
            let mut guard = pty_handle.lock().unwrap();
            if let Some(ref mut pty) = *guard {
                if let Err(e) = pty.write(&data) {
                    output.emit_public("pty_error", serde_json::json!({
                        "message": format!("PTY write error: {}", e),
                    }));
                }
            }
        }

        "pty_resize" => {
            let data = msg.get("data");
            let rows = data.and_then(|d| d.get("rows")).and_then(|v| v.as_u64()).unwrap_or(24) as u16;
            let cols = data.and_then(|d| d.get("cols")).and_then(|v| v.as_u64()).unwrap_or(80) as u16;
            let mut guard = pty_handle.lock().unwrap();
            if let Some(ref mut pty) = *guard {
                if let Err(e) = pty.resize(rows, cols) {
                    output.emit_public("pty_error", serde_json::json!({
                        "message": format!("PTY resize error: {}", e),
                    }));
                }
            }
        }

        "pty_close" => {
            let mut guard = pty_handle.lock().unwrap();
            if let Some(ref mut pty) = *guard {
                let _ = pty.close();
            }
            *guard = None;
        }

        "set_mode" => {
            use crate::router::ExecutionMode;
            let mode_str = msg.get("data").and_then(|d| d.get("mode"))
                .and_then(|v| v.as_str()).unwrap_or("auto");
            match mode_str {
                "simple" => {
                    if let Ok(mut g) = shared_mode.lock() { *g = Some(ExecutionMode::BasicLoop); }
                    let _ = ctrl_tx.send(ControlCmd::SetPlanMode(false));
                }
                "plan" => {
                    if let Ok(mut g) = shared_mode.lock() { *g = None; }
                    let _ = ctrl_tx.send(ControlCmd::SetPlanMode(true));
                }
                _ => {
                    if let Ok(mut g) = shared_mode.lock() { *g = None; }
                    let _ = ctrl_tx.send(ControlCmd::SetPlanMode(false));
                }
            }
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

        // ── MCP dynamic loading ───────────────────────────────────────────────
        // Message format:
        //   { "type": "load_mcp",
        //     "data": { "servers": [ { "name": "github", "command": "npx",
        //                             "args": [...], "env": { "TOKEN": "..." } },
        //                           { "name": "remote", "url": "http://...",
        //                             "headers": { "Authorization": "Bearer ..." } } ] } }
        "load_mcp" => {
            let servers_val = msg.get("data")
                .and_then(|d| d.get("servers"))
                .cloned()
                .unwrap_or(serde_json::json!([]));
            match serde_json::from_value::<Vec<crate::mcp_client::McpServerEntry>>(servers_val) {
                Ok(entries) if !entries.is_empty() => {
                    let _ = ctrl_tx.send(ControlCmd::LoadMcp(entries));
                }
                Ok(_) => {
                    output.emit_public("error", serde_json::json!({
                        "message": "load_mcp: 'data.servers' is empty or missing"
                    }));
                }
                Err(e) => {
                    output.emit_public("error", serde_json::json!({
                        "message": format!("load_mcp: failed to parse servers: {}", e)
                    }));
                }
            }
        }

        // Message format:
        //   { "type": "unload_mcp", "data": { "prefix": "github" } }
        "unload_mcp" => {
            if let Some(prefix) = msg.get("data").and_then(|d| d.get("prefix")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::UnloadMcp(prefix.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "unload_mcp: missing 'data.prefix'"
                }));
            }
        }

        // Message format:
        //   { "type": "list_mcp_tools" }
        "list_mcp_tools" => { let _ = ctrl_tx.send(ControlCmd::ListMcpTools); }

        // ── Plugin management (仅内存，不持久化） ──────────────────────────────
        // { "type": "list_plugins" }
        // { "type": "enable_plugin",  "data": { "id": "my-plugin" } }
        // { "type": "disable_plugin", "data": { "id": "my-plugin" } }
        "list_plugins" => { let _ = ctrl_tx.send(ControlCmd::ListPlugins); }

        "enable_plugin" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::EnablePlugin(id.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "enable_plugin: missing 'data.id'"
                }));
            }
        }

        "disable_plugin" => {
            if let Some(id) = msg.get("data").and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::DisablePlugin(id.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "disable_plugin: missing 'data.id'"
                }));
            }
        }

        // ── Reconnection recovery ─────────────────────────────────────────
        "resume" => {
            let from_seq = msg.get("data").and_then(|d| d.get("from_seq"))
                .and_then(|v| v.as_u64()).unwrap_or(0);
            let last_seq = output.last_seq();
            output.emit_public("resume_ack", serde_json::json!({
                "from_seq": from_seq,
                "last_seq": last_seq,
                "accepted": true,
            }));
            tracing::info!(
                "Resume request: client asked from_seq={}, server last_seq={}",
                from_seq, last_seq
            );
        }

        "cancel" => {
            crate::agent::request_interrupt();
            output.emit_public("cancelled", serde_json::json!({ "message": "中断请求已发送" }));
        }

        // ── File upload ───────────────────────────────────────────────────
        // Message format:
        //   { "type": "upload_file",
        //     "data": { "name": "photo.png", "content": "<base64>",
        //               "mime_type": "image/png", "target_dir": "images" } }
        "upload_file" => {
            let name = msg.get("data").and_then(|d| d.get("name"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content_b64 = msg.get("data").and_then(|d| d.get("content"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mime_type = msg.get("data").and_then(|d| d.get("mime_type"))
                .and_then(|v| v.as_str()).map(|s| s.to_string());

            if name.is_empty() || content_b64.is_empty() {
                output.emit_public("upload_file_result", serde_json::json!({
                    "success": false,
                    "error": "Missing 'name' or 'content' in upload_file data"
                }));
                return;
            }

            // Decode base64
            let data = match base64_decode(&content_b64) {
                Some(d) => d,
                None => {
                    output.emit_public("upload_file_result", serde_json::json!({
                        "success": false,
                        "name": name,
                        "error": "Invalid base64 content"
                    }));
                    return;
                }
            };

            let _ = ctrl_tx.send(ControlCmd::UploadFile(name, data, mime_type));
        }

        // ── Model & endpoint management ────────────────────────────────────
        "list_endpoints" => {
            let _ = ctrl_tx.send(ControlCmd::ListEndpoints);
        }
        "fetch_models" => {
            let url = msg.get("data").and_then(|d| d.get("url"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let api_key = msg.get("data").and_then(|d| d.get("api_key"))
                .and_then(|v| v.as_str()).map(|s| s.to_string());
            if url.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "fetch_models: missing 'data.url'"
                }));
            } else {
                let _ = ctrl_tx.send(ControlCmd::FetchModels { url, api_key });
            }
        }
        "add_model" => {
            let alias = msg.get("data").and_then(|d| d.get("alias"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let model = msg.get("data").and_then(|d| d.get("model"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let endpoint_name = msg.get("data").and_then(|d| d.get("endpoint"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            if alias.is_empty() || model.is_empty() || endpoint_name.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "add_model: missing alias/model/endpoint"
                }));
            } else {
                let _ = ctrl_tx.send(ControlCmd::AddModel { alias, model, endpoint_name });
            }
        }
        "delete_model" => {
            if let Some(alias) = msg.get("data").and_then(|d| d.get("alias")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::DeleteModel(alias.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "delete_model: missing 'data.alias'"
                }));
            }
        }
        "add_endpoint" => {
            let name = msg.get("data").and_then(|d| d.get("name"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let provider = msg.get("data").and_then(|d| d.get("provider"))
                .and_then(|v| v.as_str()).unwrap_or("openai").to_string();
            let base_url = msg.get("data").and_then(|d| d.get("base_url"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let api_key = msg.get("data").and_then(|d| d.get("api_key"))
                .and_then(|v| v.as_str()).map(|s| s.to_string());
            if name.is_empty() || base_url.is_empty() {
                output.emit_public("error", serde_json::json!({
                    "message": "add_endpoint: missing name/base_url"
                }));
            } else {
                let _ = ctrl_tx.send(ControlCmd::AddEndpoint { name, provider, base_url, api_key });
            }
        }
        "delete_endpoint" => {
            if let Some(name) = msg.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()) {
                let _ = ctrl_tx.send(ControlCmd::DeleteEndpoint(name.to_string()));
            } else {
                output.emit_public("error", serde_json::json!({
                    "message": "delete_endpoint: missing 'data.name'"
                }));
            }
        }

        other => {
            tracing::debug!("Ignoring unknown WS message type: '{}'", other);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════

