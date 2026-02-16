use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct EditFileTool;

#[async_trait::async_trait]
impl Tool for EditFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: r#"Make targeted edits to a file by replacing specific text. Provide the exact old_string to find and the new_string to replace it with. The old_string must match exactly (including whitespace and indentation). Only the first occurrence will be replaced."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact text to find and replace. Must match exactly including whitespace."
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The text to replace old_string with"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::error("Missing required parameter: old_string"),
        };

        let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::error("Missing required parameter: new_string"),
        };

        let path = resolve_path(path);

        // Read the file
        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    path.display(),
                    e
                ))
            }
        };

        // Count occurrences
        let count = content.matches(old_string).count();

        if count == 0 {
            return ToolResult::error(format!(
                "old_string not found in '{}'. Make sure the text matches exactly including whitespace and indentation.",
                path.display()
            ));
        }

        if count > 1 {
            return ToolResult::error(format!(
                "old_string found {} times in '{}'. Please provide more context to make the match unique.",
                count,
                path.display()
            ));
        }

        // Perform the replacement
        let new_content = content.replacen(old_string, new_string, 1);

        match fs::write(&path, &new_content).await {
            Ok(()) => {
                // Find the line number where the change was made
                let prefix = &content[..content.find(old_string).unwrap()];
                let line_num = prefix.lines().count() + 1;
                let old_lines = old_string.lines().count();
                let new_lines = new_string.lines().count();

                ToolResult::success(format!(
                    "Successfully edited '{}': replaced {} lines with {} lines at line {}",
                    path.display(),
                    old_lines,
                    new_lines,
                    line_num
                ))
            }
            Err(e) => {
                ToolResult::error(format!("Failed to write file '{}': {}", path.display(), e))
            }
        }
    }
}

fn resolve_path(path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}
