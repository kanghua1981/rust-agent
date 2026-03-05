//! Sandbox: file-system isolation with rollback/commit.
//!
//! Two backends:
//! - **Overlay** (Linux, requires `fuse-overlayfs`): mounts an overlayfs
//!   over the project directory.  All writes (including command side-effects
//!   like build artifacts) land in an upper layer.  The original project is
//!   completely untouched — even if the agent crashes.
//! - **Snapshot** (cross-platform fallback): backs up each file before the
//!   agent modifies it.  Covers agent tool writes but NOT command
//!   side-effects (e.g. `cargo build` output).
//!
//! User commands:
//! - `/changes`  — list modified / created / deleted files
//! - `/rollback` — discard all changes, restore original state
//! - `/commit`   — apply changes to the real project (overlay) or discard
//!   snapshots (snapshot mode, files already on disk)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;

// ═══════════════════════════════════════════════════════════════════
//  Public types
// ═══════════════════════════════════════════════════════════════════

/// Summary of a single tracked file.
#[derive(Debug)]
pub struct ChangeSummary {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub original_size: Option<usize>,
    pub current_size: Option<usize>,
}

#[derive(Debug, PartialEq)]
pub enum ChangeKind {
    Modified,
    Created,
    Deleted,
    /// File was snapshotted but current content matches the original.
    Unchanged,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Modified => write!(f, "modified"),
            ChangeKind::Created => write!(f, "created"),
            ChangeKind::Deleted => write!(f, "deleted"),
            ChangeKind::Unchanged => write!(f, "unchanged"),
        }
    }
}

#[derive(Debug, Default)]
pub struct RollbackResult {
    pub restored: usize,
    pub deleted: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Default)]
pub struct CommitResult {
    pub modified: usize,
    pub created: usize,
}

// ═══════════════════════════════════════════════════════════════════
//  Sandbox (public handle)
// ═══════════════════════════════════════════════════════════════════

/// Thread-safe sandbox handle.  Clone is cheap (inner `Arc`).
#[derive(Debug, Clone)]
pub struct Sandbox {
    /// Effective working directory for tools.
    /// - Disabled / Snapshot → same as `project_dir`.
    /// - Overlay → the fuse-overlayfs *merged* directory.
    working_dir: PathBuf,
    /// The real, original project directory (never changes).
    project_dir: PathBuf,
    inner: Arc<Mutex<SandboxInner>>,
}

// ── Internal state ──────────────────────────────────────────────

#[derive(Debug)]
struct SandboxInner {
    backend: Backend,
}

#[derive(Debug)]
enum Backend {
    Disabled,
    Snapshot {
        changes: HashMap<PathBuf, FileChange>,
        ops_count: usize,
    },
    Overlay {
        upper_dir: PathBuf,
        work_dir: PathBuf,
        merged_dir: PathBuf,
    },
}

/// A file-level change tracked by the snapshot backend.
#[derive(Debug, Clone)]
enum FileChange {
    Modified { path: PathBuf, snapshot: Vec<u8> },
    Created { path: PathBuf },
}

// ═══════════════════════════════════════════════════════════════════
//  Construction
// ═══════════════════════════════════════════════════════════════════

impl Sandbox {
    /// Create an **enabled** sandbox for `project_dir`.
    ///
    /// Tries OverlayFS first (Linux + `fuse-overlayfs` installed).
    /// Falls back to the snapshot backend otherwise.
    pub fn new(project_dir: &Path) -> Self {
        let canonical = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());

        // Try overlay
        if let Some((backend, merged)) = try_setup_overlay(&canonical) {
            tracing::info!("Sandbox: overlay backend active, merged={}", merged.display());
            return Self {
                working_dir: merged,
                project_dir: canonical,
                inner: Arc::new(Mutex::new(SandboxInner { backend })),
            };
        }

        // Fallback: snapshot
        tracing::info!("Sandbox: snapshot backend active (fuse-overlayfs not available)");
        Self {
            working_dir: canonical.clone(),
            project_dir: canonical,
            inner: Arc::new(Mutex::new(SandboxInner {
                backend: Backend::Snapshot {
                    changes: HashMap::new(),
                    ops_count: 0,
                },
            })),
        }
    }

    /// Create a **disabled** (no-op) sandbox.
    pub fn disabled(project_dir: &Path) -> Self {
        let canonical = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        Self {
            working_dir: canonical.clone(),
            project_dir: canonical,
            inner: Arc::new(Mutex::new(SandboxInner {
                backend: Backend::Disabled,
            })),
        }
    }

    // ── Accessors (sync — no mutex needed) ──────────────────────

    /// The directory tools should operate in.
    ///
    /// - Overlay: the merged mount-point (looks like the project but
    ///   writes go to the upper layer).
    /// - Snapshot / Disabled: the original project directory.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// The real, original project directory.
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    // ── Async queries ───────────────────────────────────────────

    pub async fn is_enabled(&self) -> bool {
        !matches!(self.inner.lock().await.backend, Backend::Disabled)
    }

    pub async fn is_overlay(&self) -> bool {
        matches!(self.inner.lock().await.backend, Backend::Overlay { .. })
    }

    /// Human-readable backend name.
    pub async fn backend_name(&self) -> &'static str {
        match self.inner.lock().await.backend {
            Backend::Disabled => "disabled",
            Backend::Snapshot { .. } => "snapshot",
            Backend::Overlay { .. } => "overlay",
        }
    }

    /// Number of tracked operations.
    pub async fn ops_count(&self) -> usize {
        match &self.inner.lock().await.backend {
            Backend::Disabled => 0,
            Backend::Snapshot { ops_count, .. } => *ops_count,
            Backend::Overlay { upper_dir, .. } => count_files_recursive(upper_dir),
        }
    }

    // ── Pre-write hook ──────────────────────────────────────────

    /// Snapshot a file before it is modified (snapshot backend only).
    /// For overlay, this is a no-op — the filesystem handles it.
    pub async fn before_write(&self, path: &Path) {
        let mut inner = self.inner.lock().await;
        let (changes, ops_count) = match &mut inner.backend {
            Backend::Snapshot {
                ref mut changes,
                ref mut ops_count,
            } => (changes, ops_count),
            _ => return, // overlay / disabled: no-op
        };

        let canonical = normalize_path(path).unwrap_or_else(|| path.to_path_buf());

        if changes.contains_key(&canonical) {
            *ops_count += 1;
            return;
        }

        if canonical.exists() {
            match std::fs::read(&canonical) {
                Ok(data) => {
                    changes.insert(
                        canonical.clone(),
                        FileChange::Modified {
                            path: canonical,
                            snapshot: data,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!("Sandbox: failed to snapshot {}: {}", canonical.display(), e);
                }
            }
        } else {
            changes.insert(
                canonical.clone(),
                FileChange::Created { path: canonical },
            );
        }

        *ops_count += 1;
    }

    // ── Changed files ───────────────────────────────────────────

    pub async fn changed_files(&self) -> Vec<ChangeSummary> {
        let inner = self.inner.lock().await;
        match &inner.backend {
            Backend::Disabled => vec![],
            Backend::Snapshot { changes, .. } => snapshot_changed_files(changes),
            Backend::Overlay { upper_dir, .. } => {
                overlay_changed_files(upper_dir, &self.project_dir)
            }
        }
    }

    // ── Rollback ────────────────────────────────────────────────

    pub async fn rollback(&self) -> RollbackResult {
        let mut inner = self.inner.lock().await;
        match &mut inner.backend {
            Backend::Disabled => RollbackResult::default(),
            Backend::Snapshot { changes, ops_count } => {
                let result = snapshot_rollback(changes);
                changes.clear();
                *ops_count = 0;
                result
            }
            Backend::Overlay {
                upper_dir,
                work_dir,
                merged_dir,
            } => overlay_rollback(
                &self.project_dir,
                upper_dir,
                work_dir,
                merged_dir,
            ),
        }
    }

    // ── Commit ──────────────────────────────────────────────────

    pub async fn commit(&self) -> CommitResult {
        let mut inner = self.inner.lock().await;
        match &mut inner.backend {
            Backend::Disabled => CommitResult::default(),
            Backend::Snapshot { changes, ops_count } => {
                let result = snapshot_commit(changes);
                changes.clear();
                *ops_count = 0;
                result
            }
            Backend::Overlay {
                upper_dir,
                work_dir,
                merged_dir,
            } => overlay_commit(
                &self.project_dir,
                upper_dir,
                work_dir,
                merged_dir,
            ),
        }
    }

    // ── Cleanup (call on graceful shutdown) ─────────────────────

    /// Unmount overlay (if active) and delete temp dirs.
    /// For snapshot backend this is a no-op.
    pub async fn cleanup(&self) {
        let inner = self.inner.lock().await;
        if let Backend::Overlay { merged_dir, .. } = &inner.backend {
            overlay_unmount(merged_dir);
            // Delete the sandbox base dir (parent of upper/work/merged)
            if let Some(base) = merged_dir.parent() {
                std::fs::remove_dir_all(base).ok();
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Snapshot backend implementation
// ═══════════════════════════════════════════════════════════════════

fn snapshot_changed_files(changes: &HashMap<PathBuf, FileChange>) -> Vec<ChangeSummary> {
    let mut result: Vec<ChangeSummary> = changes
        .values()
        .map(|change| match change {
            FileChange::Modified { path, snapshot } => {
                let current = std::fs::read(path).ok();
                let changed = current.as_deref() != Some(snapshot.as_slice());
                ChangeSummary {
                    path: path.clone(),
                    kind: if changed {
                        ChangeKind::Modified
                    } else {
                        ChangeKind::Unchanged
                    },
                    original_size: Some(snapshot.len()),
                    current_size: current.as_ref().map(|c| c.len()),
                }
            }
            FileChange::Created { path } => {
                let current = std::fs::metadata(path).ok();
                ChangeSummary {
                    path: path.clone(),
                    kind: ChangeKind::Created,
                    original_size: None,
                    current_size: current.map(|m| m.len() as usize),
                }
            }
        })
        .collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

fn snapshot_rollback(changes: &HashMap<PathBuf, FileChange>) -> RollbackResult {
    let mut restored = 0usize;
    let mut deleted = 0usize;
    let mut errors = Vec::new();

    for change in changes.values() {
        match change {
            FileChange::Modified { path, snapshot } => match std::fs::write(path, snapshot) {
                Ok(()) => restored += 1,
                Err(e) => errors.push(format!("{}: {}", path.display(), e)),
            },
            FileChange::Created { path } => {
                if path.exists() {
                    match std::fs::remove_file(path) {
                        Ok(()) => deleted += 1,
                        Err(e) => errors.push(format!("{}: {}", path.display(), e)),
                    }
                }
                if let Some(parent) = path.parent() {
                    remove_empty_ancestors(parent);
                }
            }
        }
    }

    RollbackResult {
        restored,
        deleted,
        errors,
    }
}

fn snapshot_commit(changes: &HashMap<PathBuf, FileChange>) -> CommitResult {
    let mut modified = 0usize;
    let mut created = 0usize;
    for change in changes.values() {
        match change {
            FileChange::Modified { .. } => modified += 1,
            FileChange::Created { .. } => created += 1,
        }
    }
    CommitResult { modified, created }
}

// ═══════════════════════════════════════════════════════════════════
//  Overlay backend implementation
// ═══════════════════════════════════════════════════════════════════

/// Try to set up a fuse-overlayfs mount.  Returns the backend enum and
/// the merged directory on success, or `None` if `fuse-overlayfs` is
/// not available or the mount fails.
fn try_setup_overlay(project_dir: &Path) -> Option<(Backend, PathBuf)> {
    // Only attempt on Linux
    if !cfg!(target_os = "linux") {
        return None;
    }

    // Check that fuse-overlayfs is installed
    if !is_command_available("fuse-overlayfs") {
        return None;
    }

    let sandbox_id = short_hash(project_dir);
    let base = PathBuf::from(format!("/tmp/agent-sandbox-{}", sandbox_id));
    let upper = base.join("upper");
    let work = base.join("work");
    let merged = base.join("merged");

    // Clean up any stale previous run
    if merged.exists() {
        overlay_unmount(&merged);
    }
    if base.exists() {
        std::fs::remove_dir_all(&base).ok();
    }

    std::fs::create_dir_all(&upper).ok()?;
    std::fs::create_dir_all(&work).ok()?;
    std::fs::create_dir_all(&merged).ok()?;

    let status = std::process::Command::new("fuse-overlayfs")
        .arg("-o")
        .arg(format!(
            "lowerdir={},upperdir={},workdir={}",
            project_dir.display(),
            upper.display(),
            work.display()
        ))
        .arg(&merged)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status();

    match status {
        Ok(s) if s.success() => {
            let backend = Backend::Overlay {
                upper_dir: upper,
                work_dir: work,
                merged_dir: merged.clone(),
            };
            Some((backend, merged))
        }
        Ok(s) => {
            tracing::warn!("fuse-overlayfs exited with {}", s);
            std::fs::remove_dir_all(&base).ok();
            None
        }
        Err(e) => {
            tracing::warn!("Failed to run fuse-overlayfs: {}", e);
            std::fs::remove_dir_all(&base).ok();
            None
        }
    }
}

/// List changed files by walking the overlay upper directory.
fn overlay_changed_files(upper_dir: &Path, project_dir: &Path) -> Vec<ChangeSummary> {
    let mut result = Vec::new();
    collect_overlay_changes(upper_dir, upper_dir, project_dir, &mut result);
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

fn collect_overlay_changes(
    dir: &Path,
    upper_root: &Path,
    project_dir: &Path,
    out: &mut Vec<ChangeSummary>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let full_path = entry.path();

        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if ft.is_dir() {
            collect_overlay_changes(&full_path, upper_root, project_dir, out);
        } else if ft.is_file() {
            let rel = match full_path.strip_prefix(upper_root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let original_path = project_dir.join(rel);

            if name_str.starts_with(".wh.") {
                // Whiteout → deletion marker
                let real_name = name_str.strip_prefix(".wh.").unwrap_or(&name_str);
                let deleted_path = original_path
                    .parent()
                    .unwrap_or(project_dir)
                    .join(real_name);
                out.push(ChangeSummary {
                    path: deleted_path,
                    kind: ChangeKind::Deleted,
                    original_size: None,
                    current_size: None,
                });
            } else {
                let current_size = entry.metadata().ok().map(|m| m.len() as usize);
                let (kind, original_size) = if original_path.exists() {
                    let orig_size = std::fs::metadata(&original_path)
                        .ok()
                        .map(|m| m.len() as usize);
                    (ChangeKind::Modified, orig_size)
                } else {
                    (ChangeKind::Created, None)
                };
                out.push(ChangeSummary {
                    path: original_path,
                    kind,
                    original_size,
                    current_size,
                });
            }
        }
    }
}

/// Rollback the overlay: unmount, wipe upper+work, remount.
fn overlay_rollback(
    project_dir: &Path,
    upper_dir: &Path,
    work_dir: &Path,
    merged_dir: &Path,
) -> RollbackResult {
    let changes = overlay_changed_files(upper_dir, project_dir);
    let restored = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Modified)
        .count();
    let deleted = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Created)
        .count();

    let mut errors = Vec::new();

    if !overlay_unmount(merged_dir) {
        errors.push("Failed to unmount overlay".to_string());
        return RollbackResult {
            restored: 0,
            deleted: 0,
            errors,
        };
    }

    if let Err(e) = clear_dir(upper_dir) {
        errors.push(format!("Failed to clear upper dir: {}", e));
    }
    if let Err(e) = clear_dir(work_dir) {
        errors.push(format!("Failed to clear work dir: {}", e));
    }

    // Remount fresh overlay
    let status = std::process::Command::new("fuse-overlayfs")
        .arg("-o")
        .arg(format!(
            "lowerdir={},upperdir={},workdir={}",
            project_dir.display(),
            upper_dir.display(),
            work_dir.display()
        ))
        .arg(merged_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if !matches!(status, Ok(s) if s.success()) {
        errors.push("Failed to remount overlay after rollback".to_string());
    }

    RollbackResult {
        restored,
        deleted,
        errors,
    }
}

/// Commit the overlay: unmount, copy upper → original, wipe, remount.
fn overlay_commit(
    project_dir: &Path,
    upper_dir: &Path,
    work_dir: &Path,
    merged_dir: &Path,
) -> CommitResult {
    let changes = overlay_changed_files(upper_dir, project_dir);
    let modified = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Modified)
        .count();
    let created = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Created)
        .count();

    // Unmount so we can safely copy
    overlay_unmount(merged_dir);

    // Copy upper → original
    if let Err(e) = copy_upper_to_original(upper_dir, upper_dir, project_dir) {
        tracing::error!("Overlay commit copy failed: {}", e);
    }

    // Wipe and remount for continued use
    clear_dir(upper_dir).ok();
    clear_dir(work_dir).ok();

    let _ = std::process::Command::new("fuse-overlayfs")
        .arg("-o")
        .arg(format!(
            "lowerdir={},upperdir={},workdir={}",
            project_dir.display(),
            upper_dir.display(),
            work_dir.display()
        ))
        .arg(merged_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    CommitResult { modified, created }
}

/// Recursively copy changed files from the overlay upper layer to the
/// original project directory.  Handles whiteout files (deletions).
fn copy_upper_to_original(
    dir: &Path,
    upper_root: &Path,
    project_dir: &Path,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let src_path = entry.path();
        let ft = entry.file_type()?;

        if ft.is_dir() {
            let rel = src_path
                .strip_prefix(upper_root)
                .unwrap_or(&src_path);
            let dst = project_dir.join(rel);
            std::fs::create_dir_all(&dst)?;
            copy_upper_to_original(&src_path, upper_root, project_dir)?;
        } else if ft.is_file() {
            if name_str.starts_with(".wh.") {
                // Whiteout: delete corresponding file/dir from project
                let real_name = name_str.strip_prefix(".wh.").unwrap_or(&name_str);
                let rel_parent = dir.strip_prefix(upper_root).unwrap_or(dir);
                let target = project_dir.join(rel_parent).join(real_name);
                if target.is_dir() {
                    std::fs::remove_dir_all(&target).ok();
                } else {
                    std::fs::remove_file(&target).ok();
                }
            } else {
                let rel = src_path
                    .strip_prefix(upper_root)
                    .unwrap_or(&src_path);
                let dst = project_dir.join(rel);
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src_path, &dst)?;
            }
        }
    }
    Ok(())
}

/// Unmount a fuse-overlayfs mount.  Returns true on success.
fn overlay_unmount(merged_dir: &Path) -> bool {
    for cmd in &["fusermount", "fusermount3"] {
        let result = std::process::Command::new(cmd)
            .arg("-u")
            .arg(merged_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if matches!(result, Ok(s) if s.success()) {
            return true;
        }
    }
    // Last resort
    let result = std::process::Command::new("umount")
        .arg(merged_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    matches!(result, Ok(s) if s.success())
}

// ═══════════════════════════════════════════════════════════════════
//  Shared helpers
// ═══════════════════════════════════════════════════════════════════

fn is_command_available(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn short_hash(path: &Path) -> String {
    let s = path.display().to_string();
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    format!("{:x}", hash)
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        path.canonicalize().ok()
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            if let (Some(canon_parent), Some(file_name)) =
                (parent.canonicalize().ok(), path.file_name())
            {
                Some(canon_parent.join(file_name))
            } else {
                Some(path.to_path_buf())
            }
        } else {
            Some(path.to_path_buf())
        }
    } else {
        Some(path.to_path_buf())
    }
}

fn clear_dir(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    count += 1;
                } else if ft.is_dir() {
                    count += count_files_recursive(&entry.path());
                }
            }
        }
    }
    count
}

fn remove_empty_ancestors(dir: &Path) {
    let mut current = dir.to_path_buf();
    loop {
        match std::fs::read_dir(&current) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    break;
                }
                if std::fs::remove_dir(&current).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => break,
        }
    }
}
