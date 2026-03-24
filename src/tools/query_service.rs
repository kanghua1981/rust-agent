//! Tool: query_service
//!
//! Sends a question to a named service registered via `connect_service` and
//! returns the answer.  Requests are serialised through a per-service semaphore
//! so a single-client service (e.g. a local model) is never over-subscribed.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::service::SERVICES;
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct QueryServiceTool;

#[derive(Serialize, Deserialize)]
struct QueryServiceInput {
    /// Name of the service to query (must have been connected via connect_service).
    service_name: String,
    /// The question or prompt to send to the service.
    question: String,
    /// Timeout in seconds (default 30).
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 { 30 }

#[async_trait]
impl Tool for QueryServiceTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "query_service".to_string(),
            description: "Send a question to a named external service and return its answer.  \
                          The service must have been registered first with connect_service.  \
                          This is for simple single-round-trip services (model servers, REST APIs) \
                          — NOT for agent servers. Use call_node to delegate tasks to another agent."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "service_name": {
                        "type": "string",
                        "description": "Identifier of the service to query (from connect_service)."
                    },
                    "question": {
                        "type": "string",
                        "description": "The question or prompt to send to the service."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Seconds to wait for a response (default 30).",
                        "default": 30
                    }
                },
                "required": ["service_name", "question"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let params: QueryServiceInput = match serde_json::from_value(input.clone()) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid input: {}", e)),
        };

        let mgr = SERVICES.lock().await;

        // Verify the service is registered.
        if !mgr.list().iter().any(|(n, _, _)| n == &params.service_name) {
            return ToolResult::error(format!(
                "Unknown service '{}'. Use connect_service to register it first.\n\
                 Available services: {}",
                params.service_name,
                {
                    let names: Vec<String> = mgr.list().iter()
                        .map(|(n, _, _)| n.clone())
                        .collect();
                    if names.is_empty() {
                        "(none)".to_string()
                    } else {
                        names.join(", ")
                    }
                }
            ));
        }

        match mgr.query(&params.service_name, &params.question, params.timeout_secs).await {
            Ok(answer) => ToolResult::success(format!(
                "[{}] {}", params.service_name, answer
            )),
            Err(e) => ToolResult::error(format!(
                "Service '{}' query failed: {}", params.service_name, e
            )),
        }
    }
}
