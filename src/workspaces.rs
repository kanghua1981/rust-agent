//! Workspace topology — hardware probing, virtual node listing, route table, and
//! NodeRegistry.
//!
//! All node definitions live in `global.db` (`nodes` table) since workspaces.toml
//! has been removed.  Peers are loaded from `peers.toml` and will migrate to the
//! DB in a future version.  The cluster token is taken from the
//! `AGENT_CLUSTER_TOKEN` environment variable.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// Unique identifier — the DB row id.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub workdir: String,
    #[serde(default)]
    pub description: String,
    /// Isolation mode string: "normal" | "container" | "sandbox".
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
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last-updated timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}

// ── Peer (remote agent server) ────────────────────────────────────────────────

/// A peer agent server on another machine.  Loaded from `global.db` (peers table).
/// This is a lightweight runtime view — the full model lives in `db::Peer`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeerEntry {
    /// Human-readable alias for this server (used as `@alias` suffix in node names).
    pub name: String,
    /// WebSocket URL of the remote agent server, e.g. `ws://192.168.1.20:9527`.
    pub url: String,
    /// Optional per-peer token override (falls back to `AGENT_CLUSTER_TOKEN`).
    #[serde(default)]
    pub token: Option<String>,
}

impl From<&crate::db::Peer> for PeerEntry {
    fn from(p: &crate::db::Peer) -> Self {
        PeerEntry {
            name: p.name.clone(),
            url: p.url.clone(),
            token: p.token.clone(),
        }
    }
}

/// Load enabled peer entries from the global database.
pub fn load_peers_from_db(db: &crate::db::GlobalDb) -> Vec<PeerEntry> {
    match db.list_enabled_peers() {
        Ok(peers) => peers.iter().map(|p| p.into()).collect(),
        Err(e) => {
            tracing::warn!("Failed to load peers from DB: {}", e);
            vec![]
        }
    }
}

/// Read the cluster token from the `AGENT_CLUSTER_TOKEN` environment variable.
pub fn cluster_token_from_env() -> Option<String> {
    std::env::var("AGENT_CLUSTER_TOKEN").ok().filter(|s| !s.is_empty())
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

/// Probe hardware capabilities and build the `VirtualNodeInfo` list from the DB.
/// Returns `(caps, virtual_nodes)`.
pub fn probe_capabilities() -> (NodeCapabilities, Vec<VirtualNodeInfo>) {
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

    let virtual_nodes = load_vnodes();

    (caps, virtual_nodes)
}

/// Convert a DB `Node` row into a `VirtualNodeInfo`.
fn node_to_vinfo(n: &crate::db::models::Node) -> VirtualNodeInfo {
    VirtualNodeInfo {
        id: n.id.clone(),
        name: n.name.clone(),
        workdir: n.workdir.clone(),
        description: n.description.clone(),
        isolation: n.isolation.clone(),
        sandbox: n.sandbox,
        exec_mode: n.exec_mode.clone(),
        tags: n.tags.clone(),
        created_at: Some(n.created_at.clone()),
        updated_at: Some(n.updated_at.clone()),
    }
}

/// Load virtual node list from the global database.
/// Called after Node CRUD mutations so the next `ready` / `node_saved` event
/// contains an up-to-date snapshot.
pub fn load_vnodes() -> Vec<VirtualNodeInfo> {
    let db_nodes = match crate::db::GlobalDb::open_or_create() {
        Ok(db) => db.list_nodes().unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    let mut result: Vec<VirtualNodeInfo> = db_nodes.iter().map(node_to_vinfo).collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

// ── Seed: first-boot import from legacy workspaces.toml ───────────────────────

/// If the DB `nodes` table is empty and a legacy `workspaces.toml` exists,
/// import its `[[node]]` entries into the DB so the user doesn't lose their
/// configuration after the upgrade.
pub fn seed_nodes_from_legacy_toml() {
    let Ok(db) = crate::db::GlobalDb::open_or_create() else { return };
    let Ok(existing) = db.list_nodes() else { return };
    if !existing.is_empty() {
        return; // Already have nodes, nothing to seed.
    }

    // Try to load legacy workspaces.toml
    let path = std::path::PathBuf::from(".")
        .join(".agent/workspaces.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Minimal legacy struct for import
    #[derive(Deserialize)]
    struct LegacyNode {
        name: String,
        #[serde(default)]
        workdir: Option<String>,
        #[serde(default)]
        description: String,
        #[serde(default)]
        isolation: Option<String>,
        #[serde(default)]
        sandbox: bool,
        #[serde(default)]
        exec_mode: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    }
    #[derive(Deserialize)]
    struct LegacyWorkspaces {
        #[serde(default, rename = "node")]
        nodes: Vec<LegacyNode>,
    }

    let Ok(legacy) = toml::from_str::<LegacyWorkspaces>(&text) else { return };
    if legacy.nodes.is_empty() {
        return;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut imported = 0usize;
    for n in &legacy.nodes {
        let wd = match &n.workdir {
            Some(w) if !w.is_empty() => w.clone(),
            _ => continue,
        };
        let node = crate::db::models::Node {
            id: uuid::Uuid::new_v4().to_string(),
            name: n.name.clone(),
            workdir: wd,
            description: n.description.clone(),
            isolation: n.isolation.clone(),
            sandbox: n.sandbox || matches!(n.isolation.as_deref(), Some("sandbox")),
            exec_mode: n.exec_mode.clone(),
            tags: n.tags.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        if db.save_node(&node).is_ok() {
            imported += 1;
        }
    }

    if imported > 0 {
        tracing::info!(
            "Seeded {} node(s) from legacy .agent/workspaces.toml into global.db",
            imported
        );
        // Rename the legacy file so we don't re-import on next startup.
        let bak = path.with_extension("toml.bak");
        let _ = std::fs::rename(&path, &bak);
    }
}

// ── In-process route table (tag → remote node) ───────────────────────────────

/// A resolved route entry: a specific virtual node on a physical server.
#[derive(Debug, Clone)]
pub struct RouteEntry {
    /// Name of the server entry.
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
// Runtime state of all known nodes: local nodes (always online) and
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

/// Populate the registry with local node entries from the DB.
/// Called once at server startup; local nodes are always Online.
pub fn registry_init_local(port: u16) {
    let vnodes = load_vnodes();

    let entries: Vec<RegistryEntry> = vnodes.iter().map(|vn| {
        let enc: String = vn.workdir.bytes().flat_map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
                vec![b as char]
            } else {
                format!("%{:02X}", b).chars().collect()
            }
        }).collect();
        RegistryEntry {
            name:          vn.name.clone(),
            url:           format!("ws://localhost:{}/?workdir={}", port, enc),
            peer_name:     None,
            status:        NodeStatus::Online,
            last_seen_secs: unix_now(),
            tags:          vn.tags.clone(),
            isolation:     vn.isolation.clone(),
            sandbox:       vn.sandbox || matches!(vn.isolation.as_deref(), Some("sandbox")),
            description:   vn.description.clone(),
            workdir:       Some(vn.workdir.clone()),
            exec_mode:     vn.exec_mode.clone(),
        }
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
