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

use anyhow::{Context, Result};
use tokio::net::TcpListener;

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
    let cluster_token: Option<String> = crate::workspaces::load(&project_dir).cluster.token;

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
            .unwrap_or_else(|_| "{}".to_string());

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
            .arg("--config-json").arg(&config_json);
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

/// Minimal percent-decoding (`%XX` → byte, `+` → space).
fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i+1..i+3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b as char);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
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
