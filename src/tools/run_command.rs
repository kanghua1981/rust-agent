use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct RunCommandTool;

#[async_trait::async_trait]
impl Tool for RunCommandTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".to_string(),
            description: "Execute a shell command and return its output. Use this for running tests, building projects, installing packages, git operations, or any terminal command.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional: working directory for the command (defaults to current directory)"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional: timeout in seconds (default 60)"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: command"),
        };

        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        self.run_command_internal(command, working_dir, timeout_secs, project_dir).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: command"),
        };

        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        // If working_dir is provided, resolve it using path manager
        let resolved_working_dir = if let Some(dir) = &working_dir {
            // Check if path is allowed (for sandbox mode)
            if !path_manager.is_path_allowed(dir) {
                return ToolResult::error(format!(
                    "Access denied: '{}' is outside the allowed directory.",
                    dir
                ));
            }
            path_manager.resolve(dir).to_path_buf()
        } else {
            // Use path manager's working directory
            path_manager.working_dir().to_path_buf()
        };

        self.run_command_internal(command, working_dir, timeout_secs, &resolved_working_dir).await
    }
}

impl RunCommandTool {
    async fn run_command_internal(
        &self,
        command: &str,
        working_dir: Option<String>,
        timeout_secs: u64,
        base_working_dir: &Path,
    ) -> ToolResult {
        tracing::info!("Executing command: {}", command);

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Use explicit working_dir if provided, otherwise default to base_working_dir
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        } else {
            cmd.current_dir(base_working_dir);
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result_text = String::new();

                if !stdout.is_empty() {
                    result_text.push_str("STDOUT:\n");
                    // Truncate if too long
                    if stdout.len() > 50000 {
                        let head = safe_truncate_head(&stdout, 25000);
                        let tail = safe_truncate_tail(&stdout, 25000);
                        result_text.push_str(head);
                        result_text.push_str("\n\n... (output truncated) ...\n\n");
                        result_text.push_str(tail);
                    } else {
                        result_text.push_str(&stdout);
                    }
                }

                if !stderr.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push_str("\n\n");
                    }
                    result_text.push_str("STDERR:\n");
                    if stderr.len() > 20000 {
                        let head = safe_truncate_head(&stderr, 10000);
                        let tail = safe_truncate_tail(&stderr, 10000);
                        result_text.push_str(head);
                        result_text.push_str("\n\n... (stderr truncated) ...\n\n");
                        result_text.push_str(tail);
                    } else {
                        result_text.push_str(&stderr);
                    }
                }

                let exit_code = output.status.code().unwrap_or(-1);
                result_text.push_str(&format!("\n\nExit code: {}", exit_code));

                if output.status.success() {
                    ToolResult::success(result_text)
                } else {
                    ToolResult::error(result_text)
                }
            }
            Ok(Err(e)) => ToolResult::error(format!("Failed to execute command: {}", e)),
            Err(_) => ToolResult::error(format!(
                "Command timed out after {} seconds",
                timeout_secs
            )),
        }
    }
}

/// Get the first `max_bytes` of a string, aligned to a char boundary.
fn safe_truncate_head(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Get the last `max_bytes` of a string, aligned to a char boundary.
fn safe_truncate_tail(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}
