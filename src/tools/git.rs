use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct GitTool;

#[async_trait::async_trait]
impl Tool for GitTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "git".to_string(),
            description: "Execute Git operations. Use this for creating branches, switching branches, checking status, viewing commit history, and other Git commands.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "The Git operation to perform. Supported operations: status, branch, checkout, commit, log, diff, add, push, pull, fetch, remote, clone, init",
                        "enum": ["status", "branch", "checkout", "commit", "log", "diff", "add", "push", "pull", "fetch", "remote", "clone", "init"]
                    },
                    "args": {
                        "type": "string",
                        "description": "Optional arguments for the Git operation. For example: branch name for checkout, commit message for commit, etc."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional: working directory for the Git command (defaults to current directory)"
                    }
                },
                "required": ["operation"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let operation = match input.get("operation").and_then(|v| v.as_str()) {
            Some(op) => op,
            None => return ToolResult::error("Missing required parameter: operation"),
        };

        let args = input
            .get("args")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        tracing::info!("Executing Git operation: {}", operation);

        // Build the Git command
        let mut git_command = format!("git {}", operation);
        if let Some(ref args_str) = args {
            git_command.push(' ');
            git_command.push_str(args_str);
        }

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(&git_command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Use explicit working_dir if provided, otherwise default to project_dir
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        } else {
            cmd.current_dir(project_dir);
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            cmd.output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result_text = String::new();
                result_text.push_str(&format!("Command: {}\n\n", git_command));

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
            Ok(Err(e)) => ToolResult::error(format!("Failed to execute Git command: {}", e)),
            Err(_) => ToolResult::error("Git command timed out after 60 seconds".to_string()),
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