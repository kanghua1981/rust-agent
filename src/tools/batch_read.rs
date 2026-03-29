//! Batch read tool: read multiple files in a single tool call to reduce
//! round-trips when the LLM needs context from several files at once.

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct BatchReadFilesTool;

#[async_trait::async_trait]
impl Tool for BatchReadFilesTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "batch_read_files".to_string(),
            description: r#"Read multiple files in a single call. Returns the content of each file separated by clear headers. Use this when you need to understand the relationship between several files at once, instead of calling read_file multiple times."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "description": "Array of file paths to read (relative to working directory or absolute)",
                        "items": {
                            "type": "string"
                        }
                    },
                    "max_lines_per_file": {
                        "type": "integer",
                        "description": "Optional: maximum lines to read per file (default: 200). Use this to keep output manageable when reading many files."
                    }
                },
                "required": ["paths"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let paths = match input.get("paths").and_then(|v| v.as_array()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: paths (must be an array)"),
        };

        if paths.is_empty() {
            return ToolResult::error("paths array is empty");
        }

        let max_lines = input
            .get("max_lines_per_file")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;

        self.batch_read_internal(paths, max_lines, project_dir).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let paths = match input.get("paths").and_then(|v| v.as_array()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: paths (must be an array)"),
        };

        if paths.is_empty() {
            return ToolResult::error("paths array is empty");
        }

        let max_lines = input
            .get("max_lines_per_file")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;

        self.batch_read_internal_with_path_manager(paths, max_lines, path_manager).await
    }
}

impl BatchReadFilesTool {
    async fn batch_read_internal(
        &self, 
        paths: &[serde_json::Value], 
        max_lines: usize, 
        project_dir: &Path
    ) -> ToolResult {
        let mut sections = Vec::new();
        let mut succeeded = 0;
        let mut failed = 0;

        for path_val in paths {
            let path_str = match path_val.as_str() {
                Some(s) => s,
                None => {
                    sections.push(format!("═══ <invalid path> ═══\nError: path must be a string\n"));
                    failed += 1;
                    continue;
                }
            };

            let resolved = resolve_path_old(path_str, project_dir);

            match fs::read_to_string(&resolved).await {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let total_lines = lines.len();
                    let display_lines = lines.len().min(max_lines);
                    let truncated = total_lines > max_lines;

                    let numbered: Vec<String> = lines[..display_lines]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:>4} | {}", i + 1, line))
                        .collect();

                    let mut section = format!(
                        "═══ {} ({} lines) ═══\n{}",
                        path_str,
                        total_lines,
                        numbered.join("\n")
                    );

                    if truncated {
                        section.push_str(&format!(
                            "\n... ({} more lines truncated)",
                            total_lines - max_lines
                        ));
                    }

                    sections.push(section);
                    succeeded += 1;
                }
                Err(e) => {
                    sections.push(format!(
                        "═══ {} ═══\nError: {}",
                        path_str, e
                    ));
                    failed += 1;
                }
            }
        }

        let header = format!(
            "Read {} files ({} succeeded, {} failed):\n\n",
            succeeded + failed,
            succeeded,
            failed
        );

        ToolResult::success(format!("{}{}", header, sections.join("\n\n")))
    }

    async fn batch_read_internal_with_path_manager(
        &self, 
        paths: &[serde_json::Value], 
        max_lines: usize, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let mut sections = Vec::new();
        let mut succeeded = 0;
        let mut failed = 0;

        for path_val in paths {
            let path_str = match path_val.as_str() {
                Some(s) => s,
                None => {
                    sections.push(format!("═══ <invalid path> ═══\nError: path must be a string\n"));
                    failed += 1;
                    continue;
                }
            };

            // Check if path is allowed (for sandbox mode)
            if !path_manager.is_path_allowed(path_str) {
                sections.push(format!(
                    "═══ {} ═══\nError: Access denied - path is outside the allowed directory.",
                    path_str
                ));
                failed += 1;
                continue;
            }

            let resolved = path_manager.resolve(path_str);

            match fs::read_to_string(&resolved).await {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let total_lines = lines.len();
                    let display_lines = lines.len().min(max_lines);
                    let truncated = total_lines > max_lines;

                    let numbered: Vec<String> = lines[..display_lines]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:>4} | {}", i + 1, line))
                        .collect();

                    let mut section = format!(
                        "═══ {} ({} lines) ═══\n{}",
                        path_str,
                        total_lines,
                        numbered.join("\n")
                    );

                    if truncated {
                        section.push_str(&format!(
                            "\n... ({} more lines truncated)",
                            total_lines - max_lines
                        ));
                    }

                    sections.push(section);
                    succeeded += 1;
                }
                Err(e) => {
                    sections.push(format!(
                        "═══ {} ═══\nError: {}",
                        path_str, e
                    ));
                    failed += 1;
                }
            }
        }

        let header = format!(
            "Read {} files ({} succeeded, {} failed):\n\n",
            succeeded + failed,
            succeeded,
            failed
        );

        ToolResult::success(format!("{}{}", header, sections.join("\n\n")))
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