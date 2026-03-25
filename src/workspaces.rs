//! workspaces.toml — the single topology config file for every agent process.
//!
//! ```toml
//! [cluster]
//! token = "shared-secret"
//!
//! # [[node]] — a local callable node on THIS machine (workdir required)
//! [[node]]
//! name      = "firmware-bk7236"
//! workdir   = "/home/user/firmware/bk7236"
//! isolation = "sandbox"   # "normal" | "container" | "sandbox"
//! tags      = ["embedded", "bk7236"]
//!
//! # [[peer]] — a remote agent server on ANOTHER machine.
//! #   Visible only to the server process during startup probe.
//! #   LLM never sees this entry; it sees the expanded `name@alias` nodes instead.
//! [[peer]]
//! name = "gpu-box"
//! url  = "ws://192.168.1.20:9527"
//!
//! [[peer]]
//! name = "pi"
//! url  = "ws://raspberrypi.local:9527"
//! ```
//!
//! Two distinct key types:
//!   `[[node]]`  — a single callable node; has `workdir`; used by workers & LLM routing.
//!   `[[peer]]`  — a peer server address; has `url`; consumed only by the server probe loop.
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

    /// `[[node]]` — a local callable node on this machine.
    #[serde(default, rename = "node")]
    pub nodes: Vec<NodeEntry>,

    /// `[[peer]]` — a remote agent server to probe on startup.
    /// Visible to the server process only; never exposed to LLM.
    #[serde(default, rename = "peer")]
    pub peers: Vec<PeerEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ClusterConfig {
    pub token: Option<String>,
}

/// A single callable node on this machine.
/// Corresponds to a `[[node]]` entry in `workspaces.toml`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeEntry {
    pub name: String,
    /// Local project directory (required).
    #[serde(default)]
    pub workdir: Option<PathBuf>,
    #[serde(default)]
    pub description: String,
    /// Preferred: "normal" | "container" | "sandbox".
    /// When set, takes precedence over the legacy `sandbox` bool.
    #[serde(default)]
    pub isolation: Option<String>,
    /// Legacy shorthand — `sandbox = true` is equivalent to `isolation = "sandbox"`.
    /// Ignored when `isolation` is set explicitly.
    #[serde(default)]
    pub sandbox: bool,
    /// Default execution mode for this node.
    /// "simple" | "plan" | "pipeline" | "auto" (= let router decide, the default).
    #[serde(default)]
    pub exec_mode: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A peer agent server on another machine.
/// Corresponds to a `[[peer]]` entry in `workspaces.toml`.
/// Consumed only by the server's startup probe loop — never exposed to LLM.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeerEntry {
    /// Human-readable alias for this server (used as `@alias` suffix in node names).
    pub name: String,
    /// WebSocket URL of the remote agent server, e.g. `ws://192.168.1.20:9527`.
    pub url: String,
    /// Optional per-peer token override (falls back to `[cluster].token`).
    #[serde(default)]
    pub token: Option<String>,
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
    /// Default execution mode: "simple" | "plan" | "pipeline" | None (auto).
    #[serde(default)]
    pub exec_mode: Option<String>,
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

/// Probe hardware capabilities and build `VirtualNodeInfo` list from local
/// node entries.  Returns `(caps, virtual_nodes)`.
pub fn probe_capabilities(nodes: &[NodeEntry]) -> (NodeCapabilities, Vec<VirtualNodeInfo>) {
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

    let virtual_nodes: Vec<VirtualNodeInfo> = nodes
        .iter()
        .filter_map(|n| n.workdir.as_ref().map(|wd| {
            // Resolve isolation: explicit field > legacy sandbox bool > None (server default)
            let isolation = match n.isolation.as_deref() {
                Some(m) => Some(m.to_string()),
                None if n.sandbox => Some("sandbox".to_string()),
                _ => None,
            };
            VirtualNodeInfo {
                name: n.name.clone(),
                workdir: wd.display().to_string(),
                description: n.description.clone(),
                isolation,
                sandbox: n.sandbox,
                exec_mode: n.exec_mode.clone(),
                tags: n.tags.clone(),
            }
        }))
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

// ── NodeRegistry ──────────────────────────────────────────────────────────────
//
// Runtime state of all known nodes: local [[node]] entries (always online) and
// peer-expanded sub-nodes (online/offline based on probe results).
// Populated at server startup by registry_init_local() + spawn_probe_loop().
// Read by build_nodes_json() for the /nodes HTTP endpoint.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Online,
    Offline,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeStatus::Online  => write!(f, "online"),
            NodeStatus::Offline => write!(f, "offline"),
        }
    }
}

/// A single entry in the runtime NodeRegistry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Node name shown to LLM (e.g. "upper-sdk" or "模型训练@gpu-box").
    pub name: String,
    /// WebSocket URL to connect to this node.
    pub url: String,
    /// None for local nodes; Some(peer_alias) for peer-expanded nodes.
    #[serde(default)]
    pub peer_name: Option<String>,
    pub status: NodeStatus,
    /// Unix timestamp (seconds) of last successful probe.  None = never.
    #[serde(default)]
    pub last_seen_secs: Option<u64>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Isolation mode: "normal" | "container" | "sandbox".
    #[serde(default)]
    pub isolation: Option<String>,
    /// Legacy compat — derived from `isolation` on construction.
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub description: String,
    /// Absolute working directory for this node (used by call_node to set ?workdir= param).
    #[serde(default)]
    pub workdir: Option<String>,
    /// Default execution mode: "simple" | "plan" | "pipeline" | None (auto).
    #[serde(default)]
    pub exec_mode: Option<String>,
}

static NODE_REGISTRY: once_cell::sync::Lazy<std::sync::RwLock<Vec<RegistryEntry>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(Vec::new()));

/// Return current Unix timestamp in seconds (used by server probe code).
pub fn unix_now_pub() -> Option<u64> {
    unix_now()
}

fn unix_now() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Populate the registry with local `[[node]]` entries.
/// Called once at server startup; local nodes are always Online.
pub fn registry_init_local(nodes: &[NodeEntry], port: u16) {
    let entries: Vec<RegistryEntry> = nodes.iter().filter_map(|n| {
        let workdir = n.workdir.as_ref()?;
        let enc: String = workdir.display().to_string().bytes().flat_map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
                vec![b as char]
            } else {
                format!("%{:02X}", b).chars().collect()
            }
        }).collect();
        Some(RegistryEntry {
            name:          n.name.clone(),
            url:           format!("ws://localhost:{}/?workdir={}", port, enc),
            peer_name:     None,
            status:        NodeStatus::Online,
            last_seen_secs: unix_now(),
            tags:          n.tags.clone(),
            isolation:     match n.isolation.as_deref() {
                               Some(m) => Some(m.to_string()),
                               None if n.sandbox => Some("sandbox".to_string()),
                               _ => None,
                           },
            sandbox:       n.sandbox || matches!(n.isolation.as_deref(), Some("sandbox")),
            description:   n.description.clone(),
            workdir:       Some(workdir.display().to_string()),
            exec_mode:     n.exec_mode.clone(),
        })
    }).collect();

    let mut reg = NODE_REGISTRY.write().unwrap();
    // Replace local entries (keep peer entries from previous probes).
    reg.retain(|e| e.peer_name.is_some());
    reg.extend(entries);
    // Local nodes first, then peer-expanded nodes.
    reg.sort_by_key(|e| e.peer_name.is_some());
}

/// Update the registry with newly-probed sub-nodes for a peer.
/// Replaces all previous entries for that peer and marks them online.
pub fn registry_update_peer(peer_name: &str, entries: Vec<RegistryEntry>) {
    let mut reg = NODE_REGISTRY.write().unwrap();
    reg.retain(|e| e.peer_name.as_deref() != Some(peer_name));
    reg.extend(entries);
}

/// Mark all registry entries for a peer as offline.
/// If the peer has never been probed, inserts a placeholder so the user can
/// see that the peer is configured but currently unreachable.
pub fn registry_mark_peer_offline(peer_name: &str, peer_url: &str) {
    let mut reg = NODE_REGISTRY.write().unwrap();
    let has_entries = reg.iter().any(|e| e.peer_name.as_deref() == Some(peer_name));
    if has_entries {
        for e in reg.iter_mut() {
            if e.peer_name.as_deref() == Some(peer_name) {
                e.status = NodeStatus::Offline;
            }
        }
    } else {
        // First probe failed — insert a placeholder so users/tools can see it.
        reg.push(RegistryEntry {
            name:           format!("(unreachable)@{}", peer_name),
            url:            peer_url.to_string(),
            peer_name:      Some(peer_name.to_string()),
            status:         NodeStatus::Offline,
            last_seen_secs: None,
            tags:           vec![],
            isolation:      None,
            sandbox:        false,
            description:    format!("peer '{}' is unreachable", peer_name),
            workdir:        None,
            exec_mode:      None,
        });
    }
}

/// Return a snapshot of the full registry (local + peer-expanded).
pub fn registry_snapshot() -> Vec<RegistryEntry> {
    NODE_REGISTRY.read().unwrap().clone()
}

// ── Helpers on WorkspacesFile ─────────────────────────────────────────────────

impl WorkspacesFile {
    /// All local nodes declared with `[[node]]`.
    pub fn all_nodes(&self) -> Vec<NodeEntry> {
        self.nodes.clone()
    }

    /// Alias for `all_nodes()` — every `[[node]]` entry is local.
    pub fn local_nodes(&self) -> Vec<NodeEntry> {
        self.nodes.clone()
    }

    /// All peer servers declared with `[[peer]]`.
    /// These are consumed by the server probe loop only.
    pub fn all_peers(&self) -> &[PeerEntry] {
        &self.peers
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
