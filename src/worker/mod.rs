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
use base64::Engine;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::agent::Agent;
use crate::config::Config;
use crate::container::IsolationMode;
use crate::output::{WsCommand, WsOutput};
use crate::pty::PtyHandle;
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
/// Isolation mode controls sandbox setup:
/// - Normal    → no sandbox, direct host access.
/// - Container → no sandbox overlay (rootfs set up by server before exec).
/// - Sandbox   → kernel overlayfs already mounted by server; wire it up.
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    fd: i32,
    isolation: IsolationMode,
    _worker_id: &str,
    _extra_binds: Vec<BindMount>,
) -> Result<()> {
    run_async(config, project_dir, isolation, fd).await
}


// ═══════════════════════════════════════════════════════════════════
//  Async agent loop
// ═══════════════════════════════════════════════════════════════════

async fn run_async(
    config: Config,
    project_dir: PathBuf,
    isolation: IsolationMode,
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

    // ── Plugin system ─────────────────────────────────────────────────────
    // Worker 加载插件的方式和 CLI 完全一致：
    //   - 项目插件：<project_dir>/.agent/plugins/  (容器内 = /workspace/.agent/plugins/)
    //   - 全局插件：~/.config/rust_agent/plugins/  (容器内 bind-mount 到 /root/.config/rust_agent/)
    //
    // 重要原则：enable/disable 仅修改内存，永远不回写磁盘（plugin.toml 只读）。
    // 客户端通过 WS 消息动态控制本 session 内哪些插件启用，持久化由客户端负责。
    let plugin_manager = {
        let pm = crate::plugin::PluginManager::new(project_dir.clone());
        Arc::new(tokio::sync::Mutex::new(pm))
    };
    {
        let mut pm_lock = plugin_manager.lock().await;
        if let Err(e) = pm_lock.load_all_plugins() {
            tracing::warn!("Worker: failed to load plugins: {}", e);
        }
        pm_lock.load_system_skills(&project_dir);
    }

    let mut agent = Agent::new(
        config,
        project_dir.clone(),
        ws_output.clone(),
        Sandbox::disabled(&project_dir),
        Some(plugin_manager.clone()),
    );

    // Hook 总线 + system_prompt 追加（与 cli.rs 保持一致）
    let extra_prompt: String;
    let hook_bus: Arc<crate::plugin::hook_bus::HookBus>;
    {
        let pm_lock = plugin_manager.lock().await;
        hook_bus = pm_lock.get_hook_bus();
        extra_prompt = pm_lock.collect_system_prompts();
        drop(pm_lock);
        agent.set_hook_bus(Some(hook_bus.clone()));
        if !extra_prompt.is_empty() {
            agent.conversation.system_prompt.push_str(&extra_prompt);
        }
        // agent.start hook（fire-and-forget）
        {
            use crate::plugin::hook_bus::HookEvent;
            let session_id = agent.session_id().unwrap_or("none").to_string();
            hook_bus.emit(HookEvent::new(
                "agent.start",
                session_id,
                serde_json::json!({
                    "project_dir": project_dir.display().to_string(),
                    "mode": "worker",
                }),
            ));
        }
    }

    // 注册插件工具 + MCP
    if let Err(e) = agent.load_plugin_tools().await {
        tracing::warn!("Worker: failed to load plugin tools: {}", e);
    }
    {
        let pm_lock = plugin_manager.lock().await;
        let mcp_entries = pm_lock.collect_mcp_entries();
        drop(pm_lock);
        if !mcp_entries.is_empty() {
            let (loaded, errors) = agent.load_mcp_from_entries(&mcp_entries).await;
            if !loaded.is_empty() {
                tracing::info!("Worker plugin MCP tools: {}", loaded.join(", "));
            }
            for err in &errors {
                tracing::warn!("Worker plugin MCP: {}", err);
            }
        }
    }

    // Plugin skills 索引注入（与 cli.rs 保持一致）
    {
        let pm_lock = plugin_manager.lock().await;
        let plugin_skills: Vec<_> = pm_lock.get_all_skills()
            .into_iter()
            .filter(|s| s.plugin_id != "@system")
            .collect();
        drop(pm_lock);
        if !plugin_skills.is_empty() {
            let mut section = "\n\n--- Plugin Skills ---".to_string();
            section.push_str("\n## Available Plugin Skills (use `load_skill` tool with the skill name to read full content)");
            for skill in &plugin_skills {
                let tags_hint = if skill.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [tags: {}]", skill.tags.join(", "))
                };
                section.push_str(&format!(
                    "\n- **{}** (plugin: {}){} — {}",
                    skill.name, skill.plugin_id, tags_hint, skill.description,
                ));
            }
            agent.conversation.system_prompt.push_str(&section);
        }
    }

    // Apply isolation mode to the sandbox handle.
    //
    // Normal    → no sandbox; runs on host.
    // Container → rootfs was set up by server (pre_exec), /workspace is a rw
    //             bind of the real project.  No overlay, no rollback.
    // Sandbox   → server pre_exec also mounted kernel overlayfs:
    //               lower  = /workspace-ro  (real project, read-only view)
    //               upper  = /tmp/overlay/upper  (writes land here, tmpfs)
    //               merged = /workspace  (tools see a mutable view)
    //             Detect /workspace-ro and wire up Sandbox::from_overlay_dirs.
    if isolation == IsolationMode::Sandbox {
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
        // emit a warning to the frontend before the session starts.
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
    let shared_project_dir: Arc<PathBuf> = Arc::new(project_dir.clone());
    let shared_project_dir_reader = shared_project_dir.clone();
    let shared_mode: Arc<std::sync::Mutex<Option<crate::router::ExecutionMode>>> =
        Arc::new(std::sync::Mutex::new(None));
    let shared_mode_reader = shared_mode.clone();

    // PTY terminal state
    let pty_handle: Arc<std::sync::Mutex<Option<PtyHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let pty_handle_reader = pty_handle.clone();

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
                        &shared_project_dir_reader,
                        &pty_handle_reader,
                    ).await;
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

    // Send ready event — workdir, sandbox state and model selection.

    // ── Auto-restore local session (same as CLI mode) ─────────────────
    // Worker mode: resolve which named session to load, then auto-restore
    // the conversation so context is preserved.  Only emit a "session_available"
    // summary to the UI — the user must explicitly click "restore" to load the
    // full message history into the chat area.
    let active_session = crate::persistence::resolve_session_name(&project_dir, None);
    let _ = crate::persistence::migrate_old_local_session(&project_dir);
    match crate::persistence::load_local_named_session(&active_session, &project_dir) {
        Ok(Some(session)) => {
            let msg_count = session.messages.len();
            let mut conv = crate::persistence::restore_conversation(&session);
            // Re-apply plugin system prompts in case they've changed since
            // the session was last saved.
            if !extra_prompt.is_empty() {
                conv.system_prompt.push_str(&extra_prompt);
            }
            agent.conversation = conv;
            agent.set_session_id(active_session.clone());
            ws_output.emit_public("session_available", serde_json::json!({
                "message_count": msg_count,
                "session_id": active_session,
                "session_name": active_session,
            }));
            tracing::info!(
                "Worker: auto-restored session '{}' with {} messages for {} (UI notified)",
                active_session,
                msg_count,
                project_dir.display()
            );
        }
        Ok(None) => {
            // No existing session - fresh start, write _active marker
            agent.set_session_id(active_session.clone());
            let _ = crate::persistence::save_local_named_session(
                &active_session, &agent.conversation, &project_dir,
            );
            let _ = crate::persistence::write_active_session_name(&project_dir, &active_session);
        }
        Err(e) => {
            tracing::warn!("Worker: failed to load local session '{}': {}", active_session, e);
        }
    }

    let available_models: Vec<serde_json::Value> = agent.models_cfg.models.iter().map(|(alias, entry)| {
        serde_json::json!({
            "alias": alias,
            "provider": entry.provider,
            "model": entry.model,
            "base_url": entry.base_url,
            "endpoint": entry.endpoint,
            "thinking_enabled": entry.thinking_enabled,
            "reasoning_effort": entry.reasoning_effort,
        })
    }).collect();

    ws_output.emit_public("ready", serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "workdir": project_dir.display().to_string(),
        "isolation": isolation.to_string(),
        // legacy field for older clients
        "sandbox": isolation == IsolationMode::Sandbox,
        "sandbox_backend": agent.sandbox.backend_label_sync(),
        "available_models": available_models,
        "active_model": agent.config.model_alias,
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
                        if !agent.sandbox.is_disabled {
                            agent.set_allowed_dir(Some(p));
                        }
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
                        let (input_tokens, output_tokens) = agent.token_usage();
                        let role_usage: serde_json::Map<String, serde_json::Value> = agent
                            .role_token_usage()
                            .iter()
                            .map(|(role, &(inp, out))| {
                                (role.clone(), serde_json::json!([inp, out]))
                            })
                            .collect();
                        let done = serde_json::json!({
                            "text": final_text,
                            "pending_changes": pending,
                            "input_tokens": input_tokens,
                            "output_tokens": output_tokens,
                            "role_usage": role_usage,
                        });
                        if let Some(ref req_id) = req_id {
                            let id_str = req_id.as_str().unwrap_or("");
                            ws_output.emit_public_with_id("done", done, id_str);
                        } else {
                            ws_output.emit_public("done", done);
                        }

                        // Notify frontend of updated sandbox state after every turn.
                        if !agent.sandbox.is_disabled {
                            ws_output.emit_public("sandbox_status", serde_json::json!({
                                "enabled": true,
                                "backend": agent.sandbox.backend_label_sync(),
                                "pending_changes": pending,
                            }));
                        }

                        if let Err(e) = {
                            let name = agent.session_id().unwrap_or("default");
                            crate::persistence::save_local_named_session(
                                name, &agent.conversation, &agent.project_dir,
                            )
                        } { tracing::warn!("save_local_named_session: {}", e); }

                        if let Err(e) = crate::persistence::save_session_for_workdir(
                            &agent.conversation, &agent.project_dir,
                        ) { tracing::warn!("save_session_for_workdir: {}", e); }

                        ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                    }
                    Err(e) => {
                        let (input_tokens, output_tokens) = agent.token_usage();
                        ws_output.emit_public("error", serde_json::json!({
                            "message": format!("{:#}", e),
                            "input_tokens": input_tokens,
                            "output_tokens": output_tokens,
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
    /// 列出本地命名会话。
    ListLocalSessions,
    /// 切换到命名会话。
    SwitchLocalSession(String),
    /// 创建新的命名会话。
    NewLocalNamedSession(String),
    /// 删除命名会话。
    DeleteLocalNamedSession(String),
    /// 重命名本地会话。
    RenameLocalSession { old: String, new: String },
    /// Sandbox toggle: in worker mode sandbox is fixed at startup.
    /// We respond with the current status and optionally a warning.
    SetSandbox(bool),
    /// Toggle plan mode (set_mode 'plan' / 'simple' / 'auto').
    SetPlanMode(bool),
    SandboxListChanges,
    SandboxCommit,
    SandboxCommitFile(String),
    SandboxRollback,
    /// Connect to the supplied MCP servers and register their tools.
    /// Entries are supplied by the client at runtime (may include secrets).
    LoadMcp(Vec<crate::mcp_client::McpServerEntry>),
    /// Unload all tools registered under the given MCP server prefix.
    UnloadMcp(String),
    /// List all currently-loaded MCP tool names.
    ListMcpTools,
    /// 列出所有插件及其状态。
    ListPlugins,
    /// 为本会话启用指定插件（仅内存，不回写磁盘）。
    EnablePlugin(String),
    /// 为本会话禁用指定插件（仅内存，不回写磁盘）。
    DisablePlugin(String),
    /// 切换模型。
    SetModel(String),
    /// 上传文件到 workspace 的 uploads/ 目录。
    /// (文件名, base64内容, 可选MIME类型)
    UploadFile(String, Vec<u8>, Option<String>),
    /// 列出所有端点。
    ListEndpoints,
    /// 从远程 API 拉取模型列表。
    FetchModels { url: String, api_key: Option<String> },
    /// 添加模型条目。
    AddModel { alias: String, model: String, endpoint_name: String },
    /// 删除模型条目。
    DeleteModel(String),
    /// 添加端点定义。
    AddEndpoint { name: String, provider: String, base_url: String, api_key: Option<String> },
    /// 删除端点定义。
    DeleteEndpoint(String),
}

fn session_info_json(workdir: &Path) -> serde_json::Value {
    let name = crate::persistence::resolve_session_name(workdir, None);
    let local_count = crate::persistence::list_local_sessions(workdir)
        .map(|s| s.len()).unwrap_or(0);
    match crate::persistence::load_local_named_session(&name, workdir) {
        Ok(Some(session)) => serde_json::json!({
            "exists": true,
            "session_name": name,
            "message_count": session.meta.message_count,
            "updated_at": session.meta.updated_at,
            "summary": session.meta.summary,
            "working_dir": session.meta.working_dir,
            "local_session_count": local_count,
        }),
        _ => serde_json::json!({
            "exists": false,
            "session_name": name,
            "local_session_count": local_count,
        }),
    }
}

/// Emit the current model + endpoint state so the frontend stays in sync.
fn emit_model_state(ws_output: &Arc<WsOutput>, cfg: &crate::model_manager::ModelsConfig) {
    let models: Vec<serde_json::Value> = cfg.models.iter().map(|(alias, entry)| {
        serde_json::json!({
            "alias": alias,
            "provider": entry.provider,
            "model": entry.model,
            "base_url": entry.base_url,
            "endpoint": entry.endpoint,
            "thinking_enabled": entry.thinking_enabled,
            "reasoning_effort": entry.reasoning_effort,
        })
    }).collect();
    let endpoints: Vec<serde_json::Value> = cfg.endpoints.iter().map(|(name, ep)| {
        serde_json::json!({
            "name": name,
            "provider": ep.provider,
            "base_url": ep.base_url,
            "has_api_key": ep.api_key.is_some(),
        })
    }).collect();
    ws_output.emit_public("model_state", serde_json::json!({
        "models": models,
        "endpoints": endpoints,
        "default": cfg.default,
    }));
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

// ═══════════════════════════════════════════════════════════════════
//  File upload helpers
// ═══════════════════════════════════════════════════════════════════

/// Decode a base64 string to raw bytes. Returns None on invalid input.
fn base64_decode(b64: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// Maximum file size for uploads (50 MB).
const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

/// Save uploaded file data to `{project_dir}/uploads/{safe_name}`.
/// Returns the relative path (from project_dir) and file size on success.
fn save_uploaded_file(
    project_dir: &Path,
    name: &str,
    data: &[u8],
    mime_type: Option<&str>,
) -> Result<(String, u64), String> {
    // ── Security: path traversal protection ──────────────────────────
    // Only allow the file basename — strip any directory components.
    let safe_name = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Fallback: sanitize manually
            name.replace('/', "_").replace('\\', "_").replace("..", "_")
        });

    // Reject if the sanitized name is empty or still contains path separators.
    if safe_name.is_empty()
        || safe_name.contains('/') || safe_name.contains('\\')
        || safe_name.contains("..")
    {
        return Err(format!("Invalid file name: '{}'", name));
    }

    // ── Size limit ───────────────────────────────────────────────────
    if data.len() > MAX_UPLOAD_SIZE {
        return Err(format!(
            "File too large: {} bytes (max {})",
            data.len(),
            MAX_UPLOAD_SIZE
        ));
    }

    // ── Optional MIME type check ─────────────────────────────────────
    // If mime_type is provided, only reject obviously executable types.
    if let Some(mt) = mime_type {
        let blocked = [
            "application/x-msdownload",
            "application/x-executable",
            "application/x-sharedlib",
        ];
        if blocked.contains(&mt) {
            return Err(format!("Blocked file type: {}", mt));
        }
    }

    // ── Create uploads directory ─────────────────────────────────────
    let uploads_dir = project_dir.join("uploads");
    std::fs::create_dir_all(&uploads_dir)
        .map_err(|e| format!("Failed to create uploads directory: {}", e))?;

    // ── Handle name conflicts: add timestamp suffix ──────────────────
    let dest_path = uploads_dir.join(&safe_name);
    let final_path = if dest_path.exists() {
        let stem = safe_name
            .rfind('.')
            .map(|i| (&safe_name[..i], &safe_name[i..]))
            .unwrap_or((safe_name.as_str(), ""));
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        uploads_dir.join(format!("{}_{}{}", stem.0, ts, stem.1))
    } else {
        dest_path
    };

    // ── Write file ───────────────────────────────────────────────────
    std::fs::write(&final_path, data)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    let size = data.len() as u64;
    let rel_path = final_path
        .strip_prefix(project_dir)
        .unwrap_or(&final_path)
        .to_string_lossy()
        .to_string();

    Ok((rel_path, size))
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// Required for TcpStream::from_raw_fd
use std::os::unix::io::FromRawFd;

mod control;
mod dispatch;
use control::*;
use dispatch::*;
