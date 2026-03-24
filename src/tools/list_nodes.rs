//! Tool: list_nodes
//!
//! Query the parent agent server's `/nodes` endpoint and return a formatted
//! table of all available remote nodes — both statically configured
//! `[[remote]]` entries from workspaces.toml and any dynamically discovered
//! virtual nodes from the in-process route table.
//!
//! The parent server address is inferred from the `AGENT_PARENT_PORT` and
//! `AGENT_CLUSTER_TOKEN` environment variables that the server injects into
//! every worker subprocess before fork/exec.  This works even inside a
//! filesystem-isolated container because the network namespace is shared.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct ListNodesTool;

#[async_trait]
impl Tool for ListNodesTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_nodes".to_string(),
            description: "List all remote agent nodes that are reachable from this server.\n\
                Returns both statically configured [[remote]] entries and any dynamically \
                discovered nodes from the route table.\n\
                \n\
                Call this tool BEFORE using call_node with a named target, to verify the \
                node name exists and check its URL and tags."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        // Read parent server coordinates from env vars injected at fork time.
        let port: u16 = std::env::var("AGENT_PARENT_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9527);
        let token = std::env::var("AGENT_CLUSTER_TOKEN").ok();

        let url = match &token {
            Some(tok) => format!("http://localhost:{}/nodes?token={}", port, urlenc(tok)),
            None      => format!("http://localhost:{}/nodes", port),
        };

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("Failed to build HTTP client: {}", e)),
        };

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::error(format!(
                "Failed to reach parent server at {}: {}\n\
                 (Is this agent running as a worker? \
                  list_nodes requires AGENT_PARENT_PORT to be set.)",
                url, e
            )),
        };

        if !resp.status().is_success() {
            return ToolResult::error(format!(
                "Parent server returned HTTP {} for /nodes", resp.status()
            ));
        }

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return ToolResult::error(format!("Failed to parse /nodes response: {}", e)),
        };

        let nodes = match body["nodes"].as_array() {
            Some(a) => a,
            None    => return ToolResult::error("Unexpected /nodes response format".to_string()),
        };

        if nodes.is_empty() {
            return ToolResult::success(
                "No remote nodes configured.\n\
                 Add [[remote]] entries to ~/.config/rust_agent/workspaces.toml to register nodes."
                    .to_string(),
            );
        }

        // Format as a table.
        let mut out = format!("Remote nodes on localhost:{}:\n\n", port);
        out.push_str(&format!("{:<28} {:<8} {:<40} {}\n", "NAME", "SOURCE", "URL", "TAGS"));
        out.push_str(&"─".repeat(90));
        out.push('\n');

        for node in nodes {
            let name   = node["name"].as_str().unwrap_or("?");
            let url    = node["url"].as_str().unwrap_or("?");
            let source = node["source"].as_str().unwrap_or("?");
            let tags: Vec<&str> = node["tags"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let tags_str = if tags.is_empty() { String::new() } else { format!("[{}]", tags.join(", ")) };
            out.push_str(&format!("{:<28} {:<8} {:<40} {}\n", name, source, url, tags_str));
        }

        ToolResult::success(out)
    }
}

fn urlenc(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            vec![c]
        } else {
            format!("%{:02X}", c as u32).chars().collect()
        }
    }).collect()
}
