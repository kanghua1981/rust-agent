//! Tool: subscribe_service
//!
//! Starts a background subscription to a service that supports push notifications.
//! Unlike `query_service` (one-shot request), this keeps a persistent WebSocket
//! connection open and forwards any pushed messages into the agent's notification
//! stream via `push_service_event`.
//!
//! The task survives across LLM iterations — it runs until `unsubscribe_service`
//! is called or the agent process exits.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::service::{cancel_subscription, start_subscription, SUBSCRIPTIONS};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct SubscribeServiceTool;
pub struct UnsubscribeServiceTool;

#[derive(Serialize, Deserialize)]
struct SubscribeInput {
    /// Short name for this subscription (used as event source label).
    name: String,
    /// WebSocket URL to subscribe to.
    url: String,
}

#[derive(Serialize, Deserialize)]
struct UnsubscribeInput {
    /// Name of the subscription to cancel.
    name: String,
}

#[async_trait]
impl Tool for SubscribeServiceTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "subscribe_service".to_string(),
            description: "Start a persistent background subscription to a WebSocket service that \
                          pushes notifications (alerts, status updates, model outputs).  \
                          Notifications appear with a [svc:name] prefix at safe points between \
                          agent iterations.  The subscription reconnects automatically if \
                          the connection drops.  Use unsubscribe_service to stop it."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Label for this subscription (e.g. 'ci', 'monitor', 'alerts')."
                    },
                    "url": {
                        "type": "string",
                        "description": "WebSocket URL of the notification service, e.g. ws://localhost:8888"
                    }
                },
                "required": ["name", "url"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let params: SubscribeInput = match serde_json::from_value(input.clone()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        // Cancel any existing subscription with the same name first.
        cancel_subscription(&params.name);

        let handle = start_subscription(params.name.clone(), params.url.clone());

        if let Ok(mut map) = SUBSCRIPTIONS.lock() {
            map.insert(params.name.clone(), handle);
        }

        ToolResult::success(format!(
            "Subscription '{}' started for {}.\n\
             Notifications will appear as [svc:{}] messages between agent iterations.",
            params.name, params.url, params.name
        ))
    }
}

#[async_trait]
impl Tool for UnsubscribeServiceTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "unsubscribe_service".to_string(),
            description: "Stop a background service subscription started by subscribe_service."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the subscription to cancel."
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let params: UnsubscribeInput = match serde_json::from_value(input.clone()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        if cancel_subscription(&params.name) {
            ToolResult::success(format!("Subscription '{}' cancelled.", params.name))
        } else {
            ToolResult::error(format!(
                "No active subscription named '{}'. \
                 Use list_services to see current subscriptions.",
                params.name
            ))
        }
    }
}
