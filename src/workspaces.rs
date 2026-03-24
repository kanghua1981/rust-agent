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

use serde::{Deserialize, Serialize};

// ── Config structs ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ClusterConfig {
    pub token: Option<String>,
}

/// A local project directory declared in `[[workspace]]`.
#[derive(Debug, Serialize, Deserialize, Clone)]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub url: String,
}

// ── Capability structs (serialised into the ready frame) ──────────────────────

/// GPU descriptor from `nvidia-smi`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
}

/// Hardware + software capabilities probed at worker startup.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeCapabilities {
    pub arch: String,
    pub os: String,
    pub cpu_cores: usize,
    pub ram_gb: u64,
    #[serde(default)]
    pub gpus: Vec<GpuInfo>,
    /// Available commands from the well-known candidate list.
    #[serde(default)]
    pub bins: Vec<String>,
}

/// Per-workspace info shipped in the ready frame so the manager LLM can route
/// tasks to the right virtual node.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VirtualNodeInfo {
    pub name: String,
    pub workdir: String,
    #[serde(default)]
    pub description: String,
    /// Isolation mode string: "normal" | "container" | "sandbox".
    /// Takes precedence over the legacy `sandbox` bool when present.
    #[serde(default)]
    pub isolation: Option<String>,
    /// Legacy field kept for backward compatibility with older server responses.
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ── Well-known bin candidates ─────────────────────────────────────────────────

pub const BIN_CANDIDATES: &[&str] = &[
    "git", "docker", "podman", "python3", "python", "pip3", "uv",
    "node", "npm", "yarn", "pnpm", "bun",
    "cargo", "rustc", "rustup",
    "gcc", "g++", "clang", "clang++", "make", "cmake", "ninja",
    "nvcc", "nvidia-smi",
    "kubectl", "helm", "terraform", "ansible",
    "go", "java", "mvn", "gradle",
    "zig", "nix",
];

// ── Probe functions ───────────────────────────────────────────────────────────

/// Check which of `candidates` exist on PATH.
pub fn probe_bins(candidates: &[&str]) -> Vec<String> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let path_dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path_var).collect();
    candidates
        .iter()
        .filter(|&&bin| {
            path_dirs.iter().any(|dir| {
                let full = dir.join(bin);
                full.is_file() || full.exists()
            })
        })
        .map(|&bin| bin.to_string())
        .collect()
}

/// Read total system RAM from `/proc/meminfo` (Linux) in GiB.
pub fn probe_ram_gb() -> u64 {
    if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            if line.starts_with("MemTotal:") {
                let kb: u64 = line.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return kb / 1024 / 1024;
            }
        }
    }
    0
}

/// Probe GPU names via `nvidia-smi` (returns empty vec if not available).
pub fn probe_gpus() -> Vec<GpuInfo> {
    let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| GpuInfo { name: l.trim().to_string() })
        .filter(|g| !g.name.is_empty())
        .collect()
}

/// Probe hardware capabilities and build `VirtualNodeInfo` list from workspace
/// entries.  Returns `(caps, virtual_nodes)`.
pub fn probe_capabilities(workspaces: &[WorkspaceEntry]) -> (NodeCapabilities, Vec<VirtualNodeInfo>) {
    let bins = probe_bins(BIN_CANDIDATES);
    let gpus = probe_gpus();
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let ram_gb = probe_ram_gb();

    let caps = NodeCapabilities {
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        cpu_cores,
        ram_gb,
        gpus,
        bins,
    };

    let virtual_nodes: Vec<VirtualNodeInfo> = workspaces
        .iter()
        .map(|w| VirtualNodeInfo {
            name: w.name.clone(),
            workdir: w.workdir.display().to_string(),
            description: w.description.clone(),
            isolation: None,
            sandbox: w.sandbox,
            tags: w.tags.clone(),
        })
        .collect();

    (caps, virtual_nodes)
}

// ── In-process route table (tag → remote node) ───────────────────────────────

/// A resolved route entry: a specific virtual node on a physical server.
#[derive(Debug, Clone)]
pub struct RouteEntry {
    /// Name of the `[[remote]]` entry in workspaces.toml.
    pub server_name: String,
    /// WebSocket URL of the physical server.
    pub server_url: String,
    /// Virtual workspace name on that server.
    pub node_name: String,
    pub workdir: String,
    pub sandbox: bool,
    pub tags: Vec<String>,
}

// Global in-process route table, populated by `/nodes` probes and call_node
// ready frames.  Entries keyed by (server_name, node_name).
static ROUTE_TABLE: once_cell::sync::Lazy<std::sync::RwLock<Vec<RouteEntry>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(Vec::new()));

/// Replace all route entries for `server_name` with the supplied virtual nodes.
pub fn update_route_table(server_name: &str, server_url: &str, virtual_nodes: &[VirtualNodeInfo]) {
    let Ok(mut table) = ROUTE_TABLE.write() else { return };
    table.retain(|e| e.server_name != server_name);
    for vn in virtual_nodes {
        table.push(RouteEntry {
            server_name: server_name.to_string(),
            server_url: server_url.to_string(),
            node_name: vn.name.clone(),
            workdir: vn.workdir.clone(),
            sandbox: vn.sandbox,
            tags: vn.tags.clone(),
        });
    }
}

/// Return the first route entry whose tags contain `tag`.
pub fn find_by_tag(tag: &str) -> Option<RouteEntry> {
    let Ok(table) = ROUTE_TABLE.read() else { return None };
    table.iter().find(|e| e.tags.iter().any(|t| t == tag)).cloned()
}

/// Return all route entries whose tags contain `tag`.
pub fn find_all_by_tag(tag: &str) -> Vec<RouteEntry> {
    let Ok(table) = ROUTE_TABLE.read() else { return vec![] };
    table.iter().filter(|e| e.tags.iter().any(|t| t == tag)).cloned().collect()
}

/// Return a snapshot of the entire route table.
pub fn get_route_table() -> Result<Vec<RouteEntry>, ()> {
    ROUTE_TABLE.read().map(|g| g.clone()).map_err(|_| ())
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

// ── URL helpers ─────────────────────────────────────────────────────────────────

/// Ensure a WebSocket URL targets `path` (e.g. `/agent` or `/probe`).
///
/// - `ws://host:port`  → `ws://host:port/agent`
/// - `ws://host:port/` → `ws://host:port/agent`
/// - `ws://host:port/agent` → unchanged  (explicit path respected)
pub fn with_path(url: &str, path: &str) -> String {
    let authority_end = url.find("://")
        .map(|i| {
            let a = i + 3;
            url[a..].find('/').map(|j| a + j).unwrap_or(url.len())
        })
        .unwrap_or(url.len());
    let existing = &url[authority_end..];
    if existing.is_empty() || existing == "/" {
        format!("{}{}", &url[..authority_end], path)
    } else {
        url.to_string()
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
