//! Persistent memory system.
//!
//! Maintains a compact `.agent/memory.md` file that grows with usage,
//! making the agent increasingly familiar with the project over time.
//!
//! The file has three fixed-size sections with automatic pruning:
//! - **Project Knowledge**: stable facts (max 10), deduplicated
//! - **File Map**: important files encountered (max 15), LRU ordered
//! - **Session Log**: rolling action log (max 20), oldest dropped

use std::path::{Path, PathBuf};
use tracing::debug;

// Section size limits (lines)
#[allow(dead_code)]
const MAX_KNOWLEDGE: usize = 10;
const MAX_FILE_MAP: usize = 15;
const MAX_SESSION_LOG: usize = 20;

/// In-memory representation of the agent's persistent memory.
#[derive(Debug, Clone)]
pub struct Memory {
    pub knowledge: Vec<String>,
    pub file_map: Vec<(String, String)>, // (path, description)
    pub session_log: Vec<String>,
    file_path: PathBuf,
}

impl Memory {
    /// Load memory from `.agent/memory.md` under the given directory.
    /// Returns an empty memory if the file doesn't exist.
    pub fn load(workdir: &Path) -> Self {
        let file_path = workdir.join(".agent").join("memory.md");
        let mut mem = Memory {
            knowledge: Vec::new(),
            file_map: Vec::new(),
            session_log: Vec::new(),
            file_path,
        };

        if let Ok(content) = std::fs::read_to_string(&mem.file_path) {
            mem.parse(&content);
        }

        mem
    }

    /// Parse the markdown content into structured sections.
    fn parse(&mut self, content: &str) {
        let mut current_section = "";

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("## Project Knowledge") {
                current_section = "knowledge";
                continue;
            } else if trimmed.starts_with("## File Map") {
                current_section = "filemap";
                continue;
            } else if trimmed.starts_with("## Session Log") {
                current_section = "sessionlog";
                continue;
            }

            // Skip empty lines and the top-level heading
            if trimmed.is_empty() || trimmed.starts_with("# ") {
                continue;
            }

            // Strip leading "- " for list items
            let entry = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            if entry.is_empty() {
                continue;
            }

            match current_section {
                "knowledge" => {
                    self.knowledge.push(entry.to_string());
                }
                "filemap" => {
                    // Format: "path: description" or just "path"
                    if let Some((path, desc)) = entry.split_once(": ") {
                        self.file_map
                            .push((path.trim().to_string(), desc.trim().to_string()));
                    } else {
                        self.file_map
                            .push((entry.to_string(), String::new()));
                    }
                }
                "sessionlog" => {
                    self.session_log.push(entry.to_string());
                }
                _ => {}
            }
        }
    }

    /// Add a project knowledge entry. Deduplicates by checking if any
    /// existing entry contains the same key information.
    #[allow(dead_code)]
    pub fn add_knowledge(&mut self, fact: &str) {
        let fact = fact.trim().to_string();
        if fact.is_empty() {
            return;
        }

        // Simple dedup: skip if an existing entry contains this fact
        // or this fact contains an existing entry (update it)
        let fact_lower = fact.to_lowercase();
        for existing in &self.knowledge {
            if existing.to_lowercase() == fact_lower {
                return; // exact duplicate
            }
        }

        // If we have an entry about the same topic, replace it
        // (heuristic: first 3 words match)
        let fact_prefix: Vec<&str> = fact_lower.split_whitespace().take(3).collect();
        if fact_prefix.len() >= 2 {
            let prefix_str = fact_prefix.join(" ");
            if let Some(idx) = self.knowledge.iter().position(|e| {
                let e_lower = e.to_lowercase();
                let e_prefix: Vec<&str> = e_lower.split_whitespace().take(3).collect();
                e_prefix.join(" ") == prefix_str
            }) {
                self.knowledge[idx] = fact; // update existing
                return;
            }
        }

        self.knowledge.push(fact);

        // Prune: keep most recent entries
        while self.knowledge.len() > MAX_KNOWLEDGE {
            self.knowledge.remove(0);
        }
    }

    /// Record a file that was accessed or modified.
    /// If the file is already tracked, update its description and
    /// move it to the end (most recently used).
    pub fn touch_file(&mut self, path: &str, description: &str) {
        let path = path.trim().to_string();
        if path.is_empty() {
            return;
        }

        // Remove existing entry for this path (we'll re-add at the end)
        self.file_map.retain(|(p, _)| p != &path);

        self.file_map
            .push((path, description.trim().to_string()));

        // Prune: drop least recently used (front of list)
        while self.file_map.len() > MAX_FILE_MAP {
            self.file_map.remove(0);
        }
    }

    /// Append an action to the session log.
    pub fn log_action(&mut self, action: &str) {
        let action = action.trim().to_string();
        if action.is_empty() {
            return;
        }

        // Add timestamp
        let ts = short_timestamp();
        let entry = format!("[{}] {}", ts, action);

        self.session_log.push(entry);

        // Prune: drop oldest
        while self.session_log.len() > MAX_SESSION_LOG {
            self.session_log.remove(0);
        }
    }

    /// Record a truncation summary (from context window management).
    pub fn log_truncation_summary(&mut self, summary: &str) {
        let ts = short_timestamp();
        let entry = format!("[{}] [summary] {}", ts, summary.trim());
        self.session_log.push(entry);

        while self.session_log.len() > MAX_SESSION_LOG {
            self.session_log.remove(0);
        }
    }

    /// Save memory back to disk.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = self.to_markdown();
        std::fs::write(&self.file_path, content)?;
        debug!("Saved memory to {}", self.file_path.display());
        Ok(())
    }

    /// Format memory as Markdown.
    fn to_markdown(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Agent Memory\n".to_string());

        // Project Knowledge
        lines.push("## Project Knowledge\n".to_string());
        if self.knowledge.is_empty() {
            lines.push("_(No knowledge recorded yet)_\n".to_string());
        } else {
            for fact in &self.knowledge {
                lines.push(format!("- {}", fact));
            }
            lines.push(String::new());
        }

        // File Map
        lines.push("## File Map\n".to_string());
        if self.file_map.is_empty() {
            lines.push("_(No files recorded yet)_\n".to_string());
        } else {
            for (path, desc) in &self.file_map {
                if desc.is_empty() {
                    lines.push(format!("- {}", path));
                } else {
                    lines.push(format!("- {}: {}", path, desc));
                }
            }
            lines.push(String::new());
        }

        // Session Log
        lines.push("## Session Log\n".to_string());
        if self.session_log.is_empty() {
            lines.push("_(No actions recorded yet)_\n".to_string());
        } else {
            for entry in &self.session_log {
                lines.push(format!("- {}", entry));
            }
            lines.push(String::new());
        }

        lines.join("\n")
    }

    /// Format memory as a compact string for injection into the system prompt.
    /// This is intentionally terse to minimize token usage.
    pub fn to_system_prompt_section(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push("\n\n--- Agent Memory (from previous sessions) ---".to_string());

        if !self.knowledge.is_empty() {
            parts.push("Known facts:".to_string());
            for fact in &self.knowledge {
                parts.push(format!("• {}", fact));
            }
        }

        if !self.file_map.is_empty() {
            parts.push("Key files:".to_string());
            for (path, desc) in &self.file_map {
                if desc.is_empty() {
                    parts.push(format!("• {}", path));
                } else {
                    parts.push(format!("• {} – {}", path, desc));
                }
            }
        }

        if !self.session_log.is_empty() {
            parts.push("Recent actions:".to_string());
            // Only include the last 10 for the prompt to save tokens
            let start = self.session_log.len().saturating_sub(10);
            for entry in &self.session_log[start..] {
                parts.push(format!("• {}", entry));
            }
        }

        parts.join("\n")
    }

    /// Check if the memory is completely empty.
    pub fn is_empty(&self) -> bool {
        self.knowledge.is_empty() && self.file_map.is_empty() && self.session_log.is_empty()
    }

    /// Total number of entries across all sections.
    pub fn entry_count(&self) -> usize {
        self.knowledge.len() + self.file_map.len() + self.session_log.len()
    }
}

/// Generate a short timestamp like "02-15 10:30"
fn short_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let remaining_days = days % 365;
    let months = remaining_days / 30 + 1;
    let day = remaining_days % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;

    format!("{:02}-{:02} {:02}:{:02}", months, day, hour, min)
}
