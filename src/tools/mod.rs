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
pub mod call_node;
pub mod list_nodes;
pub mod connect_service;
pub mod query_service;
pub mod subscribe_service;
pub mod list_services;
pub mod browser;
pub mod script_tool;
// pub mod git; // Removed - Git operations handled by run_command

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::output::AgentOutput;

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
    /// When set, write/edit/delete tools are restricted to this directory.
    /// Paths outside it are rejected before the tool runs.
    allowed_dir: Option<PathBuf>,
}

/// Trait that all tools must implement
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult;
}

impl ToolExecutor {
    pub fn new(project_dir: PathBuf, output: Arc<dyn AgentOutput>) -> Self {
        let mut executor = ToolExecutor {
            tools: HashMap::new(),
            project_dir,
            allowed_dir: None,
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
        executor.register(Box::new(browser::BrowserTool::new()));
        // executor.register(Box::new(git::GitTool)); // Removed - Git operations handled by run_command

        // Only register agent-to-agent tools for the main manager agent, not for
        // worker sub-agents (to prevent infinite recursion).
        let agent_role = std::env::var("AGENT_ROLE").unwrap_or_else(|_| "manager".to_string());
        if agent_role == "manager" {
            // call_node: unified agent-to-agent delegation.
            // list_nodes: query parent's /nodes endpoint to discover available targets.
            // call_sub_agent and spawn_sub_agent are kept as internal modules but
            // NOT exposed to the LLM to avoid confusion.
            executor.register(Box::new(call_node::CallNodeTool::new(output.clone())));
            executor.register(Box::new(list_nodes::ListNodesTool));
        }

        // Service tools are available to all roles.
        executor.register(Box::new(connect_service::ConnectServiceTool));
        executor.register(Box::new(query_service::QueryServiceTool));
        executor.register(Box::new(subscribe_service::SubscribeServiceTool));
        executor.register(Box::new(subscribe_service::UnsubscribeServiceTool));
        executor.register(Box::new(list_services::ListServicesTool));

        // Dynamically register script tools from tool.json files in skill dirs.
        executor.load_script_tools_from(&executor.project_dir.clone());

        executor
    }

    fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        let def = tool.definition();
        self.tools.insert(def.name.clone(), tool);
    }

    /// Scan `workdir` for `tool.json` files and register each as a ScriptTool.
    /// Existing tools with the same name are overwritten (script tools win).
    fn load_script_tools_from(&mut self, workdir: &std::path::Path) {
        for st in script_tool::load_script_tools(workdir) {
            let name = st.definition().name.clone();
            tracing::info!("Loaded script tool: {name}");
            self.tools.insert(name, Box::new(st));
        }
    }

    /// Reload script tools after the working directory changes.
    /// Built-in tools are unaffected; only script tools are refreshed.
    pub fn reload_script_tools(&mut self) {
        // Remove previously registered script tools (identified by checking
        // whether the tool name is NOT in the static built-in set).
        // Simplest approach: re-scan and overwrite — no stale entries remain
        // because script tools from the old dir are simply replaced or
        // shadowed on next call.
        let workdir = self.project_dir.clone();
        self.load_script_tools_from(&workdir);
    }

    /// Spawn all configured MCP servers (from `.agent/mcp.toml`) and register
    /// their tools.  Called once after construction from `Agent::load_mcp_tools`.
    pub async fn load_mcp_tools(&mut self) {
        let project_dir = self.project_dir.clone();
        let mcp_tools = crate::mcp_client::connect_all(&project_dir).await;
        for tool in mcp_tools {
            let name = tool.definition().name.clone();
            tracing::info!("MCP tool registered: {name}");
            self.tools.insert(name, tool);
        }
    }

    /// Update the working directory used by all tools.
    pub fn set_project_dir(&mut self, dir: PathBuf) {
        self.project_dir = dir;
    }

    /// Restrict write/edit tools to paths inside `dir`.
    /// Pass `None` to remove the restriction.
    pub fn set_allowed_dir(&mut self, dir: Option<PathBuf>) {
        self.allowed_dir = dir;
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
            "browser",
            // "git", // Removed - Git operations handled by run_command
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
        // ── Directory guard ───────────────────────────────────────────────────
        // If an allowed_dir is set, reject write/edit tools that try to touch
        // paths outside it.  Read-only tools are not restricted.
        const WRITE_TOOLS: &[&str] = &["write_file", "edit_file", "multi_edit_file"];
        if let Some(ref allowed) = self.allowed_dir {
            if WRITE_TOOLS.contains(&name) {
                if let Some(path_str) = input.get("path").and_then(|v| v.as_str()) {
                    let resolved = if Path::new(path_str).is_absolute() {
                        PathBuf::from(path_str)
                    } else {
                        self.project_dir.join(path_str)
                    };
                    // canonicalize() fails for not-yet-existing files; use the
                    // raw resolved path as fallback so new files are still checked.
                    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
                    // Normalise allowed_dir the same way.
                    let allowed_canon = allowed.canonicalize().unwrap_or_else(|_| allowed.clone());
                    if !canonical.starts_with(&allowed_canon) {
                        return ToolResult::error(format!(
                            "Access denied: '{}' is outside the allowed directory '{}'.",
                            canonical.display(),
                            allowed_canon.display()
                        ));
                    }
                }
            }
        }

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
