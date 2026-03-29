//! Unified path management for the Rust Agent.
//!
//! This module provides a centralized way to handle path resolution,
//! normalization, redirection (for sandbox mode), and permission checking.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::sandbox::Sandbox;

/// Manages path resolution, normalization, and redirection.
#[derive(Debug, Clone)]
pub struct PathManager {
    /// Original project directory (real filesystem)
    original_project_dir: PathBuf,
    /// Current working directory (sandbox merged dir or original dir)
    working_dir: PathBuf,
    /// Sandbox instance (optional)
    sandbox: Option<Arc<Sandbox>>,
    /// Allowed directory for write operations (optional)
    allowed_dir: Option<PathBuf>,
}

impl PathManager {
    /// Create a new PathManager.
    pub fn new(project_dir: PathBuf, sandbox: Option<Arc<Sandbox>>) -> Self {
        let working_dir = if let Some(ref sandbox) = sandbox {
            sandbox.working_dir().to_path_buf()
        } else {
            project_dir.clone()
        };

        let allowed_dir = if let Some(ref sandbox) = sandbox {
            Some(sandbox.working_dir().to_path_buf())
        } else {
            None
        };

        Self {
            original_project_dir: project_dir,
            working_dir,
            sandbox,
            allowed_dir,
        }
    }

    /// Create a PathManager without sandbox.
    pub fn without_sandbox(project_dir: PathBuf) -> Self {
        Self {
            original_project_dir: project_dir.clone(),
            working_dir: project_dir,
            sandbox: None,
            allowed_dir: None,
        }
    }

    /// Create a PathManager with sandbox.
    pub fn with_sandbox(project_dir: PathBuf, sandbox: Arc<Sandbox>) -> Self {
        let working_dir = sandbox.working_dir().to_path_buf();
        let allowed_dir = Some(working_dir.clone());

        Self {
            original_project_dir: project_dir,
            working_dir,
            sandbox: Some(sandbox),
            allowed_dir,
        }
    }

    /// Resolve a path (relative to working directory or absolute).
    /// This handles sandbox redirection if needed.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        
        if path.is_absolute() {
            // For absolute paths, check if it's inside the original project directory
            // If so, redirect to sandbox working directory if sandbox is enabled
            if let Some(ref _sandbox) = self.sandbox {
                if path.starts_with(&self.original_project_dir) {
                    // Redirect to sandbox working directory
                    let relative = path.strip_prefix(&self.original_project_dir)
                        .unwrap_or(path);
                    return self.working_dir.join(relative);
                }
            }
            path.to_path_buf()
        } else {
            // Relative paths are resolved relative to working directory
            self.working_dir.join(path)
        }
    }

    /// Resolve a path and normalize it (canonicalize if possible).
    /// This is used for permission checking.
    pub fn resolve_and_normalize(&self, path: &str) -> PathBuf {
        let resolved = self.resolve(path);
        
        // Try to canonicalize the path
        match resolved.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                // If canonicalization fails (e.g., file doesn't exist),
                // try to canonicalize the parent directory
                if let Some(parent) = resolved.parent() {
                    match parent.canonicalize() {
                        Ok(canonical_parent) => {
                            if let Some(filename) = resolved.file_name() {
                                canonical_parent.join(filename)
                            } else {
                                resolved
                            }
                        }
                        Err(_) => resolved,
                    }
                } else {
                    resolved
                }
            }
        }
    }

    /// Check if a path is allowed for write operations.
    /// Returns Ok(()) if allowed, Err(message) if not.
    pub fn check_write_permission(&self, path: &str) -> Result<(), String> {
        if let Some(ref allowed) = self.allowed_dir {
            let normalized = self.resolve_and_normalize(path);
            let allowed_normalized = allowed.canonicalize().unwrap_or_else(|_| allowed.clone());
            
            if !normalized.starts_with(&allowed_normalized) {
                return Err(format!(
                    "Access denied: '{}' is outside the allowed directory '{}'.",
                    normalized.display(),
                    allowed_normalized.display()
                ));
            }
        }
        Ok(())
    }

    /// Check if a path is inside the allowed directory (for read operations).
    pub fn is_path_allowed(&self, path: &str) -> bool {
        if let Some(ref allowed) = self.allowed_dir {
            let normalized = self.resolve_and_normalize(path);
            let allowed_normalized = allowed.canonicalize().unwrap_or_else(|_| allowed.clone());
            normalized.starts_with(&allowed_normalized)
        } else {
            true // No restrictions
        }
    }

    /// Get the working directory (sandbox merged dir or original dir).
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Get the original project directory.
    pub fn original_project_dir(&self) -> &Path {
        &self.original_project_dir
    }

    /// Check if sandbox is enabled.
    pub fn is_sandbox_enabled(&self) -> bool {
        self.sandbox.is_some()
    }

    /// Get the sandbox instance (if any).
    pub fn sandbox(&self) -> Option<&Arc<Sandbox>> {
        self.sandbox.as_ref()
    }

    /// Update the sandbox instance.
    pub fn set_sandbox(&mut self, sandbox: Option<Arc<Sandbox>>) {
        self.sandbox = sandbox;
        if let Some(ref sandbox) = self.sandbox {
            self.working_dir = sandbox.working_dir().to_path_buf();
            self.allowed_dir = Some(self.working_dir.clone());
        } else {
            self.working_dir = self.original_project_dir.clone();
            self.allowed_dir = None;
        }
    }

    /// Update the allowed directory.
    pub fn set_allowed_dir(&mut self, dir: Option<PathBuf>) {
        self.allowed_dir = dir;
    }
}

/// Resolve a path using the old logic (for backward compatibility).
/// This is used during the transition period.
pub fn resolve_path_old(path: &str, project_dir: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}