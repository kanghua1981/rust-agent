pub mod read_file;
pub mod write_file;
pub mod edit_file;
pub mod multi_edit_file;
pub mod run_command;
pub mod search;
pub mod list_dir;
pub mod batch_read;
pub mod think;
pub mod read_pdf;
pub mod read_ebook;
pub mod load_skill;
pub mod create_skill;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Definition of a tool that the LLM can use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        ToolResult {
            output: output.into(),
            is_error: false,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        ToolResult {
            output: output.into(),
            is_error: true,
        }
    }
}

/// The tool executor that manages all available tools
pub struct ToolExecutor {
    tools: HashMap<String, Box<dyn Tool + Send + Sync>>,
    project_dir: PathBuf,
}

/// Trait that all tools must implement
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult;
}

impl ToolExecutor {
    pub fn new(project_dir: PathBuf) -> Self {
        let mut executor = ToolExecutor {
            tools: HashMap::new(),
            project_dir,
        };

        // Register all built-in tools
        executor.register(Box::new(read_file::ReadFileTool));
        executor.register(Box::new(write_file::WriteFileTool));
        executor.register(Box::new(edit_file::EditFileTool));
        executor.register(Box::new(multi_edit_file::MultiEditFileTool));
        executor.register(Box::new(run_command::RunCommandTool));
        executor.register(Box::new(search::GrepSearchTool));
        executor.register(Box::new(search::FileSearchTool));
        executor.register(Box::new(list_dir::ListDirTool));
        executor.register(Box::new(batch_read::BatchReadFilesTool));
        executor.register(Box::new(think::ThinkTool));
        executor.register(Box::new(read_pdf::ReadPdfTool));
        executor.register(Box::new(read_ebook::ReadEbookTool));
        executor.register(Box::new(load_skill::LoadSkillTool));
        executor.register(Box::new(create_skill::CreateSkillTool));

        executor
    }

    fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        let def = tool.definition();
        self.tools.insert(def.name.clone(), tool);
    }

    /// Get all tool definitions for the LLM
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Get only read-only tool definitions (no file writes or edits).
    /// Used during the planning phase so the LLM can explore but not modify anything.
    /// `run_command` is included so the planner can run read-only shell/git commands
    /// (e.g. `git status`, `git log`, `git diff`, `find`, `cat`) — the planner system
    /// prompt instructs it never to run commands that mutate state.
    pub fn readonly_definitions(&self) -> Vec<ToolDefinition> {
        const READONLY_TOOLS: &[&str] = &[
            "read_file",
            "batch_read_files",
            "read_pdf",
            "list_directory",
            "grep_search",
            "file_search",
            "think",
            "read_ebook",
            "load_skill",
            "run_command",
        ];
        self.tools
            .values()
            .map(|t| t.definition())
            .filter(|d| READONLY_TOOLS.contains(&d.name.as_str()))
            .collect()
    }

    /// Execute a tool by name.
    ///
    /// Wraps the tool execution in `catch_unwind` so that a panic inside
    /// a tool (e.g. slice-index-out-of-range) is turned into a
    /// `ToolResult::Error` instead of crashing the whole agent.  This
    /// prevents orphaned `tool_use` blocks without matching `tool_result`
    /// in the conversation, which would cause Anthropic API 400 errors.
    pub async fn execute(&self, name: &str, input: &serde_json::Value) -> ToolResult {
        match self.tools.get(name) {
            Some(tool) => {
                use futures::FutureExt; // catch_unwind on futures

                let result = std::panic::AssertUnwindSafe(
                    tool.execute(input, &self.project_dir),
                )
                .catch_unwind()
                .await;

                match result {
                    Ok(r) => r,
                    Err(panic_val) => {
                        let panic_msg = if let Some(s) = panic_val.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_val.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        let msg = format!("Tool '{}' panicked: {}", name, panic_msg);
                        tracing::error!("{}", msg);
                        ToolResult::error(msg)
                    }
                }
            }
            None => ToolResult::error(format!("Unknown tool: {}", name)),
        }
    }
}
