//! Data models for the global database.
//!
//! All models support Serialize/Deserialize for WebSocket JSON transport.

use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

// ── Node (server-managed workspace) ─────────────────────────────────────

/// A server-managed workspace node.
///
/// Nodes live in `global.db` and are merged with `workspaces.toml` `[[node]]`
/// entries.  Unlike presets, a Node has no `server_url` — it is implicitly
/// scoped to the machine that hosts it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    pub workdir: String,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_mode: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Peer (remote agent server for discovery) ─────────────────────────────────────

/// A remote agent server that this server probes for virtual nodes.
///
/// Peers live in `global.db` (replaces `peers.toml`).  The server's background
/// probe loop reads all enabled peers and discovers their virtual nodes via
/// the `/probe` WebSocket endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Peer {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            url: String::new(),
            token: None,
            enabled: true,
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}