//! Tool: connect_service
//!
//! Registers and connects to a named external service.
//! The connection is kept alive in the global `SERVICES` singleton for the
//! duration of the session.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::service::{HttpServiceClient, WsServiceClient, SERVICES};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct ConnectServiceTool;

#[derive(Serialize, Deserialize)]
struct ConnectServiceInput {
    /// Identifier for this service (used in subsequent query_service calls).
    name: String,
    /// Service URL.  `ws://` or `wss://` → WebSocket client.
    /// `http://` or `https://` → HTTP client.
    url: String,
    /// Protocol hint: "ws" (default) or "http".
    #[serde(default = "default_protocol")]
    protocol: String,
}

fn default_protocol() -> String {
    "ws".to_string()
}

#[async_trait]
impl Tool for ConnectServiceTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "connect_service".to_string(),
            description: "Register and connect to a simple external service such as a local \
                          model server, notification gateway, or REST API.  Use ws:// for \
                          WebSocket services and http:// for REST APIs.  The connection persists \
                          for the session so you only need to call this once per service.\n\
                          \n\
                          IMPORTANT: Do NOT use this for agent servers (agent --mode server). \
                          Use call_node instead, which implements the correct agent-to-agent \
                          protocol (ready handshake, user_message, streaming events, confirmations)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Short identifier for this service (e.g. 'oracle', 'ci', 'weather')."
                    },
                    "url": {
                        "type": "string",
                        "description": "Service address, e.g. ws://localhost:8888 or http://localhost:11434"
                    },
                    "protocol": {
                        "type": "string",
                        "enum": ["ws", "http"],
                        "description": "Transport protocol. Defaults to 'ws'. Use 'http' for REST APIs.",
                        "default": "ws"
                    }
                },
                "required": ["name", "url"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let params: ConnectServiceInput = match serde_json::from_value(input.clone()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Auto-infer protocol from URL scheme if not explicitly set.
        let protocol = if params.url.starts_with("http://") || params.url.starts_with("https://") {
            "http"
        } else {
            params.protocol.as_str()
        };

        let client: Box<dyn crate::service::ServiceClient> = match protocol {
            "http" | "https" => Box::new(HttpServiceClient::new(&params.url)),
            _ => Box::new(WsServiceClient::new(&params.url)),
        };

        let mut mgr = SERVICES.lock().await;
        mgr.register(&params.name, client);
        match mgr.connect(&params.name).await {
            Ok(()) => ToolResult::success(format!(
                "Connected to service '{}' at {} (protocol: {}).",
                params.name, params.url, protocol
            )),
            Err(e) => ToolResult::error(format!(
                "Failed to connect to service '{}': {}", params.name, e
            )),
        }
    }
}
