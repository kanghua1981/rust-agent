//! Sandbox: file-system isolation with rollback/commit.
//!
//! One backend: **Overlay** (Linux kernel overlayfs or `fuse-overlayfs`).
//! All writes land in an upper layer; the original project is untouched
//! even if the agent crashes.  If neither overlay option is available,
//! the sandbox is silently disabled.
//!
//! User commands:
//! - `/changes`  — list modified / created / deleted files
//! - `/rollback` — discard all changes, restore original state
//! - `/commit`   — apply overlay changes to the real project

use std::ffi::CString;
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
    /// Unified diff text for text files; None for binary / new / deleted.
    pub diff: Option<String>,
}

impl ChangeSummary {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path.display().to_string(),
            "kind": self.kind.to_string(),
            "original_size": self.original_size,
            "current_size": self.current_size,
            "diff": self.diff,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum ChangeKind {
    Modified,
    Created,
    Deleted,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Modified => write!(f, "modified"),
            ChangeKind::Created => write!(f, "created"),
            ChangeKind::Deleted => write!(f, "deleted"),
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
    /// Synchronous flag: true only when backend is Disabled.
    /// Allows callers to check sandbox state without async.
    pub is_disabled: bool,
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
    Overlay {
        upper_dir: PathBuf,
        work_dir: PathBuf,
        merged_dir: PathBuf,
        /// true = kernel overlayfs (mount syscall); false = fuse-overlayfs userspace
        kernel: bool,
    },
}

// ═══════════════════════════════════════════════════════════════════
//  Construction
// ═══════════════════════════════════════════════════════════════════

impl Sandbox {
    /// Create an **enabled** sandbox for `project_dir`.
    ///
    /// Tries fuse-overlayfs (Linux, userspace).  If unavailable or the
    /// mount fails, the sandbox is silently disabled.
    pub fn new(project_dir: &Path) -> Self {
        let canonical = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());

        if let Some((backend, merged)) = try_setup_overlay(&canonical) {
            tracing::info!("Sandbox: overlay backend active, merged={}", merged.display());
            return Self {
                working_dir: merged,
                project_dir: canonical,
                is_disabled: false,
                inner: Arc::new(Mutex::new(SandboxInner { backend })),
            };
        }

        tracing::info!("Sandbox: fuse-overlayfs unavailable, sandbox disabled");
        Self::disabled(project_dir)
    }

    /// Create a **disabled** (no-op) sandbox.
    pub fn disabled(project_dir: &Path) -> Self {
        let canonical = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        Self {
            working_dir: canonical.clone(),
            project_dir: canonical,
            is_disabled: true,
            inner: Arc::new(Mutex::new(SandboxInner {
                backend: Backend::Disabled,
            })),
        }
    }

    /// Create an **overlay** sandbox from pre-mounted directories.
    /// Used by the worker process which sets up the mount before starting tokio.
    pub fn from_overlay_dirs(
        project_dir: &Path,
        upper_dir: &Path,
        work_dir: &Path,
        merged_dir: &Path,
    ) -> Self {
        let canonical = project_dir
            .canonicalize()
            .unwrap_or_else(|_| project_dir.to_path_buf());
        Self {
            working_dir: merged_dir.to_path_buf(),
            project_dir: canonical,
            is_disabled: false,
            inner: Arc::new(Mutex::new(SandboxInner {
                backend: Backend::Overlay {
                    upper_dir: upper_dir.to_path_buf(),
                    work_dir: work_dir.to_path_buf(),
                    merged_dir: merged_dir.to_path_buf(),
                    kernel: true,
                },
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

    /// Synchronous backend label — does NOT lock the async mutex.
    /// Returns: "disabled" | "overlay"
    pub fn backend_label_sync(&self) -> &'static str {
        if self.is_disabled {
            "disabled"
        } else {
            "overlay"
        }
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
            Backend::Overlay { .. } => "overlay",
        }
    }

    /// Number of tracked operations (files in overlay upper layer).
    pub async fn ops_count(&self) -> usize {
        match &self.inner.lock().await.backend {
            Backend::Disabled => 0,
            Backend::Overlay { upper_dir, .. } => count_files_recursive(upper_dir),
        }
    }

    // ── Changed files ───────────────────────────────────────────

    pub async fn changed_files(&self) -> Vec<ChangeSummary> {
        let inner = self.inner.lock().await;
        match &inner.backend {
            Backend::Disabled => vec![],
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
            Backend::Overlay {
                upper_dir,
                work_dir,
                merged_dir,
                kernel,
            } => overlay_rollback(
                &self.project_dir,
                upper_dir,
                work_dir,
                merged_dir,
                *kernel,
            ),
        }
    }

    // ── Commit ──────────────────────────────────────────────────

    pub async fn commit(&self) -> CommitResult {
        let mut inner = self.inner.lock().await;
        match &mut inner.backend {
            Backend::Disabled => CommitResult::default(),
            Backend::Overlay {
                upper_dir,
                work_dir,
                merged_dir,
                kernel,
            } => overlay_commit(
                &self.project_dir,
                upper_dir,
                work_dir,
                merged_dir,
                *kernel,
            ),
        }
    }
    pub async fn commit_file(&self, file_path: &str) -> CommitResult {
        let mut inner = self.inner.lock().await;
        match &mut inner.backend {
            Backend::Disabled => CommitResult::default(),
            Backend::Overlay {
                upper_dir,
                work_dir,
                merged_dir,
                kernel,
            } => overlay_commit_file(
                &self.project_dir,
                upper_dir,
                work_dir,
                merged_dir,
                *kernel,
                file_path,
            ),
        }
    }
    // ── Cleanup (call on graceful shutdown) ─────────────────────

    /// Called before a tool writes to a file.
    ///
    /// With the Overlay backend this is a no-op: the kernel/fuse overlay
    /// performs copy-on-write automatically, so the original lower layer is
    /// never touched.  The method exists so call sites don't need to
    /// special-case the backend.
    pub async fn before_write(&self, _path: &Path) {
        // No-op for Overlay backend.
        // (A future Snapshot backend would copy the file here.)
    }

    /// Unmount overlay (if active) and delete temp dirs.
    pub async fn cleanup(&self) {
        let inner = self.inner.lock().await;
        if let Backend::Overlay { merged_dir, kernel, .. } = &inner.backend {
            overlay_unmount(merged_dir, *kernel);
            // Delete the sandbox base dir (parent of upper/work/merged).
            // Safety guard: only remove directories that live under /tmp/agent-sandbox-*
            // to prevent accidental removal of the real project or system directories
            // (e.g. if merged_dir was set to /workspace, its parent would be "/").
            if let Some(base) = merged_dir.parent() {
                let base_str = base.to_string_lossy();
                let is_safe = base_str.starts_with("/tmp/agent-sandbox-")
                    || base_str.starts_with("/tmp/agent-worker-");
                if is_safe {
                    std::fs::remove_dir_all(base).ok();
                } else {
                    tracing::warn!(
                        "Sandbox cleanup: skipping remove_dir_all for non-tmp path: {}",
                        base.display()
                    );
                }
            }
        }
    }
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
        overlay_unmount(&merged, false);
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
                merged_dir: merged.clone(),                kernel: false,            };
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
                    diff: None,
                });
            } else {
                let current_size = entry.metadata().ok().map(|m| m.len() as usize);
                let (kind, original_size, diff) = if original_path.exists() {
                    let orig_size = std::fs::metadata(&original_path)
                        .ok()
                        .map(|m| m.len() as usize);
                    let diff = std::fs::read(&full_path).ok().and_then(|new_bytes| {
                        std::fs::read(&original_path).ok().and_then(|old_bytes| {
                            make_text_diff(&old_bytes, &new_bytes, &original_path)
                        })
                    });
                    (ChangeKind::Modified, orig_size, diff)
                } else {
                    // For new files, show full content as "diff"
                    let diff = std::fs::read_to_string(&full_path)
                        .ok()
                        .filter(|s| is_likely_text(s))
                        .map(|content| format!("--- /dev/null\n+++ {}\n@@ -0,0 +1,{} @@\n{}\n", 
                            rel.display(), 
                            content.lines().count(),
                            content.lines().map(|line| format!("+{}", line)).collect::<Vec<_>>().join("\n")));
                    (ChangeKind::Created, None, diff)
                };
                out.push(ChangeSummary {
                    path: original_path,
                    kind,
                    original_size,
                    current_size,
                    diff,
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
    kernel: bool,
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

    if !overlay_unmount(merged_dir, kernel) {
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
    if !overlay_remount(project_dir, upper_dir, work_dir, merged_dir, kernel) {
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
    kernel: bool,
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
    overlay_unmount(merged_dir, kernel);

    // In container mode project_dir is mounted bind_ro for safety.
    // Temporarily remount rw just for the copy, then lock it back to ro.
    // In CLI (non-container) mode MS_REMOUNT fails with EINVAL (no bind
    // mount) — that is harmless because the directory is already writable.
    remount_rw(project_dir);
    if let Err(e) = copy_upper_to_original(upper_dir, upper_dir, project_dir) {
        tracing::error!("Overlay commit copy failed: {}", e);
    }
    remount_ro(project_dir);

    // Wipe and remount for continued use
    clear_dir(upper_dir).ok();
    clear_dir(work_dir).ok();

    overlay_remount(project_dir, upper_dir, work_dir, merged_dir, kernel);

    CommitResult { modified, created }
}

fn overlay_commit_file(
    project_dir: &Path,
    upper_dir: &Path,
    work_dir: &Path,
    merged_dir: &Path,
    kernel: bool,
    file_path: &str,
) -> CommitResult {
    let target_path = PathBuf::from(file_path);

    // file_path may be an absolute container path like /workspace-ro/src/foo.rs.
    // strip project_dir prefix to get the relative path (e.g. src/foo.rs) that
    // mirrors the upper layer layout.  If already relative, use as-is.
    let rel_path = if target_path.is_absolute() {
        match target_path.strip_prefix(project_dir) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => return CommitResult::default(), // not under project_dir
        }
    } else {
        target_path.clone()
    };

    let upper_file = upper_dir.join(&rel_path);
    
    // Check if file exists in upper layer
    if !upper_file.exists() {
        return CommitResult::default();
    }
    
    let project_file = project_dir.join(&rel_path);
    
    // Determine if this is creation or modification
    let is_creation = !project_file.exists();
    
    // Create parent directories
    if let Some(parent) = project_file.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    
    // Temporarily make project_dir writable (bind_ro in container mode).
    remount_rw(project_dir);
    // Copy the file
    let copy_ok = std::fs::copy(&upper_file, &project_file).is_ok();
    remount_ro(project_dir);

    if copy_ok {
        // Remove from upper layer
        std::fs::remove_file(&upper_file).ok();
        // Remove empty directories
        if let Some(parent) = upper_file.parent() {
            remove_empty_dirs(parent, upper_dir).ok();
        }
        
        if is_creation {
            CommitResult { modified: 0, created: 1 }
        } else {
            CommitResult { modified: 1, created: 0 }
        }
    } else {
        CommitResult::default()
    }
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
            let rel = src_path.strip_prefix(upper_root).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("copy_upper_to_original: path {:?} is not under upper_root {:?}", src_path, upper_root),
                )
            })?;
            let dst = project_dir.join(rel);
            std::fs::create_dir_all(&dst)?;
            copy_upper_to_original(&src_path, upper_root, project_dir)?;
        } else if ft.is_file() {
            if name_str.starts_with(".wh.") {
                // Whiteout: delete corresponding file/dir from project
                let real_name = name_str.strip_prefix(".wh.").unwrap_or(&name_str);
                let rel_parent = dir.strip_prefix(upper_root).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("copy_upper_to_original: dir {:?} is not under upper_root {:?}", dir, upper_root),
                    )
                })?;
                let target = project_dir.join(rel_parent).join(real_name);
                // Safety: only delete within project_dir
                if target.starts_with(project_dir) {
                    if target.is_dir() {
                        std::fs::remove_dir_all(&target).ok();
                    } else {
                        std::fs::remove_file(&target).ok();
                    }
                } else {
                    tracing::warn!("Sandbox commit: whiteout target {:?} is outside project_dir, skipping", target);
                }
            } else {
                let rel = src_path.strip_prefix(upper_root).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("copy_upper_to_original: path {:?} is not under upper_root {:?}", src_path, upper_root),
                    )
                })?;
                let dst = project_dir.join(rel);
                // Safety: only write within project_dir
                if !dst.starts_with(project_dir) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Sandbox commit: destination {:?} is outside project_dir {:?}", dst, project_dir),
                    ));
                }
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src_path, &dst)?;
            }
        }
    }
    Ok(())
}

/// Temporarily remount a bind-mount read-write.
///
/// Used during commit to allow writing back to `/workspace-ro` (which is
/// kept `bind_ro` the rest of the time for safety).  In CLI / non-container
/// mode the path is not a bind mount, so the syscall returns `EINVAL` which
/// is silently ignored — the directory is already writable in that case.
fn remount_rw(path: &Path) {
    #[cfg(target_os = "linux")]
    if let Ok(cpath) = CString::new(path.to_string_lossy().as_ref()) {
        unsafe {
            libc::mount(
                std::ptr::null(),
                cpath.as_ptr(),
                std::ptr::null(),
                (libc::MS_BIND | libc::MS_REMOUNT) as libc::c_ulong,
                std::ptr::null(),
            )
        };
        // Intentionally ignore return value: EINVAL == not a bind mount → already rw.
    }
}

/// Re-lock a bind-mount read-only after commit is done.
fn remount_ro(path: &Path) {
    #[cfg(target_os = "linux")]
    if let Ok(cpath) = CString::new(path.to_string_lossy().as_ref()) {
        unsafe {
            libc::mount(
                std::ptr::null(),
                cpath.as_ptr(),
                std::ptr::null(),
                (libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY) as libc::c_ulong,
                std::ptr::null(),
            )
        };
    }
}

/// Unmount a fuse-overlayfs mount.  Returns true on success.
/// Public wrapper used by the worker process during initialisation cleanup.
pub fn unmount_fuse(merged_dir: &Path) {
    overlay_unmount(merged_dir, false);
}

fn overlay_unmount(merged_dir: &Path, kernel: bool) -> bool {
    if kernel {
        // Kernel overlayfs: use umount2 syscall with MNT_DETACH
        if let Ok(cpath) = CString::new(merged_dir.to_string_lossy().as_ref()) {
            let r = unsafe { libc::umount2(cpath.as_ptr(), libc::MNT_DETACH) };
            if r == 0 {
                return true;
            }
            tracing::warn!("umount2 failed for {}: errno={}", merged_dir.display(), std::io::Error::last_os_error());
        }
        return false;
    }
    // fuse-overlayfs: use fusermount
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

/// Remount an overlayfs after wipe.  Returns true on success.
fn overlay_remount(
    project_dir: &Path,
    upper_dir: &Path,
    work_dir: &Path,
    merged_dir: &Path,
    kernel: bool,
) -> bool {
    if kernel {
        // Kernel overlayfs via mount(2) syscall — no external binary needed.
        let opts = format!(
            "lowerdir={},upperdir={},workdir={},userxattr",
            project_dir.display(),
            upper_dir.display(),
            work_dir.display()
        );
        let src = CString::new("overlay").unwrap();
        let tgt = match CString::new(merged_dir.to_string_lossy().as_ref()) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let fst = CString::new("overlay").unwrap();
        let dat = match CString::new(opts.as_str()) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let r = unsafe {
            libc::mount(
                src.as_ptr(),
                tgt.as_ptr(),
                fst.as_ptr(),
                0,
                dat.as_ptr() as *const libc::c_void,
            )
        };
        if r != 0 {
            tracing::warn!(
                "kernel overlay remount failed for {}: {}",
                merged_dir.display(),
                std::io::Error::last_os_error()
            );
        }
        return r == 0;
    }
    // fuse-overlayfs subprocess
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
    matches!(status, Ok(s) if s.success())
}

// ═══════════════════════════════════════════════════════════════════
//  Shared helpers
// ═══════════════════════════════════════════════════════════════════

fn is_likely_text(content: &str) -> bool {
    // Quick heuristic: if we can read it as valid UTF-8 and it doesn't have
    // too many null bytes, consider it text
    content.len() < 100_000 && content.chars().filter(|&c| c == '\0').count() < 10
}

fn remove_empty_dirs(mut dir: &Path, stop_at: &Path) -> std::io::Result<()> {
    loop {
        if dir == stop_at || dir.parent().is_none() {
            break;
        }
        if std::fs::read_dir(dir)?.next().is_none() {
            std::fs::remove_dir(dir)?;
            if let Some(parent) = dir.parent() {
                dir = parent;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    Ok(())
}

/// Generate a unified diff between old and new file bytes.
/// Returns None for binary files or if there are no differences.
fn make_text_diff(old_bytes: &[u8], new_bytes: &[u8], path: &Path) -> Option<String> {
    if old_bytes.contains(&0) || new_bytes.contains(&0) {
        return None; // binary
    }
    let old_str = std::str::from_utf8(old_bytes).ok()?;
    let new_str = std::str::from_utf8(new_bytes).ok()?;
    if old_str == new_str {
        return None;
    }
    let path_str = path.display().to_string();
    let diff = similar::TextDiff::from_lines(old_str, new_str);
    let result = diff
        .unified_diff()
        .header(&format!("a/{}", path_str), &format!("b/{}", path_str))
        .to_string();
    if result.is_empty() { None } else { Some(result) }
}

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

