//! workspaces.toml — the single topology config file for every agent process.
//!
//! ```toml
//! [cluster]
//! token = "shared-secret"
//!
//! [[workspace]]           # inbound: exposed as virtual nodes via ready frame
//! name    = "firmware-bk7236"
//! workdir = "/home/user/firmware/bk7236"
//! sandbox = true
//! tags    = ["embedded", "bk7236"]
//!
//! [[remote]]              # outbound: call_node target resolution
//! name = "gpu-box"
//! url  = "ws://192.168.1.20:9527"
//! ```
//!
//! Search order: `.agent/workspaces.toml` (project-level) →
//!               `~/.config/rust_agent/workspaces.toml` (global)

use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── Config structs ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default, Clone)]
pub struct WorkspacesFile {
    #[serde(default)]
    pub cluster: ClusterConfig,

    /// `[[workspace]]` — inbound: local project dirs this server exposes.
    #[serde(default, rename = "workspace")]
    pub workspaces: Vec<WorkspaceEntry>,

    /// `[[remote]]` — outbound: remote agent servers reachable via call_node.
    #[serde(default, rename = "remote")]
    pub remote: Vec<RemoteEntry>,

    // ── Legacy keys — accepted so old configs don't break ─────────────────
    #[serde(default, rename = "nodes")]
    pub nodes_legacy: Vec<RemoteEntry>,
    #[serde(default, rename = "servers")]
    pub servers_legacy: Vec<RemoteEntry>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ClusterConfig {
    pub token: Option<String>,
}

/// A local project directory declared in `[[workspace]]`.
#[derive(Debug, Deserialize, Clone)]
pub struct WorkspaceEntry {
    pub name: String,
    pub workdir: PathBuf,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A remote agent server endpoint declared in `[[remote]]`.
#[derive(Debug, Deserialize, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub url: String,
}

// ── Helpers on WorkspacesFile ─────────────────────────────────────────────────

impl WorkspacesFile {
    /// All remote entries: `[[remote]]` first, then legacy keys.
    pub fn all_remotes(&self) -> Vec<&RemoteEntry> {
        self.remote.iter()
            .chain(self.servers_legacy.iter())
            .chain(self.nodes_legacy.iter())
            .collect()
    }

    /// Find a remote by name across all keys.
    pub fn find_remote(&self, name: &str) -> Option<&RemoteEntry> {
        self.all_remotes().into_iter().find(|r| r.name == name)
    }

    /// Cluster token, if configured.
    pub fn cluster_token(&self) -> Option<&str> {
        self.cluster.token.as_deref()
    }
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load `workspaces.toml`.  Returns an empty default if no file is found.
///
/// Search order:
/// 1. `<project_dir>/.agent/workspaces.toml`
/// 2. `~/.config/rust_agent/workspaces.toml`
pub fn load(project_dir: &Path) -> WorkspacesFile {
    // Project-level takes priority.
    let project_cfg = project_dir.join(".agent/workspaces.toml");
    if let Ok(text) = std::fs::read_to_string(&project_cfg) {
        if let Ok(cfg) = toml::from_str::<WorkspacesFile>(&text) {
            return cfg;
        }
    }
    // Global fallback.
    if let Some(home) = dirs::home_dir() {
        let global_cfg = home.join(".config/rust_agent/workspaces.toml");
        if let Ok(text) = std::fs::read_to_string(&global_cfg) {
            if let Ok(cfg) = toml::from_str::<WorkspacesFile>(&text) {
                return cfg;
            }
        }
    }
    WorkspacesFile::default()
}
