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

use anyhow::{Context, Result};
use tokio::net::TcpListener;

use crate::config::Config;

/// Read the cluster token from the `AGENT_CLUSTER_TOKEN` environment variable.
fn cluster_token_from_env() -> Option<String> {
    std::env::var("AGENT_CLUSTER_TOKEN").ok().filter(|s| !s.is_empty())
}use crate::container::{ContainerConfig, IsolationMode, CONTAINER_EXE, setup_rootfs};

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
    channel_configs: Vec<crate::plugin::ChannelConfig>,
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
        sa.sa_sigaction = sigchld_handler as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = libc::SA_RESTART | libc::SA_NOCLDSTOP;
        libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut());
    }

    cleanup_stale_worker_dirs();

    // Cluster token from the environment. No token = open server (local use).
    let cluster_token: Option<String> = cluster_token_from_env();

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

    // ── Channels ──────────────────────────────────────────────────────────
    // 启动所有插件声明的通道进程（如 WeChat Bridge）。
    let _channel_manager = {
        let mut mgr = crate::plugin::ChannelManager::new(port);
        if !channel_configs.is_empty() {
            println!("📡 Starting {} channel(s)...", channel_configs.len());
            mgr.spawn_all(channel_configs);
        }
        let mgr = Arc::new(tokio::sync::Mutex::new(mgr));
        crate::plugin::ChannelManager::spawn_watchdog(mgr.clone());
        mgr
    };

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
        // /file   → plain HTTP file download
        // unknown → 404, no fork
        let req_path = parse_path_from_http(&peek_buf[..peek_n]);
        match req_path.as_deref().unwrap_or("/agent") {
            "/file" => {
                // Plain HTTP GET — download a file from the project directory.
                // Query: ?path=<url-encoded relative-or-absolute path>
                // Token validation already passed above; safe to respond.
                let path_str = parse_query_param(&peek_buf[..peek_n], "path");
                let proj_dir = conn_project_dir.clone();
                tokio::spawn(async move {
                    handle_file_download(stream, proj_dir, path_str).await;
                });
                continue;
            }
            "/agent" | "/" => {
                // fall through to fork
            }
            other => {
                tracing::warn!("Unknown path '{}' from {} — rejected", other, peer);
                use tokio::io::AsyncWriteExt;
                let body = format!("Unknown path: {other}. Available: /agent (LLM session), /file (file download)");
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

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let extra_binds = config.extra_binds.clone();
        let project_dir_for_container = conn_project_dir.clone();
        // Resolve ~/.config/rust_agent/ on the host for the global-config bind.
        let global_config_dir: Option<std::path::PathBuf> = dirs::home_dir()
            .map(|h| h.join(".config").join("rust_agent"));

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
                .arg("--isolation").arg(conn_isolation.to_string());
            c.env("AGENT_PARENT_PORT", port.to_string());
            if let Some(ref tok) = cluster_token {
                c.env("AGENT_CLUSTER_TOKEN", tok);
            }

            // Set HOME=/root so dirs::config_dir() resolves to /root/.config
            // inside the container, matching the global_config bind mount.
            c.env("HOME", "/root");

            // Set up the container rootfs in the child between fork() and exec().
            // SAFETY: pre_exec runs single-threaded in the forked child; we only
            // do pure Linux syscalls and file I/O, no tokio or async.
            let use_overlay = conn_isolation == IsolationMode::Sandbox;
            let exe_clone2 = exe_clone.clone();
            let global_config_dir_clone = global_config_dir.clone();
            unsafe {
                c.pre_exec(move || {
                    setup_rootfs(&ContainerConfig {
                        project_dir: project_dir_for_container.clone(),
                        exe_path: exe_clone2.clone(),
                        extra_binds: extra_binds.clone(),
                        uid,
                        gid,
                        overlay: use_overlay,
                        global_config_dir: global_config_dir_clone.clone(),
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
    s.bytes().flat_map(|b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            vec![b as char]
        } else {
            format!("%{:02X}", b).chars().collect()
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

/// Write a minimal plain-text HTTP error response and close the connection.
async fn http_error(s: &mut tokio::net::TcpStream, status: &str, body: &str) {
    use tokio::io::AsyncWriteExt;
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, body.len(), body
    );
    let _ = s.write_all(resp.as_bytes()).await;
}

/// Handle a `GET /file?path=<url-encoded>` request — stream a file from the
/// project directory with proper download headers.
///
/// Security mirrors `read_file_content` in the worker:
///   - the path is canonicalized and must resolve inside the project dir
///   - directories and missing files get proper 4xx responses
/// Cluster-token validation happens before routing (see accept loop), so any
/// request reaching here is already authenticated.
async fn handle_file_download(
    stream: tokio::net::TcpStream,
    project_dir: std::path::PathBuf,
    path_str: Option<String>,
) {
    use tokio::io::AsyncWriteExt;

    let mut s = stream;

    let path_str = match path_str {
        Some(p) if !p.is_empty() => p,
        _ => {
            http_error(&mut s, "400 Bad Request", "Missing 'path' query parameter. Usage: /file?path=<path>&token=<token>").await;
            return;
        }
    };

    let path = std::path::Path::new(&path_str);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    };

    // Path security check — must stay inside the project directory.
    let canonical = match resolved.canonicalize() {
        Ok(c) => c,
        Err(_) => {
            http_error(&mut s, "404 Not Found", "File not found").await;
            return;
        }
    };
    let canonical_pd = project_dir.canonicalize().unwrap_or_else(|_| project_dir.clone());
    if !canonical.starts_with(&canonical_pd) {
        http_error(&mut s, "403 Forbidden", "Access denied: path is outside the project directory").await;
        return;
    }

    // Must be a regular file (not a directory / symlink to one).
    let meta = match tokio::fs::metadata(&canonical).await {
        Ok(m) => m,
        Err(_) => {
            http_error(&mut s, "404 Not Found", "File not found").await;
            return;
        }
    };
    if meta.is_dir() {
        http_error(&mut s, "400 Bad Request", "Path is a directory, not a file").await;
        return;
    }

    // Headers: MIME type, size, download filename (RFC 5987 filename* for non-ASCII).
    let size = meta.len();
    let filename = canonical
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let mime = mime_guess::from_path(&canonical).first_or_octet_stream();
    let encoded_name = url_encode(&filename);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nContent-Disposition: attachment; filename=\"{}\"; filename*=UTF-8''{}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        mime, size, filename, encoded_name
    );
    if s.write_all(head.as_bytes()).await.is_err() {
        return;
    }

    // Stream the file body.
    let mut file = match tokio::fs::File::open(&canonical).await {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = tokio::io::copy(&mut file, &mut s).await;
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
