//! WebSocket server mode — process-per-connection design.
//!
//! Listens for TCP connections and spawns a **worker** subprocess for each
//! one, passing the accepted socket fd directly to the child.  The child
//! handles all agent logic, sandbox setup, and WebSocket protocol; this
//! process does nothing but accept and spawn.
//!
//! See `docs/SANDBOX_ARCH.md` for the full design rationale.

use std::os::unix::io::IntoRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;
use crate::container::{ContainerConfig, CONTAINER_EXE, setup_rootfs};

/// Start the WebSocket server and listen forever.
///
/// Each accepted TCP connection is handed off to a freshly-spawned worker
/// process.  The raw fd is passed via `--worker-fd N`.  The parent stays
/// in its accept loop and never touches the WebSocket protocol at all.
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    host: &str,
    port: u16,
    sandbox: bool,
) -> Result<()> {
    // Reap zombie worker processes asynchronously via a real SIGCHLD handler.
    //
    // We cannot use signal(SIGCHLD, SIG_IGN) or SA_NOCLDWAIT: both cause the
    // kernel to auto-reap children, which also makes waitpid(child) return
    // ECHILD inside std::process::Command::spawn()'s error-recovery path,
    // triggering "wait() should either return Ok or panic".
    //
    // A real handler (not SIG_IGN) is reset to SIG_DFL across exec(2), so
    // worker processes inherit SIG_DFL and their own waitpid() calls work fine.
    unsafe extern "C" fn sigchld_handler(_: libc::c_int) {
        // Reap all available children without blocking.
        loop {
            let r = libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG);
            if r <= 0 {
                break;
            }
        }
    }
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sigchld_handler as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = libc::SA_RESTART | libc::SA_NOCLDSTOP;
        libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut());
    }

    cleanup_stale_worker_dirs();

    // Load cluster token once at startup.  No token = open server (local use).
    let ws_cfg = crate::workspaces::load(&project_dir);
    let cluster_token: Option<String> = ws_cfg.cluster.token.clone(); // clone so ws_cfg stays intact
    // Keep the full workspaces config behind Arc for the /nodes HTTP endpoint.
    let cached_ws_cfg = Arc::new(ws_cfg);

    // Probe capabilities once at startup and cache behind Arc so every probe
    // connection can clone cheaply without re-running nvidia-smi etc.
    let (caps, virtual_nodes) = crate::workspaces::probe_capabilities(&cached_ws_cfg.workspaces);
    let cached_caps    = Arc::new(caps);
    let cached_vnodes  = Arc::new(virtual_nodes);
    let cached_workdir = project_dir.display().to_string();

    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;

    println!("🤖 Agent WebSocket server listening on ws://{}", addr);
    println!("   Provider: {:?}  Model: {}", config.provider, config.model);
    if sandbox {
        println!("   Sandbox: ENABLED by default (override per-connection via URL sandbox=1/0)");
    } else {
        println!("   Sandbox: disabled by default (enable per-connection via URL sandbox=1)");
    }
    if cluster_token.is_some() {
        println!("   Auth: cluster token required");
    }
    println!("   Press Ctrl+C to stop.\n");

    // Get the path to the current executable so we can re-exec ourselves.
    let exe = std::env::current_exe().context("could not determine executable path")?;

    loop {
        let (stream, peer) = listener.accept().await?;

        // Peek at the opening HTTP request (without consuming bytes) to extract
        // the ?workdir= query parameter sent by the frontend in the WebSocket URL.
        // e.g. ws://127.0.0.1:9527/?workdir=%2Fhome%2Fuser%2Fmyproject
        // We MUST determine the project_dir here, before fork(), because
        // pre_exec() runs between fork and exec — too late to receive WS messages.
        let mut peek_buf = [0u8; 2048];
        let peek_n = stream.peek(&mut peek_buf).await.unwrap_or(0);
        let conn_project_dir = parse_workdir_from_http(&peek_buf[..peek_n])
            .unwrap_or_else(|| project_dir.clone());
        // Per-connection sandbox flag: URL param `sandbox=1` overrides the
        // server-level default (set at startup with --sandbox).
        let conn_sandbox = parse_sandbox_from_http(&peek_buf[..peek_n]).unwrap_or(sandbox);

        // Token validation.  If a cluster token is configured, reject connections
        // that don't present it.  We send a minimal HTTP 401 response and drop
        // the TCP stream — no process is forked for rejected connections.
        if let Some(required) = &cluster_token {
            let presented = parse_token_from_http(&peek_buf[..peek_n]);
            if presented.as_deref() != Some(required.as_str()) {
                tracing::warn!("Rejected connection from {} — token mismatch", peer);
                use tokio::io::AsyncWriteExt;
                let mut stream = stream; // move out of peek reference
                let _ = stream.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ).await;
                continue;
            }
        }

        tracing::info!("Accepted connection from {} -> project_dir={:?} sandbox={}", peer, conn_project_dir, conn_sandbox);

        // ── Path-based routing ────────────────────────────────────────────────
        // Route BEFORE converting to a raw fd.
        // /agent  → fork worker (LLM session)
        // /probe  → inline ready-frame handler (no fork)
        // unknown → 404, no fork
        let req_path = parse_path_from_http(&peek_buf[..peek_n]);
        match req_path.as_deref().unwrap_or("/agent") {
            "/probe" => {
                let caps   = cached_caps.clone();
                let vnodes = cached_vnodes.clone();
                let wd     = cached_workdir.clone();
                let sb     = conn_sandbox;
                tokio::spawn(async move {
                    handle_probe(stream, sb, wd, caps, vnodes).await;
                });
                continue;
            }
            "/nodes" => {
                // Plain HTTP GET — return all known nodes as JSON.
                // Token validation already passed above; safe to respond.
                let body = build_nodes_json(&cached_ws_cfg);
                tokio::spawn(async move {
                    use tokio::io::AsyncWriteExt;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    );
                    let mut s = stream;
                    let _ = s.write_all(resp.as_bytes()).await;
                });
                continue;
            }
            "/agent" | "/" => {
                // fall through to fork
            }
            other => {
                tracing::warn!("Unknown path '{}' from {} — rejected", other, peer);
                use tokio::io::AsyncWriteExt;
                let body = format!("Unknown path: {other}. Available: /agent (LLM session), /probe (capability query), /nodes (node list)");
                let resp = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                let mut s = stream;
                let _ = s.write_all(resp.as_bytes()).await;
                continue;
            }
        }

        // Convert to raw fd AFTER peeking (peek is non-consuming).
        let raw_fd = stream.into_std()?.into_raw_fd();

        // Clear FD_CLOEXEC so the child inherits it across exec.
        unsafe { libc::fcntl(raw_fd, libc::F_SETFD, 0) };

        let worker_id = uuid::Uuid::new_v4().to_string();
        let exe_clone = exe.clone();

        // Serialize the fully-resolved config so the worker does not need to
        // read models.toml or .env files at all — safe inside a filesystem
        // sandbox where those paths may not be accessible.
        let config_json = serde_json::to_string(&config)
            .unwrap_or_else(|_| "{}" .to_string());
        // Serialize the workspaces list so the worker can build the virtual_nodes
        // ready frame without touching the filesystem (which may be unavailable
        // inside the container namespace).
        let workspaces_json = serde_json::to_string(&cached_ws_cfg.workspaces)
            .unwrap_or_else(|_| "[]".to_string());

        // Gather info needed by the container setup closure.
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let extra_binds = config.extra_binds.clone();
        let project_dir_for_container = conn_project_dir.clone();

        // Inside the container the agent binary is always at CONTAINER_EXE (/agent).
        let mut cmd = std::process::Command::new(CONTAINER_EXE);
        cmd.arg("--mode").arg("worker")
            .arg("--worker-fd").arg(raw_fd.to_string())
            .arg("--worker-id").arg(&worker_id)
            // Inside the container, project_dir is always /workspace.
            .arg("-d").arg("/workspace")
            .arg("--config-json").arg(&config_json)
            .arg("--workspaces-json").arg(&workspaces_json);
        // Expose parent server address so worker tools (list_nodes, call_node)
        // can query localhost:{port}/nodes without reading the filesystem.
        cmd.env("AGENT_PARENT_PORT", port.to_string());
        if let Some(ref tok) = cluster_token {
            cmd.env("AGENT_CLUSTER_TOKEN", tok);
        }
        if conn_sandbox {
            cmd.arg("--sandbox");
        }

        // Set up the container rootfs in the child between fork() and exec().
        // SAFETY: pre_exec runs single-threaded in the forked child; we only
        // do pure Linux syscalls and file I/O, no tokio or async.
        unsafe {
            cmd.pre_exec(move || {
                setup_rootfs(&ContainerConfig {
                    project_dir: project_dir_for_container.clone(),
                    exe_path: exe_clone.clone(),
                    extra_binds: extra_binds.clone(),
                    uid,
                    gid,
                    sandbox: conn_sandbox,
                })
            });
        }

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(
                    "Spawned worker pid={} id={} for {}",
                    child.id(),
                    worker_id,
                    peer
                );
            }
            Err(e) => {
                tracing::warn!("Failed to spawn worker for {}: {}", peer, e);
            }
        }

        // Close our copy of the fd — the child has its own.
        unsafe { libc::close(raw_fd) };
    }
}

/// Extract the URL path (without query string) from the raw HTTP request line.
/// e.g. `GET /agent?workdir=... HTTP/1.1` → `Some("/agent")`
fn parse_path_from_http(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let full = line.split_whitespace().nth(1)?;
    let path = full.split('?').next().unwrap_or(full);
    Some(path.to_string())
}

/// Extract the `workdir` query parameter from the raw bytes of an HTTP Upgrade
/// request.  Uses `peek()` so the bytes are not consumed from the socket.
fn parse_workdir_from_http(bytes: &[u8]) -> Option<std::path::PathBuf> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for param in query.split('&') {
        if let Some(val) = param.strip_prefix("workdir=") {
            let decoded = url_decode(val);
            let p = std::path::PathBuf::from(decoded);
            if p.is_absolute() {
                return Some(p);
            }
        }
    }
    None
}

/// Extract the `sandbox` query parameter (`sandbox=1` / `sandbox=0`) from the
/// raw HTTP Upgrade request bytes.  Returns `None` if the param is absent.
fn parse_sandbox_from_http(bytes: &[u8]) -> Option<bool> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for param in query.split('&') {
        if param == "sandbox=1" || param == "sandbox=true" {
            return Some(true);
        }
        if param == "sandbox=0" || param == "sandbox=false" {
            return Some(false);
        }
    }
    None
}

/// Extract the `token` query parameter from the raw HTTP Upgrade request bytes.
fn parse_token_from_http(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for param in query.split('&') {
        if let Some(val) = param.strip_prefix("token=") {
            return Some(url_decode(val));
        }
    }
    None
}

/// Percent-decoding (`%XX` → byte, `+` → space).
/// Collects raw bytes first, then converts to UTF-8 — required for multi-byte
/// sequences such as Chinese characters (`调` → `%E8%B0%83`).
fn url_decode(s: &str) -> String {
    let mut buf: Vec<u8> = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i+1..i+3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    buf.push(b);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            buf.push(b' ');
            i += 1;
            continue;
        } else {
            buf.push(bytes[i]);
            i += 1;
            continue;
        }
        buf.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(buf)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

// ── Inline probe handler (no fork) ──────────────────────────────────────────

/// Handle a `/probe` WebSocket connection entirely in the parent process.
/// Sends one `ready` frame with cached capabilities and virtual nodes, then
/// waits for the client to close (or times out after 5 s).
async fn handle_probe(
    stream: tokio::net::TcpStream,
    sandbox_enabled: bool,
    workdir: String,
    caps: Arc<crate::workspaces::NodeCapabilities>,
    virtual_nodes: Arc<Vec<crate::workspaces::VirtualNodeInfo>>,
) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => { tracing::debug!("probe: WS upgrade failed: {}", e); return; }
    };
    let (mut write, mut read) = ws.split();

    let payload = serde_json::json!({
        "type": "ready",
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "workdir": workdir,
            "sandbox": sandbox_enabled,
            "caps": *caps,
            "virtual_nodes": *virtual_nodes,
        }
    });
    if let Err(e) = write.send(Message::Text(payload.to_string().into())).await {
        tracing::debug!("probe: send ready failed: {}", e);
        return;
    }

    // Drain until client closes or 5 s elapses.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        async { while read.next().await.map(|m| m.is_ok()).unwrap_or(false) {} },
    ).await;
}

/// Build the JSON body for `GET /nodes`.
///
/// Returns all statically configured `[[remote]]` entries merged with any
/// virtual-node entries that have been dynamically discovered and stored in
/// the in-process route table (populated by `/probe` calls or `call_node`
/// `ready` frames).  The combined list lets worker sub-agents resolve node
/// names and perform tag-based routing without reading the filesystem.
fn build_nodes_json(ws_cfg: &crate::workspaces::WorkspacesFile) -> String {
    use crate::workspaces;
    let mut nodes: Vec<serde_json::Value> = Vec::new();

    // Static [[remote]] entries from workspaces.toml.
    for r in ws_cfg.all_remotes() {
        nodes.push(serde_json::json!({
            "name":   r.name,
            "url":    r.url,
            "source": "static",
        }));
    }

    // Dynamic entries from the in-process route table (discovered at runtime).
    if let Ok(table) = workspaces::get_route_table() {
        for e in &table {
            nodes.push(serde_json::json!({
                "name":        format!("{}/{}", e.server_name, e.node_name),
                "server_name": e.server_name,
                "node_name":   e.node_name,
                "url":         e.server_url,
                "workdir":     e.workdir,
                "sandbox":     e.sandbox,
                "tags":        e.tags,
                "source":      "route_table",
            }));
        }
    }

    serde_json::json!({ "nodes": nodes }).to_string()
}

fn cleanup_stale_worker_dirs() {
    let tmp = std::path::Path::new("/tmp");
    let entries = match std::fs::read_dir(tmp) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // fuse-overlayfs work dirs
        let is_overlay = name_str.starts_with("agent-worker-");
        // container rootfs dirs (format: .agent-nr-{pid})
        let is_newroot = name_str.starts_with(".agent-nr-");
        if is_overlay || is_newroot {
            let path = entry.path();
            tracing::debug!("Cleaning up stale dir: {:?}", path);
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}
