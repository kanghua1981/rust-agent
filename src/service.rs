//! External Service client abstraction.
//!
//! A **Service** is a long-lived external process (e.g. a local model server,
//! a notification gateway, a RAG/knowledge-base server) that the agent can
//! query while it works.  Key design constraints:
//!
//!   - **Simple only**: no multi-step pipeline, one question → one answer.
//!   - **Single queue**: a `Semaphore(1)` ensures requests are serialised so a
//!     lightweight local model is never over-subscribed.
//!   - **Named connections**: `ServiceManager` holds multiple named clients so
//!     different services (e.g. "oracle", "weather") can coexist.
//!   - **Stateful**: connections are kept alive between tool calls; reconnect
//!     logic lives inside each `ServiceClient` implementation.
//!
//! # Usage via tools
//!
//! ```text
//! connect_service  name="oracle"  url="ws://localhost:8888"  protocol="ws"
//! query_service    service_name="oracle"  question="推荐一个日志分析方案"
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

// ── ServiceClient trait ───────────────────────────────────────────────────────

/// A client to a single external service endpoint.
#[async_trait]
pub trait ServiceClient: Send + Sync {
    /// Connect (or reconnect) to the service.  Idempotent — safe to call when
    /// already connected.
    async fn connect(&mut self) -> Result<()>;

    /// Send a single query and wait for the response.
    /// The service is expected to return within `timeout` seconds.
    async fn query(&self, question: &str, timeout_secs: u64) -> Result<String>;

    /// Whether the client believes it is currently connected.
    fn is_connected(&self) -> bool;

    /// Human-readable URL / address of the service.
    fn url(&self) -> &str;

    /// Gracefully disconnect.
    async fn disconnect(&mut self);
}

// ── WebSocket ServiceClient ──────────────────────────────────────────────────

/// Simple JSON-over-WebSocket service client.
///
/// Protocol (compatible with the agent's own `--mode server`):
///   Client sends:  `{"type":"query","data":{"text":"<question>"}}`
///   Server replies: `{"type":"response","data":{"text":"<answer>"}}`
///              or: `{"type":"error","data":{"message":"<msg>"}}`
///
/// Any other `--mode server` compatible JSON frames (streaming tokens etc.)
/// are silently accumulated until a `response` or `error` frame arrives.
pub struct WsServiceClient {
    url: String,
    connected: bool,
}

impl WsServiceClient {
    pub fn new(url: impl Into<String>) -> Self {
        WsServiceClient { url: url.into(), connected: false }
    }
}

#[async_trait]
impl ServiceClient for WsServiceClient {
    async fn connect(&mut self) -> Result<()> {
        // Perform a quick connectivity check by opening and closing a WS connection.
        let (mut ws, _) = connect_async(&self.url).await
            .map_err(|e| anyhow!("Cannot connect to service at {}: {}", self.url, e))?;
        ws.close(None).await.ok();
        self.connected = true;
        Ok(())
    }

    async fn query(&self, question: &str, timeout_secs: u64) -> Result<String> {
        let (ws_stream, _) = connect_async(&self.url).await
            .map_err(|e| anyhow!("Service connection failed ({}): {}", self.url, e))?;

        let (mut write, mut read) = ws_stream.split();

        let msg = json!({
            "type": "query",
            "data": { "text": question }
        });
        write.send(Message::Text(msg.to_string().into())).await
            .map_err(|e| anyhow!("Failed to send query: {}", e))?;

        // Accumulate streaming tokens as fallback answer.
        let mut accumulated = String::new();

        let result = timeout(Duration::from_secs(timeout_secs), async {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        let ev: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                        match ev["type"].as_str() {
                            Some("response") | Some("done") | Some("final_response") => {
                                let answer = ev["data"]["text"].as_str()
                                    .or_else(|| ev["data"]["content"].as_str())
                                    .or_else(|| ev["text"].as_str())
                                    .unwrap_or("")
                                    .to_string();
                                // Prefer explicit response frame; fall back to accumulated.
                                return Ok(if answer.is_empty() { accumulated } else { answer });
                            }
                            Some("error") => {
                                let msg = ev["data"]["message"].as_str()
                                    .or_else(|| ev["message"].as_str())
                                    .unwrap_or("Unknown service error")
                                    .to_string();
                                return Err(anyhow!("Service error: {}", msg));
                            }
                            Some("streaming_token") => {
                                if let Some(token) = ev["data"]["token"].as_str() {
                                    accumulated.push_str(token);
                                }
                            }
                            Some("assistant_text") => {
                                if let Some(text) = ev["data"]["text"].as_str() {
                                    accumulated = text.to_string();
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            if accumulated.is_empty() {
                Err(anyhow!("Service closed connection without sending a response"))
            } else {
                Ok(accumulated)
            }
        })
        .await;

        write.send(Message::Close(None)).await.ok();

        match result {
            Ok(inner) => inner,
            Err(_) => bail!("Service query timed out after {}s", timeout_secs),
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn url(&self) -> &str {
        &self.url
    }

    async fn disconnect(&mut self) {
        self.connected = false;
    }
}

// ── HTTP ServiceClient ────────────────────────────────────────────────────────

/// Simple HTTP POST service client.
///
/// Sends: `POST <url>` with JSON body `{"question": "<text>"}`
/// Expects JSON response: `{"answer": "<text>"}` or `{"text": "<text>"}`
pub struct HttpServiceClient {
    url: String,
    http: reqwest::Client,
    connected: bool,
}

impl HttpServiceClient {
    pub fn new(url: impl Into<String>) -> Self {
        HttpServiceClient {
            url: url.into(),
            http: reqwest::Client::new(),
            connected: false,
        }
    }
}

#[async_trait]
impl ServiceClient for HttpServiceClient {
    async fn connect(&mut self) -> Result<()> {
        // HTTP is stateless; just verify the endpoint is reachable with a HEAD request.
        self.http.head(&self.url).send().await
            .map_err(|e| anyhow!("Cannot reach service at {}: {}", self.url, e))?;
        self.connected = true;
        Ok(())
    }

    async fn query(&self, question: &str, timeout_secs: u64) -> Result<String> {
        let body = json!({ "question": question, "text": question });
        let response = timeout(
            Duration::from_secs(timeout_secs),
            self.http.post(&self.url).json(&body).send(),
        )
        .await
        .map_err(|_| anyhow!("HTTP service query timed out after {}s", timeout_secs))?
        .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            bail!("HTTP service returned status {}", response.status());
        }

        let json: Value = response.json().await
            .map_err(|e| anyhow!("Failed to parse service JSON response: {}", e))?;

        let answer = json["answer"].as_str()
            .or_else(|| json["text"].as_str())
            .or_else(|| json["response"].as_str())
            .unwrap_or("")
            .to_string();

        if answer.is_empty() {
            bail!("Service returned empty answer");
        }
        Ok(answer)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn url(&self) -> &str {
        &self.url
    }

    async fn disconnect(&mut self) {
        self.connected = false;
    }
}

// ── ServiceManager ────────────────────────────────────────────────────────────

/// Manages a collection of named `ServiceClient` connections.
///
/// Enforces single-queue access per service via a `Semaphore(1)` so a
/// lightweight local model is never called concurrently.  If the semaphore
/// cannot be acquired within 10 seconds the call is rejected with a "busy"
/// error, allowing the agent to inform the LLM and try later.
pub struct ServiceManager {
    clients:    HashMap<String, Box<dyn ServiceClient>>,
    semaphores: HashMap<String, Arc<Semaphore>>,
}

/// Maximum time to wait for a service slot before reporting "busy".
const QUEUE_TIMEOUT: Duration = Duration::from_secs(10);

impl ServiceManager {
    pub fn new() -> Self {
        ServiceManager {
            clients:    HashMap::new(),
            semaphores: HashMap::new(),
        }
    }

    /// Register a new service client under `name`.
    /// If a client with the same name already exists it is replaced.
    pub fn register(&mut self, name: impl Into<String>, client: Box<dyn ServiceClient>) {
        let name = name.into();
        self.clients.insert(name.clone(), client);
        self.semaphores.entry(name).or_insert_with(|| Arc::new(Semaphore::new(1)));
    }

    /// Connect to a named service.  Returns error if the service is unknown.
    pub async fn connect(&mut self, name: &str) -> Result<()> {
        let client = self.clients.get_mut(name)
            .ok_or_else(|| anyhow!("Unknown service '{}'", name))?;
        client.connect().await
    }

    /// Query a named service.
    ///
    /// Acquires the per-service semaphore (single queue) before sending the
    /// request.  If the service is busy for more than 10 seconds, returns an
    /// error so the agent can retry later.
    pub async fn query(&self, name: &str, question: &str, timeout_secs: u64) -> Result<String> {
        let sem = self.semaphores.get(name)
            .ok_or_else(|| anyhow!("Unknown service '{}'", name))?
            .clone();

        let _permit = timeout(QUEUE_TIMEOUT, sem.acquire_owned())
            .await
            .map_err(|_| anyhow!("Service '{}' is busy — try again in a moment", name))?
            .map_err(|_| anyhow!("Service semaphore closed for '{}'", name))?;

        let client = self.clients.get(name)
            .ok_or_else(|| anyhow!("Unknown service '{}'", name))?;

        client.query(question, timeout_secs).await
    }

    /// Return names and URLs of all registered services.
    pub fn list(&self) -> Vec<(String, String, bool)> {
        self.clients.iter()
            .map(|(name, c)| (name.clone(), c.url().to_string(), c.is_connected()))
            .collect()
    }

    /// Whether a named service exists and reports as connected.
    pub fn is_connected(&self, name: &str) -> bool {
        self.clients.get(name).map(|c| c.is_connected()).unwrap_or(false)
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Global singleton ──────────────────────────────────────────────────────────

use once_cell::sync::Lazy;
use tokio::sync::Mutex as TokioMutex;

/// An event pushed by an external service into the agent's notification stream.
#[derive(Debug, Clone)]
pub struct ServiceEvent {
    pub source:  String,
    pub level:   crate::output::NotifyLevel,
    pub message: String,
}

/// Capacity of the broadcast channel.  Old events are dropped when the buffer
/// is full (the agent is busy and a few missed notifications are acceptable).
const EVENT_CHANNEL_CAP: usize = 64;

/// Process-wide `ServiceManager` shared across all tool invocations.
///
/// Tools (`connect_service`, `query_service`) operate on this singleton so
/// connections persist between calls within the same session.
pub static SERVICES: Lazy<Arc<TokioMutex<ServiceManager>>> =
    Lazy::new(|| Arc::new(TokioMutex::new(ServiceManager::new())));

/// Global broadcast channel for service push notifications.
///
/// Any service client can write to `SERVICE_EVENT_TX`.
/// The agent main loop subscribes via `SERVICE_EVENT_TX.subscribe()` and
/// drains pending events at safe points (between tool iterations).
pub static SERVICE_EVENT_TX: Lazy<tokio::sync::broadcast::Sender<ServiceEvent>> = Lazy::new(|| {
    let (tx, _) = tokio::sync::broadcast::channel(EVENT_CHANNEL_CAP);
    tx
});

/// Push a notification into the global service event stream.
/// Silently ignores errors (no active listeners, channel full, etc.).
pub fn push_service_event(source: impl Into<String>, level: crate::output::NotifyLevel, message: impl Into<String>) {
    let _ = SERVICE_EVENT_TX.send(ServiceEvent {
        source:  source.into(),
        level,
        message: message.into(),
    });
}

// ── Background subscription registry ─────────────────────────────────────────

/// Tracks running background subscription tasks by service name.
/// Dropping a `JoinHandle` does NOT abort the task; call `abort()` explicitly.
pub static SUBSCRIPTIONS: Lazy<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// Spawn a persistent background task that keeps a WebSocket connection open to
/// `url` and fires `push_service_event` for every message it receives.
///
/// The task reconnects automatically (5 s back-off) if the connection drops.
/// Call `cancel_subscription(name)` to stop it.
///
/// Supported server push frame formats:
///   `{"type":"notification","data":{"level":"info|warning|alert","message":"..."}}`
///   `{"type":"alert","data":{"message":"..."}}`             → level = Alert
///   `{"type":"info","data":{"message":"..."}}`              → level = Info
///   `{"type":"warning","data":{"message":"..."}}`           → level = Warning
///   Any other text frame                                    → level = Info, raw text
pub fn start_subscription(name: String, url: String) -> tokio::task::JoinHandle<()> {
    let name_clone  = name.clone();
    let url_clone   = url.clone();

    tokio::spawn(async move {
        use tokio::time::sleep;
        const BACKOFF: Duration = Duration::from_secs(5);

        loop {
            match connect_async(&url_clone).await {
                Err(e) => {
                    push_service_event(
                        &name_clone,
                        crate::output::NotifyLevel::Warning,
                        &format!("Subscription connect failed: {} — retrying in {}s", e, BACKOFF.as_secs()),
                    );
                    sleep(BACKOFF).await;
                    continue;
                }
                Ok((ws_stream, _)) => {
                    push_service_event(
                        &name_clone,
                        crate::output::NotifyLevel::Info,
                        "Subscription connected.",
                    );

                    let (mut write, mut read) = ws_stream.split();

                    // Send a subscribe handshake so the server knows this is a
                    // listener (not a query client).
                    let _ = write.send(Message::Text(
                        json!({"type":"subscribe","data":{}}).to_string().into()
                    )).await;

                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(ev) = serde_json::from_str::<Value>(&text) {
                                    dispatch_push_event(&name_clone, &ev);
                                } else {
                                    // Plain-text notification
                                    push_service_event(
                                        &name_clone,
                                        crate::output::NotifyLevel::Info,
                                        text.as_str(),
                                    );
                                }
                            }
                            Ok(Message::Ping(d)) => {
                                let _ = write.send(Message::Pong(d)).await;
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }

                    push_service_event(
                        &name_clone,
                        crate::output::NotifyLevel::Warning,
                        &format!("Subscription disconnected — retrying in {}s", BACKOFF.as_secs()),
                    );
                    sleep(BACKOFF).await;
                }
            }
        }
    })
}

/// Stop a running subscription by name.  Returns false if not found.
pub fn cancel_subscription(name: &str) -> bool {
    if let Ok(mut map) = SUBSCRIPTIONS.lock() {
        if let Some(handle) = map.remove(name) {
            handle.abort();
            return true;
        }
    }
    false
}

/// Parse a pushed JSON frame and forward to `push_service_event`.
fn dispatch_push_event(source: &str, ev: &Value) {
    use crate::output::NotifyLevel;

    let level_str = ev["data"]["level"].as_str()
        .or_else(|| ev["level"].as_str())
        .unwrap_or("info");
    let level = match level_str {
        "warning" | "warn" => NotifyLevel::Warning,
        "alert"   | "error" => NotifyLevel::Alert,
        _ => NotifyLevel::Info,
    };

    let message = match ev["type"].as_str() {
        Some("notification") | Some("info") | Some("warning") | Some("alert") => {
            ev["data"]["message"].as_str()
                .or_else(|| ev["data"]["text"].as_str())
                .or_else(|| ev["message"].as_str())
                .unwrap_or("")
                .to_string()
        }
        _ => {
            // Unknown frame — use raw JSON as message at Info level
            ev.to_string()
        }
    };

    if !message.is_empty() {
        push_service_event(source, level, message);
    }
}