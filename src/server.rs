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
use crate::container::{ContainerConfig, IsolationMode, CONTAINER_EXE, setup_rootfs};

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
    isolation: IsolationMode,
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
    let cluster_token: Option<String> = ws_cfg.cluster.token.clone();
    // Keep the full workspaces config behind Arc for the /nodes HTTP endpoint.
    let cached_ws_cfg = Arc::new(ws_cfg);

    // Probe capabilities once at startup and cache behind Arc so every probe
    // connection can clone cheaply without re-running nvidia-smi etc.
    let (caps, virtual_nodes) = crate::workspaces::probe_capabilities(&cached_ws_cfg.local_nodes());
    let cached_caps    = Arc::new(caps);
    let cached_vnodes  = Arc::new(virtual_nodes);
    let cached_workdir = project_dir.display().to_string();

    // ── Phase 2: NodeRegistry initialisation ─────────────────────────────────
    // 1. Seed the registry with local [[node]] entries (always online).
    crate::workspaces::registry_init_local(&cached_ws_cfg.local_nodes(), port);
    // 2. Spawn background task that probes all [[peer]] entries at startup,
    //    then retries offline peers every 30s and heartbeats online ones every 120s.
    spawn_probe_loop(
        cached_ws_cfg.peers.clone(),
        cluster_token.clone(),
        port,
    );

    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;

    println!("🤖 Agent WebSocket server listening on ws://{}", addr);
    println!("   Provider: {:?}  Model: {}", config.provider, config.model);
    match isolation {
        IsolationMode::Normal    => println!("   Isolation: normal (no container) — default per-connection (override via URL mode=normal/container/sandbox)"),
        IsolationMode::Container => println!("   Isolation: container (namespace+rootfs, direct write) — default per-connection"),
        IsolationMode::Sandbox   => println!("   Isolation: sandbox (container+overlayfs, /rollback enabled) — default per-connection"),
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
        // Per-connection isolation mode: URL param `mode=normal/container/sandbox`
        // overrides the server-level default.  Legacy `sandbox=1/0` still understood.
        let conn_isolation = parse_isolation_from_http(&peek_buf[..peek_n]).unwrap_or(isolation);

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

        tracing::info!("Accepted connection from {} -> project_dir={:?} isolation={}", peer, conn_project_dir, conn_isolation);

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
                let sb     = conn_isolation;
                tokio::spawn(async move {
                    handle_probe(stream, sb, wd, caps, vnodes).await;
                });
                continue;
            }
            "/nodes" => {
                // Plain HTTP GET — return all known nodes as JSON.
                // Token validation already passed above; safe to respond.
                let body = build_nodes_json(&cached_ws_cfg, port);
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
            "/reprobe" => {
                // On-demand peer re-probe triggered by call_node when a target is offline.
                // Query param: ?peer=<peer_name>
                // Returns updated /nodes JSON after re-probing the specified peer.
                let peer_name = parse_query_param(&peek_buf[..peek_n], "peer");
                let peers_cfg = cached_ws_cfg.peers.clone();
                let tok       = cluster_token.clone();
                let port_cap  = port;
                tokio::spawn(async move {
                    use tokio::io::AsyncWriteExt;
                    let mut s = stream;
                    if let Some(ref name) = peer_name {
                        if let Some(peer) = peers_cfg.iter().find(|p| &p.name == name) {
                            probe_and_update(peer, tok.as_deref(), port_cap).await;
                        }
                    }
                    // Return updated node list.
                    let body = crate::workspaces::registry_snapshot();
                    let json = serde_json::json!({ "nodes": body }).to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        json.len(), json
                    );
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
                let body = format!("Unknown path: {other}. Available: /agent (LLM session), /probe (capability query), /nodes (node list), /reprobe (on-demand peer probe)");
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
        let workspaces_json = serde_json::to_string(&cached_ws_cfg.local_nodes())
            .unwrap_or_else(|_| "[]".to_string());

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let extra_binds = config.extra_binds.clone();
        let project_dir_for_container = conn_project_dir.clone();

        // ── Spawn strategy depends on isolation mode ──────────────────────────
        //
        // Normal    → no container at all; use the real executable and real
        //             project_dir.  Worker runs on the host directly.
        // Container → namespace + rootfs; pre_exec calls setup_rootfs with
        //             overlay=false.  Worker sees /workspace as a rw bind.
        // Sandbox   → namespace + rootfs + overlayfs; pre_exec calls
        //             setup_rootfs with overlay=true.  Writes go to tmpfs.
        let mut cmd = if conn_isolation == IsolationMode::Normal {
            // Normal mode: spawn directly — no container, no pre_exec.
            let mut c = std::process::Command::new(&exe);
            c.arg("--mode").arg("worker")
                .arg("--worker-fd").arg(raw_fd.to_string())
                .arg("--worker-id").arg(&worker_id)
                .arg("-d").arg(&conn_project_dir)
                .arg("--config-json").arg(&config_json)
                .arg("--workspaces-json").arg(&workspaces_json)
                .arg("--isolation").arg(conn_isolation.to_string());
            c.env("AGENT_PARENT_PORT", port.to_string());
            if let Some(ref tok) = cluster_token {
                c.env("AGENT_CLUSTER_TOKEN", tok);
            }
            c
        } else {
            // Container / Sandbox mode: exec inside a fresh rootfs.
            let mut c = std::process::Command::new(CONTAINER_EXE);
            c.arg("--mode").arg("worker")
                .arg("--worker-fd").arg(raw_fd.to_string())
                .arg("--worker-id").arg(&worker_id)
                // Inside the container project_dir is always /workspace.
                .arg("-d").arg("/workspace")
                .arg("--config-json").arg(&config_json)
                .arg("--workspaces-json").arg(&workspaces_json)
                .arg("--isolation").arg(conn_isolation.to_string());
            c.env("AGENT_PARENT_PORT", port.to_string());
            if let Some(ref tok) = cluster_token {
                c.env("AGENT_CLUSTER_TOKEN", tok);
            }

            // Set up the container rootfs in the child between fork() and exec().
            // SAFETY: pre_exec runs single-threaded in the forked child; we only
            // do pure Linux syscalls and file I/O, no tokio or async.
            let use_overlay = conn_isolation == IsolationMode::Sandbox;
            let exe_clone2 = exe_clone.clone();
            unsafe {
                c.pre_exec(move || {
                    setup_rootfs(&ContainerConfig {
                        project_dir: project_dir_for_container.clone(),
                        exe_path: exe_clone2.clone(),
                        extra_binds: extra_binds.clone(),
                        uid,
                        gid,
                        overlay: use_overlay,
                    })
                });
            }
            c
        };

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

/// Extract the isolation mode from the raw HTTP Upgrade request bytes.
/// Supports `mode=normal/container/sandbox` (preferred) and legacy
/// `sandbox=1` → Sandbox, `sandbox=0` → Container.
/// Returns `None` if no relevant param is present.
fn parse_isolation_from_http(bytes: &[u8]) -> Option<IsolationMode> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for param in query.split('&') {
        if let Some(val) = param.strip_prefix("mode=") {
            return val.parse::<IsolationMode>().ok();
        }
        // Legacy compat
        if param == "sandbox=1" || param == "sandbox=true" {
            return Some(IsolationMode::Sandbox);
        }
        if param == "sandbox=0" || param == "sandbox=false" {
            return Some(IsolationMode::Container);
        }
    }
    None
}

/// Extract the `token` query parameter from the raw HTTP Upgrade request bytes.
fn parse_token_from_http(bytes: &[u8]) -> Option<String> {
    parse_query_param(bytes, "token")
}

/// Extract an arbitrary query parameter by name from the raw HTTP request bytes.
fn parse_query_param(bytes: &[u8], key: &str) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    let prefix = format!("{}=", key);
    for param in query.split('&') {
        if let Some(val) = param.strip_prefix(&prefix) {
            return Some(url_decode(val));
        }
    }
    None
}

/// Percent-encoding for query parameter values.
fn url_encode(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            vec![c]
        } else {
            format!("%{:02X}", c as u32).chars().collect()
        }
    }).collect()
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
    isolation: IsolationMode,
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
            "isolation": isolation.to_string(),
            // legacy field — kept for older web-ui versions
            "sandbox": isolation == IsolationMode::Sandbox,
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
/// Returns the NodeRegistry snapshot: local nodes + peer-expanded sub-nodes.
/// Does NOT include raw [[peer]] gateway entries — those are server-internal.
fn build_nodes_json(_ws_cfg: &crate::workspaces::WorkspacesFile, _port: u16) -> String {
    use crate::workspaces::{NodeStatus, registry_snapshot};
    let entries = registry_snapshot();

    let nodes: Vec<serde_json::Value> = entries.iter()
        // Skip tombstone placeholders (name starts with "(unreachable)@")
        .filter(|e| !e.name.starts_with("(unreachable)@"))
        .map(|e| {
            let mut obj = serde_json::json!({
                "name":        e.name,
                "url":         e.url,
                "status":      e.status.to_string(),
                "tags":        e.tags,
                "sandbox":     e.sandbox,
                "description": e.description,
            });
            if let Some(ref peer) = e.peer_name {
                obj["source"]    = serde_json::json!("remote");
                obj["peer_name"] = serde_json::json!(peer);
            } else {
                obj["source"] = serde_json::json!("local");
            }
            if let Some(secs) = e.last_seen_secs {
                obj["last_seen_secs"] = serde_json::json!(secs);
            }
            // Offline remote nodes expose the peer name so call_node can trigger re-probe.
            if matches!(e.status, NodeStatus::Offline) {
                obj["offline"] = serde_json::json!(true);
            }
            obj
        })
        .collect();

    // Also include offline placeholder rows so callers know unreachable peers.
    let placeholders: Vec<serde_json::Value> = entries.iter()
        .filter(|e| e.name.starts_with("(unreachable)@"))
        .map(|e| serde_json::json!({
            "name":         e.name,
            "peer_name":    e.peer_name,
            "status":       "offline",
            "source":       "remote",
            "description":  e.description,
        }))
        .collect();

    let mut all = nodes;
    all.extend(placeholders);
    serde_json::json!({ "nodes": all }).to_string()
}

// ── Peer probe functions ──────────────────────────────────────────────────────

/// Connect to a peer's `/probe` WebSocket, receive the `ready` frame, return
/// the list of virtual nodes it advertises.  Returns `None` on any error.
async fn probe_peer_once(
    peer_url: &str,
    token: Option<&str>,
) -> Option<Vec<crate::workspaces::VirtualNodeInfo>> {
    let base = crate::workspaces::with_path(peer_url, "/probe");
    let url = match token {
        Some(tok) => {
            let sep = if base.contains('?') { '&' } else { '?' };
            format!("{}{}token={}", base, sep, url_encode(tok))
        }
        None => base,
    };

    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(&url),
    ).await.ok()?.ok()?;

    let (mut write, mut read) = ws.split();

    let msg = tokio::time::timeout(Duration::from_secs(5), read.next())
        .await.ok()??.ok()?;

    // Politely close after reading the ready frame.
    let _ = write.close().await;

    match msg {
        Message::Text(text) => {
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            if v["type"].as_str() != Some("ready") { return None; }
            let vnodes: Vec<crate::workspaces::VirtualNodeInfo> =
                serde_json::from_value(v["data"]["virtual_nodes"].clone()).unwrap_or_default();
            Some(vnodes)
        }
        _ => None,
    }
}

/// Probe a peer and update the NodeRegistry accordingly.
async fn probe_and_update(
    peer: &crate::workspaces::PeerEntry,
    cluster_token: Option<&str>,
    port: u16,
) {
    let tok = peer.token.as_deref().or(cluster_token);
    match probe_peer_once(&peer.url, tok).await {
        Some(vnodes) => {
            let base_url = peer.url.trim_end_matches('/').split('?').next().unwrap_or(&peer.url);
            let entries: Vec<crate::workspaces::RegistryEntry> = vnodes.iter().map(|vn| {
                let enc: String = vn.workdir.chars().flat_map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '/') {
                        vec![c]
                    } else {
                        format!("%{:02X}", c as u32).chars().collect()
                    }
                }).collect();
                crate::workspaces::RegistryEntry {
                    name:           format!("{}@{}", vn.name, peer.name),
                    url:            format!("{}/?workdir={}", base_url, enc),
                    peer_name:      Some(peer.name.clone()),
                    status:         crate::workspaces::NodeStatus::Online,
                    last_seen_secs: crate::workspaces::unix_now_pub(),
                    tags:           vn.tags.clone(),
                    sandbox:        vn.sandbox,
                    description:    vn.description.clone(),
                }
            }).collect();
            let n = entries.len();
            crate::workspaces::registry_update_peer(&peer.name, entries);
            tracing::info!("probe: peer '{}' online — {} node(s) expanded", peer.name, n);
        }
        None => {
            crate::workspaces::registry_mark_peer_offline(&peer.name, &peer.url);
            tracing::warn!("probe: peer '{}' ({}) unreachable", peer.name, peer.url);
        }
    }
}

/// Spawn the background probe loop in a Tokio task.
///
/// - On startup: probes all peers concurrently.
/// - Every 30 s: re-probes `offline` peers.
/// - Every 120 s: heartbeat-probes `online` peers.
pub fn spawn_probe_loop(
    peers: Vec<crate::workspaces::PeerEntry>,
    cluster_token: Option<String>,
    port: u16,
) {
    if peers.is_empty() { return; }
    tokio::spawn(async move {
        // Initial probe — all peers concurrently.
        let futs: Vec<_> = peers.iter()
            .map(|p| probe_and_update(p, cluster_token.as_deref(), port))
            .collect();
        futures::future::join_all(futs).await;

        // Periodic polls.
        let mut tick30  = tokio::time::interval(Duration::from_secs(30));
        let mut tick120 = tokio::time::interval(Duration::from_secs(120));
        tick30.tick().await;   // consume first immediate tick
        tick120.tick().await;

        loop {
            tokio::select! {
                _ = tick30.tick() => {
                    // Retry offline peers.
                    let offline_peers: Vec<_> = {
                        let snap = crate::workspaces::registry_snapshot();
                        peers.iter()
                            .filter(|p| snap.iter().any(|e|
                                e.peer_name.as_deref() == Some(&p.name) &&
                                e.status == crate::workspaces::NodeStatus::Offline
                            ))
                            .cloned()
                            .collect()
                    };
                    for p in &offline_peers {
                        probe_and_update(p, cluster_token.as_deref(), port).await;
                    }
                }
                _ = tick120.tick() => {
                    // Heartbeat online peers.
                    let online_peers: Vec<_> = {
                        let snap = crate::workspaces::registry_snapshot();
                        peers.iter()
                            .filter(|p| snap.iter().any(|e|
                                e.peer_name.as_deref() == Some(&p.name) &&
                                e.status == crate::workspaces::NodeStatus::Online
                            ))
                            .cloned()
                            .collect()
                    };
                    for p in &online_peers {
                        probe_and_update(p, cluster_token.as_deref(), port).await;
                    }
                }
            }
        }
    });
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
