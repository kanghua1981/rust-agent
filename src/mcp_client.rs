//! MCP (Model Context Protocol) client — Direction B.
//!
//! Connects to external MCP servers at startup, pulls their tool lists, and
//! registers each discovered tool in the agent's `ToolExecutor` so the LLM
//! can invoke them transparently alongside built-in tools.
//!
//! # Two transport modes
//!
//! **stdio** (default): spawns a local process and communicates via stdin/stdout.
//! **HTTP + SSE**: connects to a remote server via HTTP POST + Server-Sent Events.
//!   The client POSTs JSON-RPC requests and receives responses/notifications over
//!   a persistent SSE stream.  Notifications are forwarded into the agent's
//!   `push_service_event` channel so they surface between iterations.
//!
//! # Configuration
//!
//! Create `.agent/mcp.toml` (project-level) or
//! `~/.config/rust_agent/mcp.toml` (user-level).  Project-level takes priority
//! and the two files are merged (project entries listed first).
//!
//! ```toml
//! # .agent/mcp.toml
//!
//! # stdio transport (local subprocess)
//! [[server]]
//! name    = "filesystem"
//! command = "npx"
//! args    = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
//!
//! [[server]]
//! name    = "github"
//! command = "npx"
//! args    = ["-y", "@modelcontextprotocol/server-github"]
//! env     = { GITHUB_PERSONAL_ACCESS_TOKEN = "ghp_xxx" }
//!
//! # HTTP + SSE transport (remote server, no local process needed)
//! [[server]]
//! name = "remote-tools"
//! url  = "http://192.168.1.10:8080"          # base URL, /sse appended automatically
//!
//! # With authentication headers
//! [[server]]
//! name    = "pmcp"
//! url     = "http://your-server:8765/sse"       # full SSE URL also accepted
//! headers = { Authorization = "Bearer pmcp_xxxxxxxx" }
//! ```
//!
//! # Tool naming
//! Tools are prefixed with the server name to avoid collisions:
//! `filesystem__read_file`, `github__search_repositories`, etc.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::output::NotifyLevel;
use crate::service::push_service_event;
use crate::tools::{Tool, ToolDefinition, ToolResult};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct McpConfig {
    #[serde(default, rename = "server")]
    pub servers: Vec<McpServerEntry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpServerEntry {
    /// Human-readable name, used as tool prefix.
    pub name: String,
    /// Executable to spawn — required for stdio transport, unused for HTTP.
    #[serde(default)]
    pub command: String,
    /// Command-line arguments (stdio transport only).
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables injected into the server process (stdio only).
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// HTTP/SSE URL.  When present, uses HTTP+SSE transport instead of stdio.
    /// Accepted forms:
    ///   - Base URL only:  `http://host:8765`    → `/sse` is appended automatically.
    ///   - Full SSE URL:   `http://host:8765/sse` → used as-is for the SSE stream.
    /// The actual JSON-RPC POST endpoint is discovered from the SSE `endpoint` event.
    pub url: Option<String>,
    /// Extra HTTP headers attached to every SSE and POST request (HTTP transport only).
    /// Useful for authentication, e.g. `{ "Authorization" = "Bearer <token>" }`.
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Load config from `.agent/mcp.toml` (project) and
/// `~/.config/rust_agent/mcp.toml` (user, fallback).
/// Both files are loaded and merged; project entries come first.
pub fn load_config(project_dir: &Path) -> McpConfig {
    let mut merged = McpConfig::default();

    // User-level config first (lowest priority)
    if let Some(user_cfg_path) = dirs::config_dir().map(|d| d.join("rust_agent").join("mcp.toml")) {
        if let Ok(text) = std::fs::read_to_string(&user_cfg_path) {
            if let Ok(cfg) = toml::from_str::<McpConfig>(&text) {
                merged.servers.extend(cfg.servers);
            }
        }
    }

    // Project-level config (highest priority — prepended so it comes first)
    let project_cfg_path = project_dir.join(".agent").join("mcp.toml");
    if let Ok(text) = std::fs::read_to_string(&project_cfg_path) {
        if let Ok(cfg) = toml::from_str::<McpConfig>(&text) {
            let mut project_servers = cfg.servers;
            project_servers.extend(merged.servers);
            merged.servers = project_servers;
        }
    }

    merged
}

// ── Transport abstraction ─────────────────────────────────────────────────────

/// Unified interface over stdio, legacy HTTP+SSE, and new Streamable HTTP transports.
enum McpTransport {
    Stdio(McpConnection),
    Http(McpHttpConnection),
    /// MCP 2025 "Streamable HTTP" transport — single endpoint, POST returns response
    /// directly in the HTTP response body (JSON or inline SSE stream).
    StreamableHttp(McpStreamableHttpConnection),
}

impl McpTransport {
    async fn list_tools(&mut self) -> Result<Vec<Value>> {
        match self {
            Self::Stdio(c)          => c.list_tools().await,
            Self::Http(c)           => c.list_tools().await,
            Self::StreamableHttp(c) => c.list_tools().await,
        }
    }

    async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<(String, bool)> {
        match self {
            Self::Stdio(c)          => c.call_tool(name, arguments).await,
            Self::Http(c)           => c.call_tool(name, arguments).await,
            Self::StreamableHttp(c) => c.call_tool(name, arguments).await,
        }
    }
}

// ── stdio transport ───────────────────────────────────────────────────────────

struct McpConnection {
    /// Child process — kept alive for the connection lifetime.
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Monotonically increasing JSON-RPC request ID (protected by Mutex).
    next_id: u64,
}

impl McpConnection {
    /// Spawn the server process and complete the MCP `initialize` handshake.
    async fn spawn(entry: &McpServerEntry) -> Result<Self> {
        use tokio::process::Command;

        let mut cmd = Command::new(&entry.command);
        cmd.args(&entry.args)
            .envs(&entry.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null()); // suppress server chatter

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn MCP server '{}'", entry.name))?;

        let stdin = child.stdin.take().context("missing stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("missing stdout")?);

        let mut conn = McpConnection {
            _child: child,
            stdin,
            stdout,
            next_id: 1,
        };

        // MCP handshake: initialize →  notifications/initialized
        let id = conn.next_id();
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "rust-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        conn.send(&init_req).await?;
        let resp = conn.recv().await?;

        if resp.get("error").is_some() {
            bail!(
                "MCP server '{}' initialize error: {}",
                entry.name,
                resp["error"]
            );
        }

        // Confirm initialization
        let notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        conn.send(&notif).await?;

        Ok(conn)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn send(&mut self, msg: &Value) -> Result<()> {
        let line = serde_json::to_string(msg)?;
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Value> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).await?;
            if n == 0 {
                bail!("MCP server stdout closed unexpectedly");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue; // skip blank lines
            }
            return serde_json::from_str(trimmed)
                .with_context(|| format!("invalid JSON from MCP server: {}", trimmed));
        }
    }

    /// Send a `tools/list` request and return raw tool definitions.
    async fn list_tools(&mut self) -> Result<Vec<Value>> {
        let id = self.next_id();
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list"
        });
        self.send(&req).await?;
        let resp = self.recv().await?;

        if let Some(err) = resp.get("error") {
            bail!("tools/list error: {}", err);
        }

        Ok(resp["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    /// Send a `tools/call` request and return the result text.
    async fn call_tool(
        &mut self,
        original_name: &str,
        arguments: &Value,
    ) -> Result<(String, bool)> {
        let id = self.next_id();
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": original_name,
                "arguments": arguments
            }
        });
        self.send(&req).await?;
        let resp = self.recv().await?;

        if let Some(err) = resp.get("error") {
            return Ok((format!("MCP error: {}", err), true));
        }

        let is_error = resp["result"]["isError"].as_bool().unwrap_or(false);
        let text = resp["result"]["content"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .filter_map(|block| {
                        if block["type"] == "text" {
                            block["text"].as_str().map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .reduce(|a, b| a + "\n" + &b)
            })
            .unwrap_or_default();

        Ok((text, is_error))
    }
}

// ── HTTP + SSE transport ──────────────────────────────────────────────────────

/// Pending request: request_id → oneshot sender waiting for the JSON-RPC response.
type PendingMap = Arc<std::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>;

/// MCP client over HTTP POST + Server-Sent Events.
///
/// The server is expected to implement the MCP HTTP+SSE transport:
///   `GET  <base>/sse`     — SSE stream; first event is `endpoint` with the POST URL.
///   `POST <post_url>`     — submit a JSON-RPC request; actual response arrives via SSE.
struct McpHttpConnection {
    post_url: String,
    http:     reqwest::Client,
    pending:  PendingMap,
    next_id:  u64,
    /// Keep the background SSE task alive.
    _sse_task: tokio::task::JoinHandle<()>,
}

impl McpHttpConnection {
    async fn connect(entry: &McpServerEntry, base_url: &str) -> Result<Self> {
        // Build reqwest client with any configured default headers.
        let mut header_map = reqwest::header::HeaderMap::new();
        for (k, v) in &entry.headers {
            let name  = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                .with_context(|| format!("invalid MCP header name: {}", k))?;
            let value = reqwest::header::HeaderValue::from_str(v)
                .with_context(|| format!("invalid MCP header value for '{}'", k))?;
            header_map.insert(name, value);
        }
        let http = reqwest::Client::builder()
            .default_headers(header_map)
            .build()
            .context("failed to build HTTP client for MCP server")?;

        let pending: PendingMap = Arc::new(std::sync::Mutex::new(HashMap::new()));

        // Oneshot channel to receive the POST endpoint URL from the SSE `endpoint` event.
        let (ep_tx, ep_rx) = tokio::sync::oneshot::channel::<String>();
        let ep_tx_slot: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>> =
            Arc::new(std::sync::Mutex::new(Some(ep_tx)));

        // If the URL already ends with /sse (e.g. "http://host:8765/sse"), use it directly.
        let url_clean = base_url.trim_end_matches('/');
        let sse_url = if url_clean.ends_with("/sse") {
            url_clean.to_string()
        } else {
            format!("{}/sse", url_clean)
        };
        // Base URL without the /sse suffix — used for fallback POST endpoint and relative path resolution.
        let base_origin = url_clean.trim_end_matches("/sse").to_string();
        let _sse_task = tokio::spawn(run_sse_task(
            http.clone(),
            sse_url,
            base_origin.clone(),
            pending.clone(),
            ep_tx_slot,
            entry.name.clone(),
        ));

        // Wait briefly for the server to announce its POST URL via `endpoint` SSE event.
        // Many servers (e.g. FastMCP / uvicorn-based) don't send this event and expect
        // clients to POST to `<base>/message` by convention — fall back to that if no
        // endpoint event arrives within 2 seconds.
        let post_url = match tokio::time::timeout(std::time::Duration::from_secs(2), ep_rx).await {
            Ok(Ok(url)) => url,
            _ => {
                tracing::debug!(
                    "MCP server '{}': no `endpoint` SSE event received, falling back to {}/message",
                    entry.name, base_origin
                );
                format!("{}/message", base_origin)
            }
        };

        let mut conn = McpHttpConnection { post_url, http, pending, next_id: 1, _sse_task };

        // MCP initialize handshake.
        let id = conn.next_id();
        let init_req = json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "rust-agent", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let resp = conn.send_and_recv(&init_req).await?;
        if resp.get("error").is_some() {
            bail!("MCP server '{}' initialize error: {}", entry.name, resp["error"]);
        }
        // Confirmed notification — fire-and-forget, no response expected.
        conn.post(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await?;

        Ok(conn)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// POST a JSON-RPC message; for requests the response arrives via SSE.
    async fn post(&self, msg: &Value) -> Result<()> {
        let resp = self.http.post(&self.post_url).json(msg).send().await
            .context("MCP HTTP POST failed")?;
        // MCP servers may return 200, 202, or 204 — all are acceptable.
        if !resp.status().is_success() {
            bail!("MCP HTTP POST returned {}", resp.status());
        }
        Ok(())
    }

    /// POST a request and await the matching response via SSE (matched by `id`).
    async fn send_and_recv(&mut self, req: &Value) -> Result<Value> {
        let id = req["id"].as_u64().context("request missing id")?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.post(req).await?;
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .context("MCP HTTP request timed out")?
            .context("SSE task dropped before responding")
    }

    async fn list_tools(&mut self) -> Result<Vec<Value>> {
        let id  = self.next_id();
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list" });
        let resp = self.send_and_recv(&req).await?;
        if let Some(err) = resp.get("error") {
            bail!("tools/list error: {}", err);
        }
        Ok(resp["result"]["tools"].as_array().cloned().unwrap_or_default())
    }

    async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<(String, bool)> {
        let id  = self.next_id();
        let req = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        });
        let resp = self.send_and_recv(&req).await?;
        if let Some(err) = resp.get("error") {
            return Ok((format!("MCP error: {}", err), true));
        }
        let is_error = resp["result"]["isError"].as_bool().unwrap_or(false);
        let text = resp["result"]["content"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        if b["type"] == "text" { b["text"].as_str().map(|s| s.to_string()) } else { None }
                    })
                    .reduce(|a, b| a + "\n" + &b)
            })
            .unwrap_or_default();
        Ok((text, is_error))
    }
}

/// Background task: reads the SSE stream from `sse_url`, routes JSON-RPC
/// responses to pending callers, and forwards notifications to `push_service_event`.
async fn run_sse_task(
    http:        reqwest::Client,
    sse_url:     String,
    base_url:    String,   // used to resolve relative endpoint paths
    pending:     PendingMap,
    ep_tx_slot:  Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    source_name: String,
) {
    const BACKOFF: std::time::Duration = std::time::Duration::from_secs(5);
    loop {
        let resp = match http.get(&sse_url).send().await {
            Ok(r)  => r,
            Err(e) => {
                eprintln!("[mcp-sse] '{}' connect failed: {} — retrying", source_name, e);
                tokio::time::sleep(BACKOFF).await;
                continue;
            }
        };
        let mut stream = resp.bytes_stream();
        let mut buf        = String::new();
        let mut event_type = String::new();

        while let Some(Ok(chunk)) = stream.next().await {
            // Normalise CRLF → LF so that `\n\n` reliably marks event boundaries
            // regardless of whether the server (e.g. uvicorn) uses \r\n line endings.
            let text = String::from_utf8_lossy(&chunk);
            buf.push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));

            // SSE events are delimited by a blank line (\n\n).
            while let Some(end) = buf.find("\n\n") {
                let block = buf[..end].to_string();
                buf = buf[end + 2..].to_string();

                let mut data = String::new();
                for line in block.lines() {
                    if let Some(t) = line.strip_prefix("event: ") {
                        event_type = t.trim().to_string();
                    } else if let Some(d) = line.strip_prefix("data: ") {
                        data = d.trim().to_string();
                    }
                }

                match event_type.as_str() {
                    "endpoint" => {
                        // Server tells us where to POST requests.
                        // `data` may be an absolute URL or a relative path.
                        let resolved = if data.starts_with("http://") || data.starts_with("https://") {
                            data.clone()
                        } else {
                            format!("{}{}", base_url.trim_end_matches('/'), data)
                        };
                        if let Ok(mut slot) = ep_tx_slot.lock() {
                            if let Some(tx) = slot.take() {
                                let _ = tx.send(resolved);
                            }
                        }
                    }
                    "message" | "" if !data.is_empty() => {
                        if let Ok(msg) = serde_json::from_str::<Value>(&data) {
                            if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                                // Response to a pending call — wake the caller.
                                if let Ok(mut map) = pending.lock() {
                                    if let Some(tx) = map.remove(&id) {
                                        let _ = tx.send(msg);
                                    }
                                }
                            } else {
                                // Server-initiated notification.
                                let method  = msg["method"].as_str().unwrap_or("notification");
                                let content = serde_json::to_string_pretty(&msg["params"])
                                    .unwrap_or_else(|_| msg.to_string());
                                push_service_event(
                                    &source_name,
                                    NotifyLevel::Info,
                                    format!("[{}] {}", method, content),
                                );
                            }
                        }
                    }
                    _ => {}
                }
                event_type.clear();
            }
        }

        eprintln!("[mcp-sse] '{}' stream ended — reconnecting", source_name);
        tokio::time::sleep(BACKOFF).await;
    }
}

// ── McpClientTool: one registered tool from an external MCP server ────────────

/// A tool backed by an external MCP server.  All tools from the *same* server
/// share one `McpTransport` behind an `Arc<Mutex<_>>` to serialise requests.
pub struct McpClientTool {
    definition: ToolDefinition,
    /// Name the MCP server actually uses (without our prefix).
    original_name: String,
    connection: Arc<Mutex<McpTransport>>,
}

#[async_trait]
impl Tool for McpClientTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    async fn execute(&self, input: &Value, _project_dir: &Path) -> ToolResult {
        let mut conn = self.connection.lock().await;
        match conn.call_tool(&self.original_name, input).await {
            Ok((text, is_error)) => {
                if is_error { ToolResult::error(text) } else { ToolResult::success(text) }
            }
            Err(e) => ToolResult::error(format!("MCP call failed: {}", e)),
        }
    }
}

// ── MCP 2025 Streamable HTTP transport ───────────────────────────────────────
//
// Spec: https://modelcontextprotocol.io/specification/2025-03-26/basic/transports
//
// A single HTTP endpoint (e.g. `/mcp`) handles both directions:
//   POST <endpoint>  Content-Type: application/json
//                    Accept: application/json, text/event-stream
//     → 200 application/json        — direct JSON-RPC response
//     → 200 text/event-stream       — inline SSE stream (one or more events)
//
// Session management: on the `initialize` response the server may return an
// `Mcp-Session-Id` header; subsequent requests must echo it back.
//
// Auto-detection in `connect_server()`:
//   1. POST the `initialize` request to the URL as-is.
//   2. If the server responds with 200 → Streamable HTTP (this struct).
//   3. If the server responds with 404 / 405 → fall back to legacy HTTP+SSE.

struct McpStreamableHttpConnection {
    endpoint: String,
    http: reqwest::Client,
    session_id: Option<String>,
    next_id: u64,
}

impl McpStreamableHttpConnection {
    /// Try to connect using the Streamable HTTP transport.
    ///
    /// Returns `Ok(conn)` if the server speaks the new protocol,
    /// `Err(..)` if the server returned 404/405 (caller should try legacy SSE).
    async fn connect(entry: &McpServerEntry, url: &str) -> Result<Self> {
        let mut header_map = reqwest::header::HeaderMap::new();
        for (k, v) in &entry.headers {
            let name  = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                .with_context(|| format!("invalid MCP header name: {}", k))?;
            let value = reqwest::header::HeaderValue::from_str(v)
                .with_context(|| format!("invalid MCP header value for '{}'", k))?;
            header_map.insert(name, value);
        }
        let http = reqwest::Client::builder()
            .default_headers(header_map)
            .build()
            .context("failed to build HTTP client")?;

        let mut conn = McpStreamableHttpConnection {
            endpoint: url.trim_end_matches('/').to_string(),
            http,
            session_id: None,
            next_id: 1,
        };

        let id = conn.next_id();
        let init_req = json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "rust-agent", "version": env!("CARGO_PKG_VERSION") }
            }
        });

        let resp = conn.post_raw(&init_req).await?;
        let status = resp.status();

        // 404 / 405 → server doesn't speak this protocol; signal caller to fall back.
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            bail!("__fallback__: server returned {}", status);
        }
        if !status.is_success() {
            bail!("Streamable HTTP initialize failed with status {}", status);
        }

        // Extract optional session id.
        if let Some(sid) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            conn.session_id = Some(sid.to_string());
        }

        let result = conn.read_response_body(resp).await?;
        if result.get("error").is_some() {
            bail!("MCP initialize error: {}", result["error"]);
        }

        // Confirmed notification.
        let _ = conn.post_raw(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;

        Ok(conn)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// POST a JSON-RPC message and return the raw response (status + headers + body).
    async fn post_raw(&self, msg: &Value) -> Result<reqwest::Response> {
        let mut req = self.http
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(msg);
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        req.send().await.context("MCP HTTP POST failed")
    }

    /// Read the response body and extract the first JSON-RPC response object.
    ///
    /// The server may respond with:
    ///   - `application/json`   → body is the response directly.
    ///   - `text/event-stream`  → one or more `data: <json>` lines; we return
    ///                            the first object that has an `id` field (the
    ///                            actual response), ignoring pure notifications.
    async fn read_response_body(&self, resp: reqwest::Response) -> Result<Value> {
        let ct = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();

        if ct.contains("text/event-stream") {
            // Parse the inline SSE stream and return the first response message.
            let text = resp.text().await.context("reading SSE response body")?;
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" { continue; }
                    if let Ok(v) = serde_json::from_str::<Value>(data) {
                        // Return the first message that looks like a JSON-RPC response.
                        if v.get("id").is_some() || v.get("result").is_some() || v.get("error").is_some() {
                            return Ok(v);
                        }
                    }
                }
            }
            bail!("no JSON-RPC response found in SSE stream");
        } else {
            // Direct JSON response.
            resp.json::<Value>().await.context("reading JSON response body")
        }
    }

    /// POST a request and return the JSON-RPC response value.
    async fn send_and_recv(&mut self, req: &Value) -> Result<Value> {
        let resp = self.post_raw(req).await?;
        if !resp.status().is_success() {
            bail!("MCP HTTP POST returned {}", resp.status());
        }
        self.read_response_body(resp).await
    }

    async fn list_tools(&mut self) -> Result<Vec<Value>> {
        let id  = self.next_id();
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list" });
        let resp = self.send_and_recv(&req).await?;
        if let Some(err) = resp.get("error") { bail!("tools/list error: {}", err); }
        Ok(resp["result"]["tools"].as_array().cloned().unwrap_or_default())
    }

    async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<(String, bool)> {
        let id  = self.next_id();
        let req = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        });
        let resp = self.send_and_recv(&req).await?;
        if let Some(err) = resp.get("error") {
            return Ok((format!("MCP error: {}", err), true));
        }
        let is_error = resp["result"]["isError"].as_bool().unwrap_or(false);
        let text = resp["result"]["content"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .filter_map(|b| if b["type"] == "text" { b["text"].as_str().map(|s| s.to_string()) } else { None })
                    .reduce(|a, b| a + "\n" + &b)
            })
            .unwrap_or_default();
        Ok((text, is_error))
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Connect to a list of MCP server entries provided directly (no config file read).
///
/// Returns `(tools, errors)` — tools ready to register, plus human-readable
/// error strings for any servers that failed to connect (so the caller can
/// report them back to the client without aborting the others).
pub async fn connect_from_entries(
    entries: &[McpServerEntry],
) -> (Vec<Box<dyn Tool + Send + Sync>>, Vec<String>) {
    let mut tools: Vec<Box<dyn Tool + Send + Sync>> = vec![];
    let mut errors: Vec<String> = vec![];

    for entry in entries {
        match connect_server(entry).await {
            Ok(server_tools) => {
                tracing::info!(
                    "MCP client: connected to '{}', {} tool(s) registered",
                    entry.name,
                    server_tools.len()
                );
                tools.extend(server_tools);
            }
            Err(e) => {
                let msg = format!("server '{}': {:#}", entry.name, e);
                tracing::warn!("MCP client: skipping {}", msg);
                eprintln!("[mcp] skipping {}", msg);
                errors.push(msg);
            }
        }
    }

    (tools, errors)
}

/// Spawn all configured MCP servers, complete handshakes, list their tools,
/// and return a flat `Vec<Box<dyn Tool>>` ready to register in `ToolExecutor`.
///
/// Servers that fail to start or respond are logged to `stderr` and skipped
/// so a broken MCP server never prevents the agent from starting.
pub async fn connect_all(project_dir: &PathBuf) -> Vec<Box<dyn Tool + Send + Sync>> {
    let cfg = load_config(project_dir);
    if cfg.servers.is_empty() {
        return vec![];
    }

    let mut tools: Vec<Box<dyn Tool + Send + Sync>> = vec![];

    for entry in &cfg.servers {
        match connect_server(entry).await {
            Ok(server_tools) => {
                tracing::info!(
                    "MCP client: connected to '{}', {} tool(s) registered",
                    entry.name,
                    server_tools.len()
                );
                tools.extend(server_tools);
            }
            Err(e) => {
                tracing::warn!("MCP client: skipping server '{}': {}", entry.name, e);
                eprintln!("[mcp] skipping '{}': {}", entry.name, e);
            }
        }
    }

    tools
}

async fn connect_server(
    entry: &McpServerEntry,
) -> Result<Vec<Box<dyn Tool + Send + Sync>>> {
    // Dispatch to the appropriate transport.
    let transport: McpTransport = if let Some(base_url) = &entry.url {
        // ── HTTP transport: auto-detect protocol version ──────────────────
        //
        // 1. Try new "Streamable HTTP" (MCP 2025): POST initialize directly to
        //    the configured URL.  If the server responds 200 → use this.
        // 2. If the server returns 404 / 405 → the URL is likely a base URL for
        //    the legacy HTTP+SSE protocol; connect to <base>/sse instead.
        let url_clean = base_url.trim_end_matches('/');

        // Only attempt Streamable HTTP if the URL does NOT already look like a
        // legacy SSE endpoint (i.e., doesn't end in /sse or /messages).
        let skip_streamable = url_clean.ends_with("/sse") || url_clean.ends_with("/messages");

        if !skip_streamable {
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                McpStreamableHttpConnection::connect(entry, url_clean),
            ).await {
                Ok(Ok(conn)) => {
                    tracing::info!(
                        "MCP server '{}': using Streamable HTTP transport (2025)",
                        entry.name
                    );
                    McpTransport::StreamableHttp(conn)
                }
                Ok(Err(e)) if e.to_string().starts_with("__fallback__") => {
                    // Server explicitly rejected with 404/405 → fall through to legacy SSE.
                    tracing::debug!(
                        "MCP server '{}': Streamable HTTP not supported, falling back to legacy SSE",
                        entry.name
                    );
                    connect_legacy_sse(entry, url_clean).await?
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => bail!("timed out connecting to MCP server '{}' (Streamable HTTP)", entry.name),
            }
        } else {
            // URL explicitly points to /sse — use legacy protocol directly.
            connect_legacy_sse(entry, url_clean).await?
        }
    } else {
        // stdio transport.
        if entry.command.is_empty() {
            bail!("MCP server '{}': either 'command' (stdio) or 'url' (HTTP) must be set", entry.name);
        }
        let conn = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            McpConnection::spawn(entry),
        )
        .await
        .with_context(|| format!("timed out connecting to MCP server '{}'", entry.name))??;
        McpTransport::Stdio(conn)
    };

    let mut transport = transport;
    let raw_tools  = transport.list_tools().await?;
    let conn_arc   = Arc::new(Mutex::new(transport));

    let tools: Vec<Box<dyn Tool + Send + Sync>> = raw_tools
        .into_iter()
        .filter_map(|t| {
            let original_name = t["name"].as_str()?.to_string();
            let description   = t["description"].as_str().unwrap_or("").to_string();
            let parameters    = t
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            let prefixed_name = format!("{}__{}", entry.name, original_name);

            Some(Box::new(McpClientTool {
                definition: ToolDefinition {
                    name:        prefixed_name,
                    description: format!("[{}] {}", entry.name, description),
                    parameters,
                },
                original_name,
                connection: conn_arc.clone(),
            }) as Box<dyn Tool + Send + Sync>)
        })
        .collect();

    Ok(tools)
}

/// Connect using the legacy HTTP+SSE transport (MCP pre-2025).
async fn connect_legacy_sse(entry: &McpServerEntry, url_clean: &str) -> Result<McpTransport> {
    let conn = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        McpHttpConnection::connect(entry, url_clean),
    )
    .await
    .with_context(|| format!("timed out connecting to MCP server '{}' (legacy SSE)", entry.name))??;
    tracing::info!("MCP server '{}': using legacy HTTP+SSE transport", entry.name);
    Ok(McpTransport::Http(conn))
}
