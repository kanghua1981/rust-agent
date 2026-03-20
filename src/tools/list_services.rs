//! Tool: list_services
//!
//! Lists all registered service connections and active subscriptions with their
//! current status.  Useful for diagnosing what services are available before
//! calling query_service or subscribe_service.

use std::path::Path;

use async_trait::async_trait;
use serde_json::json;

use crate::service::{SERVICES, SUBSCRIPTIONS};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct ListServicesTool;

#[async_trait]
impl Tool for ListServicesTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_services".to_string(),
            description: "List all registered services (connect_service) and active push \
                          subscriptions (subscribe_service) with their connection status."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, _input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let mgr = SERVICES.lock().await;
        let connections = mgr.list();

        let subs: Vec<String> = SUBSCRIPTIONS
            .lock()
            .map(|m| {
                m.iter()
                    .map(|(name, handle)| {
                        let status = if handle.is_finished() { "stopped" } else { "running" };
                        format!("  {} [{}]", name, status)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut out = String::new();

        out.push_str("── Query Connections ───────────────────────────────\n");
        if connections.is_empty() {
            out.push_str("  (none — use connect_service to register)\n");
        } else {
            for (name, url, connected) in &connections {
                let status = if *connected { "✓ connected" } else { "✗ not connected" };
                out.push_str(&format!("  {} [{}]  {}\n", name, status, url));
            }
        }

        out.push_str("\n── Push Subscriptions ──────────────────────────────\n");
        if subs.is_empty() {
            out.push_str("  (none — use subscribe_service to start)\n");
        } else {
            for s in &subs {
                out.push_str(s);
                out.push('\n');
            }
        }

        ToolResult::success(out)
    }
}
