use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Tool for searching file contents using grep/ripgrep
pub struct GrepSearchTool;

/// Tool for finding files by name/pattern
pub struct FileSearchTool;

#[async_trait::async_trait]
impl Tool for GrepSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep_search".to_string(),
            description: "Search for text patterns in files using ripgrep (rg) or grep. Returns matching lines with file paths and line numbers. Supports regex patterns.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The search pattern (supports regex). When using '|' for alternation, wrap it in parentheses: use '(foo|bar)' instead of 'foo|bar'."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional: directory or file to search in (defaults to current directory)"
                    },
                    "include": {
                        "type": "string",
                        "description": "Optional: file glob pattern to include (e.g., '*.rs', '*.py')"
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Optional: whether the search is case-sensitive (default: false)"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Optional: maximum number of results to return (default: 50)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: pattern"),
        };

        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let include = input.get("include").and_then(|v| v.as_str());
        let case_sensitive = input
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        self.grep_search_internal(pattern, search_path, include, case_sensitive, max_results, project_dir).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: pattern"),
        };

        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let include = input.get("include").and_then(|v| v.as_str());
        let case_sensitive = input
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        // Check if search path is allowed (for sandbox mode)
        if !path_manager.is_path_allowed(search_path) {
            return ToolResult::error(format!(
                "Access denied: '{}' is outside the allowed directory.",
                search_path
            ));
        }

        let resolved_path = path_manager.resolve(search_path);
        let working_dir = path_manager.working_dir();

        self.grep_search_internal(pattern, &resolved_path.to_string_lossy(), include, case_sensitive, max_results, working_dir).await
    }
}

impl GrepSearchTool {
    async fn grep_search_internal(
        &self,
        pattern: &str,
        search_path: &str,
        include: Option<&str>,
        case_sensitive: bool,
        max_results: usize,
        working_dir: &Path,
    ) -> ToolResult {
        // Try ripgrep first, fall back to grep
        let (cmd_name, args) = if which_exists("rg") {
            let mut args = vec![
                "--line-number".to_string(),
                "--no-heading".to_string(),
                "--color=never".to_string(),
                format!("--max-count={}", max_results),
            ];

            if !case_sensitive {
                args.push("--ignore-case".to_string());
            }

            if let Some(inc) = include {
                args.push("--glob".to_string());
                args.push(inc.to_string());
            }

            args.push(pattern.to_string());
            args.push(search_path.to_string());

            ("rg", args)
        } else {
            let mut args = vec![
                "-rn".to_string(),
                "-E".to_string(),  // Extended regex: enables |, +, ?, () etc.
                "--color=never".to_string(),
            ];

            if !case_sensitive {
                args.push("-i".to_string());
            }

            if let Some(inc) = include {
                args.push("--include".to_string());
                args.push(inc.to_string());
            }

            args.push(pattern.to_string());
            args.push(search_path.to_string());

            ("grep", args)
        };

        let output = Command::new(cmd_name)
            .args(&args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.is_empty() {
                    ToolResult::success(format!("No matches found for pattern: {}", pattern))
                } else {
                    let lines: Vec<&str> = stdout.lines().take(max_results).collect();
                    let result = format!(
                        "Found {} matches for '{}':\n\n{}",
                        lines.len(),
                        pattern,
                        lines.join("\n")
                    );
                    ToolResult::success(result)
                }
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }
}

#[async_trait::async_trait]
impl Tool for FileSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_search".to_string(),
            description: "Search for files by name pattern using glob matching or find command. Returns paths of matching files.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The file name pattern to search for (e.g., '*.rs', 'Cargo.toml', 'test_*.py')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional: directory to search in (defaults to current directory)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: pattern"),
        };

        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        self.file_search_internal(pattern, search_path, project_dir).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: pattern"),
        };

        let search_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        // Check if search path is allowed (for sandbox mode)
        if !path_manager.is_path_allowed(search_path) {
            return ToolResult::error(format!(
                "Access denied: '{}' is outside the allowed directory.",
                search_path
            ));
        }

        let resolved_path = path_manager.resolve(search_path);
        let working_dir = path_manager.working_dir();

        self.file_search_internal(pattern, &resolved_path.to_string_lossy(), working_dir).await
    }
}

impl FileSearchTool {
    async fn file_search_internal(
        &self,
        pattern: &str,
        search_path: &str,
        working_dir: &Path,
    ) -> ToolResult {
        // Use find command for reliable cross-platform file search
        let output = Command::new("find")
            .arg(search_path)
            .arg("-name")
            .arg(pattern)
            .arg("-not")
            .arg("-path")
            .arg("*/target/*")
            .arg("-not")
            .arg("-path")
            .arg("*/node_modules/*")
            .arg("-not")
            .arg("-path")
            .arg("*/.git/*")
            .arg("-type")
            .arg("f")
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.is_empty() {
                    ToolResult::success(format!("No files found matching pattern: {}", pattern))
                } else {
                    let mut files: Vec<&str> = stdout.lines().collect();
                    files.sort();
                    let result = format!(
                        "Found {} files matching '{}':\n\n{}",
                        files.len(),
                        pattern,
                        files.join("\n")
                    );
                    ToolResult::success(result)
                }
            }
            Err(e) => ToolResult::error(format!("File search failed: {}", e)),
        }
    }
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}