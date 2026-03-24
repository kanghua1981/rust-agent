//! Script-based tool loader.
//!
//! Scans `.agent/skills/*/tool.json` (and `skills/*/tool.json` for OpenClaw
//! AgentSkills compatibility) and registers each as a callable tool in the
//! agent's tool executor.
//!
//! ## tool.json format
//!
//! ```json
//! {
//!   "name": "my-tool",
//!   "description": "What this tool does",
//!   "parameters": {
//!     "type": "object",
//!     "properties": {
//!       "input": { "type": "string", "description": "The input text" }
//!     },
//!     "required": ["input"]
//!   },
//!   "command": "./run.sh",
//!   "timeout_secs": 30
//! }
//! ```
//!
//! ## Execution contract
//!
//! When the agent calls the tool, the executor:
//!   1. Serialises the LLM-supplied parameters as a single-line JSON string.
//!   2. Writes it to the script's **stdin**.
//!   3. Runs the command in the skill directory (so relative paths work).
//!   4. Returns stdout as the tool result, or stderr on non-zero exit.
//!
//! The command is executed via `bash -c`, so shell features (pipes, env vars,
//! `./script.sh`) all work as expected.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{Tool, ToolDefinition, ToolResult};

/// A tool backed by an external script, loaded from a `tool.json` file.
pub struct ScriptTool {
    definition: ToolDefinition,
    /// The command to run (passed to `bash -c`).
    command: String,
    /// Directory where the tool.json lives; the command runs here.
    skill_dir: PathBuf,
    /// Maximum execution time in seconds.
    timeout_secs: u64,
}

impl ScriptTool {
    pub fn new(
        definition: ToolDefinition,
        command: String,
        skill_dir: PathBuf,
        timeout_secs: u64,
    ) -> Self {
        Self {
            definition,
            command,
            skill_dir,
            timeout_secs,
        }
    }
}

#[async_trait::async_trait]
impl Tool for ScriptTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        // Serialize the LLM-supplied parameters to JSON for the script.
        let params_json = match serde_json::to_string(input) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("Failed to serialize parameters: {e}")),
        };

        tracing::info!(
            "Executing script tool '{}': {} (in {})",
            self.definition.name,
            self.command,
            self.skill_dir.display()
        );

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(&self.command)
            .current_dir(&self.skill_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to spawn command '{}': {e}",
                    self.command
                ))
            }
        };

        // Write params JSON to stdin then close it.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(params_json.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            // stdin is dropped here, signalling EOF to the child.
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            child.wait_with_output(),
        )
        .await;

        match result {
            Err(_) => ToolResult::error(format!(
                "Script tool '{}' timed out after {}s",
                self.definition.name, self.timeout_secs
            )),
            Ok(Err(e)) => ToolResult::error(format!("Script tool execution error: {e}")),
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

                if output.status.success() {
                    let out = if stdout.is_empty() {
                        if stderr.is_empty() {
                            "(no output)".to_string()
                        } else {
                            stderr
                        }
                    } else {
                        stdout
                    };
                    ToolResult::success(out)
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    let msg = if !stderr.is_empty() {
                        format!("exit code {code}: {stderr}")
                    } else if !stdout.is_empty() {
                        format!("exit code {code}: {stdout}")
                    } else {
                        format!("exit code {code}")
                    };
                    ToolResult::error(msg)
                }
            }
        }
    }
}

// ── Scanning ──────────────────────────────────────────────────────────────────

/// Scan well-known skill directories under `workdir` for `tool.json` files
/// and return all successfully parsed `ScriptTool` instances.
///
/// Directories scanned (in order):
///   - `.agent/skills/*/tool.json`   (native format)
///   - `skills/*/tool.json`          (OpenClaw AgentSkills layout)
pub fn load_script_tools(workdir: &Path) -> Vec<ScriptTool> {
    let mut tools = Vec::new();

    let candidates = [
        workdir.join(".agent").join("skills"),
        workdir.join("skills"),
    ];

    for skills_root in &candidates {
        if !skills_root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(skills_root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();

        for skill_dir in dirs {
            let tool_json_path = skill_dir.join("tool.json");
            if !tool_json_path.exists() {
                continue;
            }
            match parse_tool_json(&tool_json_path, &skill_dir) {
                Ok(tool) => {
                    tracing::debug!(
                        "Registered script tool '{}' from {}",
                        tool.definition.name,
                        tool_json_path.display()
                    );
                    tools.push(tool);
                }
                Err(e) => {
                    tracing::warn!(
                        "Skipping tool.json at {}: {e}",
                        tool_json_path.display()
                    );
                }
            }
        }
    }

    tools
}

/// Parse a single `tool.json` file into a `ScriptTool`.
fn parse_tool_json(path: &Path, skill_dir: &Path) -> anyhow::Result<ScriptTool> {
    let raw = std::fs::read_to_string(path)?;
    let val: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

    let name = val["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing \"name\""))?
        .to_string();

    let description = val["description"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing \"description\""))?
        .to_string();

    let parameters = val
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

    let command = val["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing \"command\""))?
        .to_string();

    let timeout_secs = val
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(60);

    let definition = ToolDefinition {
        name,
        description,
        parameters,
    };

    Ok(ScriptTool::new(
        definition,
        command,
        skill_dir.to_path_buf(),
        timeout_secs,
    ))
}
