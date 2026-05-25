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
pub mod upload_image;
pub mod todo;
pub mod memory_tool;
pub mod cache;
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

/// Toolset categories for grouping tools into logical families.
///
/// Used by the role system (planner/executor/checker) and for
/// runtime enable/disable of tool groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Toolset {
    FileRead,
    FileWrite,
    Search,
    Shell,
    AgentComms,
    Memory,
    Browser,
    Skill,
    Think,
    UploadImage,
}

impl Toolset {
    /// Human-readable label for display and configuration.
    pub fn label(&self) -> &'static str {
        match self {
            Toolset::FileRead => "file_read",
            Toolset::FileWrite => "file_write",
            Toolset::Search => "search",
            Toolset::Shell => "shell",
            Toolset::AgentComms => "agent_comms",
            Toolset::Memory => "memory",
            Toolset::Browser => "browser",
            Toolset::Skill => "skill",
            Toolset::Think => "think",
            Toolset::UploadImage => "upload_image",
        }
    }

    /// Parse from a label string.
    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "file_read" => Some(Toolset::FileRead),
            "file_write" => Some(Toolset::FileWrite),
            "search" => Some(Toolset::Search),
            "shell" => Some(Toolset::Shell),
            "agent_comms" => Some(Toolset::AgentComms),
            "memory" => Some(Toolset::Memory),
            "browser" => Some(Toolset::Browser),
            "skill" => Some(Toolset::Skill),
            "think" => Some(Toolset::Think),
            "upload_image" => Some(Toolset::UploadImage),
            _ => None,
        }
    }

    /// Default toolsets for a planner role (explore, understand, plan).
    pub fn planner_toolsets() -> Vec<Toolset> {
        vec![
            Toolset::FileRead,
            Toolset::Search,
            Toolset::Think,
            Toolset::Skill,
            Toolset::Memory,
        ]
    }

    /// Default toolsets for an executor role (modify, build, run).
    pub fn executor_toolsets() -> Vec<Toolset> {
        vec![
            Toolset::FileRead,
            Toolset::FileWrite,
            Toolset::Search,
            Toolset::Shell,
            Toolset::Browser,
            Toolset::Think,
            Toolset::Skill,
            Toolset::Memory,
        ]
    }

    /// Default toolsets for a checker role (verify, test, review).
    pub fn checker_toolsets() -> Vec<Toolset> {
        vec![
            Toolset::FileRead,
            Toolset::Search,
            Toolset::Shell,
            Toolset::Think,
        ]
    }
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
    path_manager: Option<Arc<crate::path_manager::PathManager>>,
    /// When set, write/edit/delete tools are restricted to this directory.
    /// Paths outside it are rejected before the tool runs.
    allowed_dir: Option<PathBuf>,
    /// 插件管理器（可选）
    plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>,
    /// Hook 事件总线（与 Agent 共享同一 Arc）
    hook_bus: Option<Arc<crate::plugin::hook_bus::HookBus>>,
    /// In-memory cache for read-only tool results (Layer 1).
    result_cache: std::cell::RefCell<cache::ToolResultCache>,
}

/// Trait that all tools must implement
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult;

    /// Which toolset this tool belongs to.  Used for role-based filtering.
    /// Default returns `None`; implement for built-in tools to enable
    /// toolset-based grouping.
    fn toolset(&self) -> Option<Toolset> {
        None
    }
    
    /// Execute with path manager (optional, for tools that need advanced path handling)
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        // Default implementation falls back to execute with project_dir
        self.execute(input, path_manager.working_dir()).await
    }
}

impl ToolExecutor {
    pub fn new(project_dir: PathBuf, output: Arc<dyn AgentOutput>, plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>) -> Self {
        let mut executor = ToolExecutor {
            tools: HashMap::new(),
            project_dir,
            path_manager: None,
            allowed_dir: None,
            plugin_manager,
            hook_bus: None,
            result_cache: std::cell::RefCell::new(cache::ToolResultCache::new(200, 120)), // 200 entries, 2-min TTL
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
        let pm_for_load_skill = executor.plugin_manager.clone();
        executor.register(Box::new(load_skill::LoadSkillTool::new(pm_for_load_skill)));
        executor.register(Box::new(create_skill::CreateSkillTool));
        executor.register(Box::new(browser::BrowserTool::new()));
        executor.register(Box::new(upload_image::UploadImageTool));
        executor.register(Box::new(todo::TodoWriteTool));
        executor.register(Box::new(todo::TodoUpdateTool));
        executor.register(Box::new(todo::TodoReadTool));
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

        executor
    }

    /// 加载插件工具
    pub async fn load_plugin_tools(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Clone the plugin manager Arc first to avoid borrow conflicts
        let pm_clone = self.plugin_manager.clone();
        
        if let Some(pm) = &pm_clone {
            let pm_lock = pm.lock().await;
            // 获取所有插件工具
            let tools = pm_lock.get_all_tools();
            let tool_count = tools.len();
            
            // 注册每个插件工具
            for tool in tools {
                let plugin_tool = PluginToolWrapper::new(
                    format!("{}__{}", tool.name, tool.plugin_id),
                    tool.description.clone(),
                    tool.parameters.clone(),
                    tool.plugin_id.clone(),
                    pm.clone(),
                );
                self.register(Box::new(plugin_tool));
            }
            
            tracing::info!("Loaded {} plugin tools", tool_count);
        }
        Ok(())
    }

    fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        let def = tool.definition();
        self.tools.insert(def.name.clone(), tool);
    }

    /// Register the memory tool after Agent construction (needs the Agent's
    /// MemoryProvider). Called from Agent::new().
    pub fn register_memory_tool(&mut self, memory: std::sync::Arc<dyn crate::memory::MemoryProvider>) {
        let tool = Box::new(memory_tool::MemoryTool::new(memory));
        self.register(tool);
    }

    /// Scan `workdir` for `tool.json` files and register each as a ScriptTool.
    /// 保留此方法供测试和外部直接注入用。
    /// 正常展运而言，script tools 应通过插件系统加载。
    #[allow(dead_code)]
    fn load_script_tools_from(&mut self, workdir: &std::path::Path) {
        for st in script_tool::load_script_tools(workdir) {
            let name = st.definition().name.clone();
            tracing::info!("Loaded script tool: {name}");
            self.tools.insert(name, Box::new(st));
        }
    }


    /// Connect to a list of MCP server entries supplied at runtime (e.g. from
    /// a `load_mcp` WebSocket message) and register their tools.
    ///
    /// Returns `(loaded_tool_names, error_strings)`.  Existing tools with
    /// conflicting names are **overwritten** so the client can reload a server
    /// with updated credentials without needing to reconnect.
    pub async fn load_mcp_from_entries(
        &mut self,
        entries: &[crate::mcp_client::McpServerEntry],
    ) -> (Vec<String>, Vec<String>) {
        let (mcp_tools, errors) =
            crate::mcp_client::connect_from_entries(entries).await;
        let mut loaded = Vec::new();
        for tool in mcp_tools {
            let name = tool.definition().name.clone();
            tracing::info!("MCP tool registered (dynamic): {name}");
            loaded.push(name.clone());
            self.tools.insert(name, tool);
        }
        (loaded, errors)
    }

    /// Remove all tools whose name starts with `<prefix>__`.
    ///
    /// Used to unload a specific MCP server without tearing down the whole
    /// connection.  Returns the names of the tools that were removed.
    pub fn unload_mcp_by_prefix(&mut self, prefix: &str) -> Vec<String> {
        let full_prefix = format!("{}__", prefix);
        let to_remove: Vec<String> = self
            .tools
            .keys()
            .filter(|k| k.starts_with(&full_prefix))
            .cloned()
            .collect();
        for key in &to_remove {
            self.tools.remove(key);
        }
        to_remove
    }

    /// Return `(name, description)` pairs for all currently-registered MCP
    /// tools (i.e. those whose name contains `__`, the server-prefix separator).
    pub fn list_mcp_tools(&self) -> Vec<(String, String)> {
        let mut items: Vec<(String, String)> = self
            .tools
            .iter()
            .filter(|(k, _)| k.contains("__"))
            .map(|(_, t)| {
                let def = t.definition();
                (def.name.clone(), def.description.clone())
            })
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }

    /// Update the working directory used by all tools.
    pub fn set_project_dir(&mut self, dir: PathBuf) {
        self.project_dir = dir;
    }

    /// Set the path manager for unified path handling.
    pub fn set_path_manager(&mut self, path_manager: Arc<crate::path_manager::PathManager>) {
        self.path_manager = Some(path_manager.clone());
        // Update project_dir to match path manager's working directory
        self.project_dir = path_manager.working_dir().to_path_buf();
    }

    /// Restrict write/edit tools to paths inside `dir`.
    /// Pass `None` to remove the restriction.
    pub fn set_allowed_dir(&mut self, dir: Option<PathBuf>) {
        self.allowed_dir = dir;
    }

    /// 设置 Hook 事件总线居用引用。由 Agent::set_hook_bus() 调用。
    pub fn set_hook_bus(&mut self, bus: Option<Arc<crate::plugin::hook_bus::HookBus>>) {
        self.hook_bus = bus;
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

    /// Get tool definitions filtered by a set of toolsets.
    ///
    /// Useful for role-based tool restriction: planner gets file_read+search+think,
    /// executor gets file_write+shell+browser, etc.
    pub fn definitions_for_toolsets(&self, toolsets: &[Toolset]) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .filter(|t| {
                t.toolset()
                    .map(|ts| toolsets.contains(&ts))
                    .unwrap_or(true) // Tools without a toolset tag are always included
            })
            .map(|t| t.definition())
            .collect()
    }

    /// Check whether a tool name belongs to a specific toolset.
    pub fn tool_in_toolset(&self, tool_name: &str, toolset: Toolset) -> bool {
        self.tools
            .get(tool_name)
            .and_then(|t| t.toolset())
            .map(|ts| ts == toolset)
            .unwrap_or(false)
    }

    /// Get all active toolsets (those with at least one registered tool).
    pub fn active_toolsets(&self) -> Vec<Toolset> {
        let mut sets: Vec<Toolset> = self.tools
            .values()
            .filter_map(|t| t.toolset())
            .collect();
        sets.sort_by_key(|s| s.label());
        sets.dedup();
        sets
    }

    /// Execute a tool by name.
    ///
    /// Wraps the tool execution in `catch_unwind` so that a panic inside
    /// a tool (e.g. slice-index-out-of-range) is turned into a
    /// `ToolResult::Error` instead of crashing the whole agent.  This
    /// prevents orphaned `tool_use` blocks without matching `tool_result`
    /// in the conversation, which would cause Anthropic API 400 errors.
    ///
    /// ## 3-Layer Persistence
    /// 1. **Memory cache** — read-only tool results are cached; repeated
    ///    identical calls return the cached result immediately.
    /// 2. **Result size limit** — outputs exceeding 20K chars are truncated.
    /// 3. **Session log** — results are recorded to persistent memory by
    ///    the Agent (see `record_tool_to_memory`).
    pub async fn execute(&self, name: &str, input: &serde_json::Value) -> ToolResult {
        // ── Constants ────────────────────────────────────────────────────────
        const WRITE_TOOLS: &[&str] = &["write_file", "edit_file", "multi_edit_file"];

        // ── Layer 1: Cache lookup for read-only tools ─────────────────────
        if cache::ToolResultCache::is_cacheable(name) {
            let args_hash = cache::hash_args(name, input);
            if let Some(cached) = self.result_cache.borrow_mut().get(name, args_hash) {
                tracing::debug!(tool = name, "cache hit");
                return cached;
            }
        }

        // ── Directory guard ───────────────────────────────────────────────────
        // If an allowed_dir is set, reject write/edit tools that try to touch
        // paths outside it.  Read-only tools are not restricted.
        
        if WRITE_TOOLS.contains(&name) {
            if let Some(path_str) = input.get("path").and_then(|v| v.as_str()) {
                // Use path manager if available, otherwise use old logic
                if let Some(ref path_manager) = self.path_manager {
                    match path_manager.check_write_permission(path_str) {
                        Ok(()) => (),
                        Err(err) => return ToolResult::error(err),
                    }
                } else if let Some(ref allowed) = self.allowed_dir {
                    // Old permission check logic
                    let resolved = if Path::new(path_str).is_absolute() {
                        PathBuf::from(path_str)
                    } else {
                        self.project_dir.join(path_str)
                    };
                    // canonicalize() fails for not-yet-existing files; try to canonicalize parent directory
                    let canonical = match resolved.canonicalize() {
                        Ok(canonical) => canonical,
                        Err(_) => {
                            // For new files, try to canonicalize the parent directory
                            if let Some(parent) = resolved.parent() {
                                match parent.canonicalize() {
                                    Ok(canonical_parent) => {
                                        // Parent directory canonicalized successfully
                                        if let Some(filename) = resolved.file_name() {
                                            canonical_parent.join(filename)
                                        } else {
                                            resolved.clone()
                                        }
                                    }
                                    Err(_) => {
                                        // Parent directory also doesn't exist, use resolved path
                                        resolved.clone()
                                    }
                                }
                            } else {
                                // No parent directory (should not happen for file paths)
                                resolved.clone()
                            }
                        }
                    };
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

        // ── tool.before hook（intercepting）─────────────────────────────────────
        // 在执行工具之前发布事件，允许插件 hook 拦截或修改参数。
        // 脚本失败或超时均降级为 Continue，不阻断主流程。
        let patched: Option<serde_json::Value> = if let Some(bus) = &self.hook_bus {
            use crate::plugin::hook_bus::{HookEvent, HookResult};
            let event = HookEvent::new(
                "tool.before",
                "none",
                serde_json::json!({ "tool_name": name, "params": input }),
            );
            match bus.emit_intercepting(event).await {
                HookResult::Cancel { reason } => {
                    return ToolResult::error(format!("[hook] tool blocked: {}", reason));
                }
                HookResult::PatchParams { params } => {
                    let mut merged = input.clone();
                    if let (Some(m), Some(p)) = (merged.as_object_mut(), params.as_object()) {
                        for (k, v) in p {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                    Some(merged)
                }
                // Approved / Continue in tool.before context → proceed as-is
                HookResult::Approved { .. } | HookResult::Continue => None,
            }
        } else {
            None
        };
        let input = patched.as_ref().unwrap_or(input);

        let tool_result = match self.tools.get(name) {
            Some(tool) => {
                use futures::FutureExt; // catch_unwind on futures

                let result = if let Some(ref path_manager) = self.path_manager {
                    // Use path manager if available
                    std::panic::AssertUnwindSafe(
                        tool.execute_with_path_manager(input, path_manager),
                    )
                    .catch_unwind()
                    .await
                } else {
                    // Fall back to old method
                    std::panic::AssertUnwindSafe(
                        tool.execute(input, &self.project_dir),
                    )
                    .catch_unwind()
                    .await
                };

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
            None => {
                // 如果没有找到内置工具，尝试执行插件工具
                if let Some(pm) = &self.plugin_manager {
                    let mut pm_lock = pm.lock().await;
                    match pm_lock.execute_tool(name, input).await {
                        Ok(result) => {
                            // 将插件工具的结果转换为 ToolResult
                            let output = if let Some(output_str) = result.as_str() {
                                output_str.to_string()
                            } else {
                                serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
                            };
                            ToolResult::success(output)
                        }
                        Err(e) => ToolResult::error(format!("Plugin tool execution failed: {}", e)),
                    }
                } else {
                    ToolResult::error(format!("Unknown tool: {}", name))
                }
            }
        };

        // ── tool.after hook（fire-and-forget）────────────────────────────────────
        if let Some(bus) = &self.hook_bus {
            use crate::plugin::hook_bus::HookEvent;
            let preview: String = tool_result.output.chars().take(200).collect();
            bus.emit(HookEvent::new(
                "tool.after",
                "none",
                serde_json::json!({
                    "tool_name": name,
                    "success":        !tool_result.is_error,
                    "output_preview": preview,
                }),
            ));
        }

        // ── Layer 2: Enforce result size limit ────────────────────────────
        let mut final_result = tool_result;
        cache::enforce_result_size_limit(&mut final_result);

        // ── Layer 1: Cache or invalidate ─────────────────────────────────
        if cache::ToolResultCache::is_cacheable(name) && !final_result.is_error {
            let args_hash = cache::hash_args(name, input);
            let file_path = input
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| {
                    if Path::new(s).is_absolute() {
                        PathBuf::from(s)
                    } else {
                        self.project_dir.join(s)
                    }
                });
            self.result_cache.borrow_mut()
                .put(name, args_hash, file_path.as_deref(), final_result.clone());
        }

        // Invalidate cache entries for paths touched by writes
        if WRITE_TOOLS.contains(&name) && !final_result.is_error {
            if let Some(path_str) = input.get("path").and_then(|v| v.as_str()) {
                let path = if Path::new(path_str).is_absolute() {
                    PathBuf::from(path_str)
                } else {
                    self.project_dir.join(path_str)
                };
                self.result_cache.borrow_mut().invalidate_path(&path);
            }
        }

        final_result
    }
}

/// 插件工具包装器
struct PluginToolWrapper {
    name: String,
    description: String,
    /// 来自插件工具定义的 JSON Schema，直接透传给 LLM
    parameters: serde_json::Value,
    plugin_id: String,
    plugin_manager: Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>,
}

/// 将工具名中不符合 `^[a-zA-Z0-9_-]+$` 的字符替换为 `_`。
/// Anthropic 和 OpenAI 兼容 API 均要求工具名满足此规则。
fn sanitize_tool_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

impl PluginToolWrapper {
    fn new(
        name: String,
        description: String,
        parameters: Option<serde_json::Value>,
        plugin_id: String,
        plugin_manager: Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>,
    ) -> Self {
        // 如果插件定义了参数 schema 则使用，否则回退到宽松的 additionalProperties
        let parameters = parameters.unwrap_or_else(|| serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true,
        }));
        Self {
            name: sanitize_tool_name(&name),
            description,
            parameters,
            plugin_id,
            plugin_manager,
        }
    }
}

#[async_trait::async_trait]
impl Tool for PluginToolWrapper {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            // 直接使用插件提供的 schema（已在 new() 中设置默认值）
            parameters: self.parameters.clone(),
        }
    }
    
    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        // 执行插件工具（self.name 已经是 name@plugin_id 格式）
        let mut pm_lock = self.plugin_manager.lock().await;
        match pm_lock.execute_tool(&self.name, input).await {
            Ok(result) => {
                // 将插件工具的结果转换为ToolResult
                let output = if let Some(output_str) = result.as_str() {
                    output_str.to_string()
                } else {
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
                };
                ToolResult::success(output)
            }
            Err(e) => ToolResult::error(format!("Plugin tool execution failed: {}", e)),
        }
    }
    
    async fn execute_with_path_manager(
        &self,
        input: &serde_json::Value,
        _path_manager: &crate::path_manager::PathManager,
    ) -> ToolResult {
        // 插件工具不使用路径管理器，直接调用execute
        self.execute(input, Path::new(".")).await
    }
}

