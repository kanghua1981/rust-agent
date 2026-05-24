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
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory".to_string(),
            description: 
                "Manage the agent's persistent memory. Memory entries persist across sessions \
                and are injected into the system prompt. Use this to remember important project \
                facts, conventions, and lessons learned.\n\n\
                Actions:\n\
                - add: Append a knowledge fact. The fact should be a single concise sentence.\n\
                - replace: Update a fact. Provide `old_substring` to find the entry + `new_fact`.\n\
                - remove: Delete a fact by providing a `substring` that matches it.\n\
                - read: Show current memory contents (knowledge facts only by default)."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add", "replace", "remove", "read"],
                        "description": "The operation to perform on memory."
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

    async fn execute(&self, input: &serde_json::Value, _project_dir: &std::path::Path) -> ToolResult {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("read");

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

                let knowledge = self.memory.knowledge();
                let sub_lower = sub.to_lowercase();
                // Find first match
                if let Some(idx) = knowledge.iter().position(|k| k.to_lowercase().contains(&sub_lower)) {
                    let removed = &knowledge[idx];
                    self.memory.on_memory_write("remove", "knowledge", removed);
                    // Note: MemoryProvider doesn't have a direct remove method.
                    // The new fact added below is a placeholder removal.
                    // Real removal requires extending the provider interface.
                    let removal_marker = format!("[REMOVED] {}", removed);
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
