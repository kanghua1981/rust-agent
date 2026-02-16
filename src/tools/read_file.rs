use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct ReadFileTool;

#[async_trait::async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file. You can optionally specify start_line and end_line to read a specific range.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to read (relative to the working directory or absolute)"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional: the 1-based line number to start reading from"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional: the 1-based line number to stop reading at (inclusive)"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let path = resolve_path(path, project_dir);

        match fs::read_to_string(&path).await {
            Ok(content) => {
                let start_line = input
                    .get("start_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let end_line = input
                    .get("end_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();

                let start = start_line.unwrap_or(1).saturating_sub(1);
                let end = end_line.unwrap_or(total_lines).min(total_lines);

                if start >= total_lines {
                    return ToolResult::error(format!(
                        "start_line {} exceeds file length of {} lines",
                        start + 1,
                        total_lines
                    ));
                }

                let selected_lines: Vec<String> = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
                    .collect();

                let result = format!(
                    "File: {} ({} total lines)\nShowing lines {}-{}:\n\n{}",
                    path.display(),
                    total_lines,
                    start + 1,
                    end,
                    selected_lines.join("\n")
                );

                ToolResult::success(result)
            }
            Err(e) => ToolResult::error(format!("Failed to read file '{}': {}", path.display(), e)),
        }
    }
}

fn resolve_path(path: &str, project_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}
