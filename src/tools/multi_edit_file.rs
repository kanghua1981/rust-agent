//! Multi-edit tool: apply multiple find-and-replace edits to a single file
//! in one tool call, avoiding redundant LLM round-trips.

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct MultiEditFileTool;

#[async_trait::async_trait]
impl Tool for MultiEditFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "multi_edit_file".to_string(),
            description: r#"Apply multiple find-and-replace edits to a single file in one operation. Each edit specifies an old_string to find and a new_string to replace it with. Edits are applied sequentially (top to bottom is recommended). Each old_string must match exactly once in the file content at the time it is applied."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to edit"
                    },
                    "edits": {
                        "type": "array",
                        "description": "Array of edits to apply sequentially",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_string": {
                                    "type": "string",
                                    "description": "The exact text to find (must match exactly once)"
                                },
                                "new_string": {
                                    "type": "string",
                                    "description": "The text to replace old_string with"
                                }
                            },
                            "required": ["old_string", "new_string"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let edits = match input.get("edits").and_then(|v| v.as_array()) {
            Some(e) => e,
            None => return ToolResult::error("Missing required parameter: edits (must be an array)"),
        };

        if edits.is_empty() {
            return ToolResult::error("edits array is empty");
        }

        let path = resolve_path_old(path, project_dir);

        self.multi_edit_internal(&path, input).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let edits = match input.get("edits").and_then(|v| v.as_array()) {
            Some(e) => e,
            None => return ToolResult::error("Missing required parameter: edits (must be an array)"),
        };

        if edits.is_empty() {
            return ToolResult::error("edits array is empty");
        }

        // Check write permission
        if let Err(err) = path_manager.check_write_permission(path) {
            return ToolResult::error(err);
        }

        let resolved_path = path_manager.resolve(path);
        self.multi_edit_internal(&resolved_path, input).await
    }
}

impl MultiEditFileTool {
    async fn multi_edit_internal(&self, path: &Path, input: &serde_json::Value) -> ToolResult {
        let edits = match input.get("edits").and_then(|v| v.as_array()) {
            Some(e) => e,
            None => return ToolResult::error("Missing required parameter: edits (must be an array)"),
        };

        if edits.is_empty() {
            return ToolResult::error("edits array is empty");
        }

        // Read the file
        let mut content = match fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    path.display(),
                    e
                ))
            }
        };

        let mut applied = 0;
        let total = edits.len();
        let mut details = Vec::new();

        for (i, edit) in edits.iter().enumerate() {
            let old_string = match edit.get("old_string").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    details.push(format!("Edit {}: skipped (missing old_string)", i + 1));
                    continue;
                }
            };

            let new_string = match edit.get("new_string").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    details.push(format!("Edit {}: skipped (missing new_string)", i + 1));
                    continue;
                }
            };

            let count = content.matches(old_string).count();

            if count == 0 {
                details.push(format!(
                    "Edit {}: FAILED — old_string not found (may have been modified by a previous edit)",
                    i + 1
                ));
                continue;
            }

            if count > 1 {
                details.push(format!(
                    "Edit {}: FAILED — old_string found {} times, must be unique",
                    i + 1, count
                ));
                continue;
            }

            // Find the line number for the report
            let prefix = &content[..content.find(old_string).unwrap()];
            let line_num = prefix.lines().count() + 1;
            let old_lines = old_string.lines().count();
            let new_lines = new_string.lines().count();

            content = content.replacen(old_string, new_string, 1);
            applied += 1;

            details.push(format!(
                "Edit {}: OK — replaced {} lines with {} lines at line {}",
                i + 1, old_lines, new_lines, line_num
            ));
        }

        if applied == 0 {
            return ToolResult::error(format!(
                "No edits were applied to '{}':\n{}",
                path.display(),
                details.join("\n")
            ));
        }

        // Write the result
        match fs::write(path, &content).await {
            Ok(()) => ToolResult::success(format!(
                "Applied {}/{} edits to '{}':\n{}",
                applied,
                total,
                path.display(),
                details.join("\n")
            )),
            Err(e) => ToolResult::error(format!(
                "Failed to write file '{}': {}",
                path.display(),
                e
            )),
        }
    }
}

// Keep old resolve_path for backward compatibility
fn resolve_path_old(path: &str, project_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}
