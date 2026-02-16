use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct WriteFileTool;

#[async_trait::async_trait]
impl Tool for WriteFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Create a new file or completely overwrite an existing file with new content. Use edit_file for partial modifications.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path where the file should be written"
                    },
                    "content": {
                        "type": "string",
                        "description": "The complete content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let content = match input.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: content"),
        };

        let path = resolve_path(path);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    return ToolResult::error(format!(
                        "Failed to create directories for '{}': {}",
                        path.display(),
                        e
                    ));
                }
            }
        }

        match fs::write(&path, content).await {
            Ok(()) => {
                let line_count = content.lines().count();
                ToolResult::success(format!(
                    "Successfully wrote {} lines to '{}'",
                    line_count,
                    path.display()
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
