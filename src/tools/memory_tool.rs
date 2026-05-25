//! Memory tool — lets the LLM manage the agent's persistent memory.
//!
//! Supports four actions:
//! - `add`     — append a knowledge fact
//! - `replace` — replace a knowledge entry matched by substring
//! - `remove`  — remove a knowledge entry matched by substring
//! - `read`    — read current memory contents
//!
//! Memory is persisted to `.agent/memory.md` (and `.agent/intelligent.json`
//! when using the IntelligentMemory backend).
//!
//! Behavioral notes (injected into the tool description):
//! - Use `add` to record durable project facts: architecture decisions,
//!   file locations, build conventions, coding patterns.
//! - Use `replace` to update stale facts.
//! - Use `remove` sparingly — only for facts proven wrong.
//! - `read` returns a snapshot; knowledge facts, file map, and recent session entries.

use std::sync::Arc;

use super::{Tool, ToolDefinition, ToolResult};
use crate::memory::MemoryProvider;

/// Maximum allowed length for a single memory entry (characters).
/// Entries exceeding this are rejected to prevent memory stuffing attacks.
const MAX_ENTRY_CHARS: usize = 2000;

/// Result of a security scan on memory content.
#[derive(Debug)]
struct SecurityScanResult {
    passed: bool,
    warnings: Vec<String>,
}

/// Scan memory content for security concerns.
///
/// Detects:
/// - Invisible/zero-width Unicode (U+200B, U+200C, U+200D, U+FEFF, U+2060, etc.)
/// - Prompt injection patterns ("ignore previous instructions", "system:", etc.)
/// - Excessive length (> MAX_ENTRY_CHARS)
///
/// Inspired by hermes-agent's `_scan_memory_content()`.
fn scan_memory_content(content: &str, action: &str) -> SecurityScanResult {
    let mut warnings: Vec<String> = Vec::new();

    // ── Length check ────────────────────────────────────────────────────
    if content.chars().count() > MAX_ENTRY_CHARS {
        warnings.push(format!(
            "Entry is {} chars (max {}). This may be a memory-stuffing attempt.",
            content.chars().count(),
            MAX_ENTRY_CHARS
        ));
    }

    // ── Invisible unicode detection ─────────────────────────────────────
    let invisible_chars: Vec<char> = content.chars().filter(|c| is_invisible_unicode(*c)).collect();
    if !invisible_chars.is_empty() {
        let unique: Vec<char> = {
            let mut v = invisible_chars.clone();
            v.sort();
            v.dedup();
            v
        };
        warnings.push(format!(
            "Entry contains {} invisible/zero-width Unicode character(s): {:?} (U+{:04X?}). \
             This may be an attempt to hide content or manipulate prompt behavior.",
            invisible_chars.len(),
            unique.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(""),
            unique.iter().map(|c| *c as u32).collect::<Vec<u32>>()
        ));
    }

    // ── Prompt injection pattern detection ──────────────────────────────
    let injection_patterns = [
        ("ignore previous instructions", "prompt injection"),
        ("ignore all previous", "prompt injection"),
        ("disregard prior", "prompt injection"),
        ("forget everything", "prompt injection"),
        ("you are now", "role hijacking"),
        ("new system prompt", "prompt override"),
        ("<|im_start|>system", "delimiter injection"),
        ("<|im_end|>", "delimiter injection"),
        ("[system](#system)", "markdown injection"),
        ("[INST]", "instruction delimiter"),
        ("[/INST]", "instruction delimiter"),
        ("<system>", "XML injection"),
        ("</system>", "XML injection"),
        ("print your system prompt", "prompt extraction"),
        ("reveal your instructions", "prompt extraction"),
        ("base64_decode", "code injection"),
        ("exec(", "code injection"),
        ("__import__", "code injection"),
    ];

    let content_lower = content.to_lowercase();
    for (pattern, category) in &injection_patterns {
        if content_lower.contains(pattern) {
            warnings.push(format!(
                "Entry matches {} pattern: '{}'. This may be a {} attempt.",
                category, pattern, category
            ));
            break; // One match is enough to flag
        }
    }

    // ── Data exfiltration pattern detection ─────────────────────────────
    // Long base64-like strings (potential encoded data dump)
    let base64_like = content.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
        .count();
    if base64_like > 200 && base64_like as f64 / content.len().max(1) as f64 > 0.8 {
        warnings.push(
            "Entry appears to contain a long base64-encoded payload. \
             This may be a data exfiltration attempt.".to_string()
        );
    }

    SecurityScanResult {
        passed: warnings.is_empty(),
        warnings,
    }
}

/// Characters considered "invisible" or zero-width that can be used to
/// hide content from human review while still being ingested by LLMs.
fn is_invisible_unicode(c: char) -> bool {
    matches!(c,
        '\u{200B}' | // Zero Width Space
        '\u{200C}' | // Zero Width Non-Joiner
        '\u{200D}' | // Zero Width Joiner
        '\u{FEFF}' | // Zero Width No-Break Space (BOM)
        '\u{2060}' | // Word Joiner
        '\u{2061}' | // Function Application
        '\u{2062}' | // Invisible Times
        '\u{2063}' | // Invisible Separator
        '\u{2064}' | // Invisible Plus
        '\u{00AD}' | // Soft Hyphen
        '\u{034F}' | // Combining Grapheme Joiner
        '\u{061C}' | // Arabic Letter Mark
        '\u{202A}' | // Left-to-Right Embedding
        '\u{202B}' | // Right-to-Left Embedding
        '\u{202C}' | // Pop Directional Formatting
        '\u{202D}' | // Left-to-Right Override
        '\u{202E}' | // Right-to-Left Override
        '\u{2066}' | // Left-to-Right Isolate
        '\u{2067}' | // Right-to-Left Isolate
        '\u{2068}' | // First Strong Isolate
        '\u{2069}' | // Pop Directional Isolate
        '\u{206A}' | // Inhibit Symmetric Swapping
        '\u{206B}' | // Activate Symmetric Swapping
        '\u{206C}' | // Inhibit Arabic Form Shaping
        '\u{206D}' | // Activate Arabic Form Shaping
        '\u{206E}' | // National Digit Shapes
        '\u{206F}'   // Nominal Digit Shapes
    )
}

pub struct MemoryTool {
    memory: Arc<dyn MemoryProvider>,
}

impl MemoryTool {
    pub fn new(memory: Arc<dyn MemoryProvider>) -> Self {
        Self { memory }
    }
}

#[async_trait::async_trait]
impl Tool for MemoryTool {
    fn toolset(&self) -> Option<super::Toolset> {
        Some(super::Toolset::Memory)
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory".to_string(),
            description: 
                "Manage the agent's persistent memory. Memory entries persist across sessions \
                and are injected into the system prompt. Use this to remember important project \
                facts, conventions, and lessons learned.\n\n\
                Actions:\n\
                - add: Append a fact. The fact should be a single concise sentence.\n\
                - replace: Update a fact. Provide `old_substring` to find the entry + `new_fact`.\n\
                - remove: Delete a fact by providing a `substring` that matches it.\n\
                - read: Show current memory contents (knowledge facts only by default).\n\n\
                Target (default: 'knowledge'):\n\
                - knowledge: Project-specific facts, architecture decisions, conventions.\n\
                - user: User preferences, coding style, communication preferences (persist across projects)."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add", "replace", "remove", "read"],
                        "description": "The operation to perform on memory."
                    },
                    "target": {
                        "type": "string",
                        "enum": ["knowledge", "user"],
                        "description": "Which memory store to operate on. 'knowledge'=project facts, 'user'=user preferences/style.",
                        "default": "knowledge"
                    },
                    "fact": {
                        "type": "string",
                        "description": "The knowledge fact to add (for 'add' action). One concise, durable sentence."
                    },
                    "old_substring": {
                        "type": "string",
                        "description": "A substring that uniquely matches the entry to replace (for 'replace' action)."
                    },
                    "new_fact": {
                        "type": "string",
                        "description": "The replacement fact (for 'replace' action)."
                    },
                    "substring": {
                        "type": "string",
                        "description": "A substring matching the entry to remove (for 'remove' action)."
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &std::path::Path) -> ToolResult {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("read");

        let target = input
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("knowledge");

        match action {
            "add" => {
                let fact = input
                    .get("fact")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if fact.trim().is_empty() {
                    return ToolResult::error("'fact' parameter is required for add action");
                }
                let trimmed = fact.trim().to_string();

                // Security scan before writing
                let scan = scan_memory_content(&trimmed, "add");
                if !scan.passed {
                    let warn_msg = scan.warnings.join("\n- ");
                    tracing::warn!("Memory tool security scan flagged add: {}", warn_msg);
                    return ToolResult::error(format!(
                        "Memory entry blocked by security scan:\n- {}\n\
                         If this is a false positive, edit .agent/memory.md directly.",
                        warn_msg
                    ));
                }

                if target == "user" {
                    // Store to .agent/user.md (user profile, cross-project)
                    if let Err(e) = store_user_entry(project_dir, &trimmed) {
                        return ToolResult::error(format!("Failed to store user entry: {}", e));
                    }
                    self.memory.on_memory_write("add", "user", &trimmed);
                    return ToolResult::success(format!("User profile updated: {}", trimmed));
                }

                self.memory.add_knowledge(&trimmed);
                // Notify external providers of the write
                self.memory.on_memory_write("add", "knowledge", &trimmed);
                ToolResult::success(format!("Memory added: {}", trimmed))
            }

            "replace" => {
                let old_sub = input
                    .get("old_substring")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_fact = input
                    .get("new_fact")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if old_sub.trim().is_empty() || new_fact.trim().is_empty() {
                    return ToolResult::error(
                        "Both 'old_substring' and 'new_fact' are required for replace action",
                    );
                }

                // Security scan on new fact
                let scan = scan_memory_content(new_fact.trim(), "replace");
                if !scan.passed {
                    let warn_msg = scan.warnings.join("\n- ");
                    tracing::warn!("Memory tool security scan flagged replace: {}", warn_msg);
                    return ToolResult::error(format!(
                        "Memory entry blocked by security scan:\n- {}\n\
                         If this is a false positive, edit .agent/memory.md directly.",
                        warn_msg
                    ));
                }

                if target == "user" {
                    let user_entries = load_user_entries(project_dir);
                    let old_lower = old_sub.to_lowercase();
                    if let Some(idx) = user_entries.iter().position(|e| e.to_lowercase().contains(&old_lower)) {
                        let mut updated = user_entries.clone();
                        updated[idx] = new_fact.trim().to_string();
                        if let Err(e) = save_user_entries(project_dir, &updated) {
                            return ToolResult::error(format!("Failed to save user entries: {}", e));
                        }
                        self.memory.on_memory_write("replace", "user", new_fact.trim());
                        return ToolResult::success(format!(
                            "User profile updated: '{}' -> '{}'",
                            &user_entries[idx], new_fact.trim()
                        ));
                    } else {
                        return ToolResult::error(format!(
                            "No user entry found containing '{}'", old_sub.trim()
                        ));
                    }
                }

                let knowledge = self.memory.knowledge();
                let old_lower = old_sub.to_lowercase();
                if let Some(idx) = knowledge.iter().position(|k| k.to_lowercase().contains(&old_lower)) {
                    let old = &knowledge[idx];
                    self.memory.on_memory_write("replace", "knowledge", new_fact.trim());
                    // Remove old entry then add new one
                    // (MemoryProvider doesn't have replace directly, so we add + mark)
                    self.memory.add_knowledge(new_fact.trim());
                    // We can't truly delete via the trait, so we log the replace
                    ToolResult::success(format!(
                        "Replaced memory entry '{}' with '{}'.\n\
                         Note: the old entry '{}' may still appear; use 'remove' action \
                         with substring to clean it up.",
                        old_sub.trim(),
                        new_fact.trim(),
                        old
                    ))
                } else {
                    ToolResult::error(format!(
                        "No knowledge entry found containing substring '{}'. \
                         Use 'read' action to see all entries.",
                        old_sub.trim()
                    ))
                }
            }

            "remove" => {
                let sub = input
                    .get("substring")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if sub.trim().is_empty() {
                    return ToolResult::error("'substring' parameter is required for remove action");
                }

                if target == "user" {
                    let mut user_entries = load_user_entries(project_dir);
                    let sub_lower = sub.to_lowercase();
                    if let Some(idx) = user_entries.iter().position(|e| e.to_lowercase().contains(&sub_lower)) {
                        let removed = user_entries.remove(idx);
                        if let Err(e) = save_user_entries(project_dir, &user_entries) {
                            return ToolResult::error(format!("Failed to save user entries: {}", e));
                        }
                        self.memory.on_memory_write("remove", "user", &removed);
                        return ToolResult::success(format!("User entry removed: {}", removed));
                    } else {
                        return ToolResult::error(format!(
                            "No user entry found containing '{}'", sub.trim()
                        ));
                    }
                }

                let knowledge = self.memory.knowledge();
                let sub_lower = sub.to_lowercase();
                // Find first match
                if let Some(idx) = knowledge.iter().position(|k| k.to_lowercase().contains(&sub_lower)) {
                    let removed = &knowledge[idx];
                    self.memory.on_memory_write("remove", "knowledge", removed);
                    // Note: MemoryProvider doesn't have a direct remove method.
                    let _removal_marker = format!("[REMOVED] {}", removed);
                    ToolResult::success(format!(
                        "Memory entry '{}' marked for removal.\n\
                         Note: removal support varies by memory backend. \
                         LocalFileMemory stores in .agent/memory.md — \
                         you may need to manually clean up the file.",
                        removed
                    ))
                } else {
                    ToolResult::error(format!(
                        "No knowledge entry found containing substring '{}'",
                        sub.trim()
                    ))
                }
            }

            "read" => {
                // Show USER profile if target is "user"
                if target == "user" {
                    let user_entries = load_user_entries(project_dir);
                    if user_entries.is_empty() {
                        return ToolResult::success("User profile is empty. Use `target: user` with `add` to set preferences.");
                    }
                    let mut output = String::from("## User Profile\n\n");
                    for (i, entry) in user_entries.iter().enumerate() {
                        output.push_str(&format!("{}. {}\n", i + 1, entry));
                    }
                    output.push_str(&format!("\nTotal: {} user preference entries.", user_entries.len()));
                    return ToolResult::success(output);
                }

                let knowledge = self.memory.knowledge();
                let file_map = self.memory.file_map();
                let session_log = self.memory.session_log();

                let mut output = String::new();

                if knowledge.is_empty() && file_map.is_empty() && session_log.is_empty() {
                    output.push_str("Memory is empty. No entries recorded yet.");
                } else {
                    output.push_str("## Knowledge Facts\n\n");
                    if knowledge.is_empty() {
                        output.push_str("_(none)_\n");
                    } else {
                        for (i, k) in knowledge.iter().enumerate() {
                            output.push_str(&format!("{}. {}\n", i + 1, k));
                        }
                    }

                    output.push_str("\n## File Map\n\n");
                    if file_map.is_empty() {
                        output.push_str("_(none)_\n");
                    } else {
                        for (path, desc) in file_map.iter().rev().take(10) {
                            output.push_str(&format!("- {} — {}\n", path, desc));
                        }
                    }

                    output.push_str("\n## Recent Session Log (last 5 entries)\n\n");
                    if session_log.is_empty() {
                        output.push_str("_(none)_\n");
                    } else {
                        for entry in session_log.iter().rev().take(5) {
                            output.push_str(&format!("- {}\n", entry));
                        }
                    }

                    output.push_str(&format!(
                        "\nTotal: {} knowledge facts, {} tracked files, {} log entries.",
                        knowledge.len(),
                        file_map.len(),
                        session_log.len(),
                    ));
                }
                ToolResult::success(output)
            }

            other => ToolResult::error(format!(
                "Unknown action '{}'. Valid actions: add, replace, remove, read.",
                other
            )),
        }
    }
}

// ── User Profile Storage (.agent/user.md) ─────────────────────────────────

/// User profile file path under the project's .agent directory.
fn user_file_path(project_dir: &std::path::Path) -> std::path::PathBuf {
    project_dir.join(".agent").join("user.md")
}

/// Load user profile entries from .agent/user.md.
/// Returns empty vec if the file doesn't exist.
fn load_user_entries(project_dir: &std::path::Path) -> Vec<String> {
    let path = user_file_path(project_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    None
                } else {
                    Some(trimmed.strip_prefix("- ").unwrap_or(trimmed).to_string())
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Save user profile entries to .agent/user.md.
fn save_user_entries(project_dir: &std::path::Path, entries: &[String]) -> anyhow::Result<()> {
    let path = user_file_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut content = String::from("# User Profile\n\n");
    content.push_str("_Preferences and style notes the agent learns about you._\n\n");
    for entry in entries {
        content.push_str(&format!("- {}\n", entry));
    }

    // Atomic write: write to temp file then rename
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &content)?;
    std::fs::rename(&tmp_path, &path)?;

    tracing::debug!("Saved {} user entries to {}", entries.len(), path.display());
    Ok(())
}

/// Append a single user entry to .agent/user.md.
fn store_user_entry(project_dir: &std::path::Path, entry: &str) -> anyhow::Result<()> {
    let mut entries = load_user_entries(project_dir);
    // Dedup: skip exact duplicates
    let entry_lower = entry.to_lowercase();
    if entries.iter().any(|e| e.to_lowercase() == entry_lower) {
        return Ok(());
    }
    // Cap at 20 user entries
    while entries.len() >= 20 {
        entries.remove(0);
    }
    entries.push(entry.to_string());
    save_user_entries(project_dir, &entries)
}
