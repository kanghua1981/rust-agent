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
                "No nodes configured.\n\
                 Add [[node]] entries (local) or [[peer]] entries (remote servers) \
                 to ~/.config/rust_agent/workspaces.toml."
                    .to_string(),
            );
        }

        // ── Separate local from remote ───────────────────────────────────────
        let local: Vec<&serde_json::Value> = nodes.iter()
            .filter(|n| n["source"].as_str() == Some("local"))
            .collect();
        let remote: Vec<&serde_json::Value> = nodes.iter()
            .filter(|n| n["source"].as_str() == Some("remote"))
            .collect();

        let mut out = String::new();

        // Local nodes.
        if !local.is_empty() {
            out.push_str("本机节点 (local)\n");
            for n in &local {
                let name    = n["name"].as_str().unwrap_or("?");
                let url     = n["url"].as_str().unwrap_or("?");
                let sandbox = if n["sandbox"].as_bool().unwrap_or(false) { "sandbox=on" } else { "sandbox=off" };
                let tags: Vec<&str> = n["tags"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let tags_str = if tags.is_empty() { String::new() } else { format!("  [{}]", tags.join(", ")) };
                out.push_str(&format!("  {:<26} {}  {}{}\n", name, sandbox, url, tags_str));
            }
            out.push('\n');
        }

        // Remote nodes — group by peer.
        if !remote.is_empty() {
            // Collect unique peer names in order of appearance.
            let mut seen_peers: Vec<&str> = vec![];
            for n in &remote {
                if let Some(p) = n["peer_name"].as_str() {
                    if !seen_peers.contains(&p) { seen_peers.push(p); }
                }
            }
            for peer in seen_peers {
                let peer_nodes: Vec<&&serde_json::Value> = remote.iter()
                    .filter(|n| n["peer_name"].as_str() == Some(peer))
                    .collect();
                // Determine peer status from its nodes.
                let all_offline = peer_nodes.iter().all(|n| n["offline"].as_bool().unwrap_or(false));
                let status_icon = if all_offline { "❌ 离线" } else { "✅ 在线" };

                out.push_str(&format!("远端节点 via {} — {}\n", peer, status_icon));
                for n in &peer_nodes {
                    let name = n["name"].as_str().unwrap_or("?");
                    if name.starts_with("(unreachable)@") { continue; }
                    let url  = n["url"].as_str().unwrap_or("?");
                    let offline = n["offline"].as_bool().unwrap_or(false);
                    let status  = if offline { " [offline]" } else { "" };
                    let tags: Vec<&str> = n["tags"].as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    let tags_str = if tags.is_empty() { String::new() } else { format!("  [{}]", tags.join(", ")) };
                    out.push_str(&format!("  {:<26} {}{}{}\n", name, url, status, tags_str));
                }
                if all_offline {
                    if let Some(n) = peer_nodes.first() {
                        if let Some(secs) = n["last_seen_secs"].as_u64() {
                            let elapsed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs().saturating_sub(secs))
                                .unwrap_or(0);
                            out.push_str(&format!("  （上次在线：{}s 前，正在重试...）\n", elapsed));
                        } else {
                            out.push_str("  （从未成功连接，正在重试...）\n");
                        }
                    }
                }
                out.push('\n');
            }
        }

        out.push_str("提示：use call_node target=\"<name>\" to delegate a task");
        ToolResult::success(out)
    }
}

fn urlenc(s: &str) -> String {
    s.bytes().flat_map(|b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            vec![b as char]
        } else {
            format!("%{:02X}", b).chars().collect()
        }
    }).collect()
}
