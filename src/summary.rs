//! Project summary management.
//!
//! Maintains a standalone `.agent/summary.md` file that contains
//! a concise, LLM-generated overview of the project. This is
//! separate from the dynamic memory system (`memory.rs`), because
//! the summary is a **static snapshot** of the project structure
//! that users can also edit by hand.
//!
//! The file is plain Markdown — no structured sections, just
//! human-readable project description.

use std::path::{Path, PathBuf};
use tracing::debug;

/// Get the path to the summary file for a given work directory.
fn summary_path(workdir: &Path) -> PathBuf {
    workdir.join(".agent").join("summary.md")
}

/// Check if a project summary file exists.
pub fn exists(workdir: &Path) -> bool {
    let path = summary_path(workdir);
    path.exists() && path.is_file()
}

/// Load the project summary from `.agent/summary.md`.
/// Returns `None` if the file doesn't exist or is empty.
pub fn load(workdir: &Path) -> Option<String> {
    let path = summary_path(workdir);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                debug!("Loaded project summary from {}", path.display());
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Save a project summary to `.agent/summary.md`.
pub fn save(workdir: &Path, content: &str) -> std::io::Result<()> {
    let path = summary_path(workdir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content.trim())?;
    debug!("Saved project summary to {}", path.display());
    Ok(())
}

/// Format the summary for injection into the system prompt.
pub fn to_system_prompt_section(summary: &str) -> String {
    format!(
        "\n\n--- Project Summary (from .agent/summary.md) ---\n{}",
        summary.trim()
    )
}
