//! 插件管理器
//! 
//! 管理插件的加载、启用、禁用和查询。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use super::metadata::PluginMeta;
use super::scope::{PluginScope, ScopeManager};
use super::skill_loader::SkillLoader;
use super::PluginError;

/// 插件状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    /// 已加载但未启用
    Loaded,
    /// 已启用
    Enabled,
    /// 已禁用
    Disabled,
    /// 加载失败
    Failed(String),
}

/// 插件实例
#[derive(Debug, Clone)]
pub struct PluginInstance {
    /// 插件元数据
    pub meta: Arc<PluginMeta>,
    /// 插件作用域
    pub scope: PluginScope,
    /// 插件路径
    pub path: PathBuf,
    /// 插件状态
    pub status: PluginStatus,
    /// 加载时间
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// 插件信息（用于CLI显示）
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// 插件ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件描述
    pub description: String,
    /// 插件作者
    pub author: String,
    /// 是否启用
    pub enabled: bool,
    /// 插件工具列表
    pub tools: Vec<ToolInfo>,
}

/// 工具信息（用于CLI显示）
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
}

impl PluginInstance {
    /// 创建新的插件实例
    pub fn new(meta: PluginMeta, scope: PluginScope, path: PathBuf) -> Self {
        Self {
            meta: Arc::new(meta),
            scope,
            path,
            status: PluginStatus::Loaded,
            loaded_at: chrono::Utc::now(),
        }
    }
    
    /// 获取插件唯一标识符
    pub fn id(&self) -> String {
        self.meta.id()
    }
    
    /// 获取插件名称
    pub fn name(&self) -> &str {
        &self.meta.name
    }
    
    /// 启用插件
    pub fn enable(&mut self) {
        self.status = PluginStatus::Enabled;
    }
    
    /// 禁用插件
    pub fn disable(&mut self) {
        self.status = PluginStatus::Disabled;
    }
    
    /// 检查插件是否启用
    pub fn is_enabled(&self) -> bool {
        self.status == PluginStatus::Enabled
    }
}

/// 插件管理器
#[derive(Debug)]
pub struct PluginManager {
    /// 作用域管理器
    scope_manager: Arc<ScopeManager>,
    /// 插件实例映射（插件ID -> 插件实例）
    plugins: HashMap<String, PluginInstance>,
    /// 作用域插件映射（作用域 -> 插件ID列表）
    scope_plugins: HashMap<PluginScope, HashSet<String>>,
    /// 名称插件映射（插件名称 -> 插件ID列表，用于冲突检测）
    name_plugins: HashMap<String, Vec<String>>,
    /// 工具加载器
    tool_loader: super::tool_loader::ToolLoader,
    /// 技能加载器
    skill_loader: SkillLoader,
    /// Hook 事件总线（由 Agent 和 ToolExecutor 共享同一 Arc）
    pub hook_bus: Arc<super::hook_bus::HookBus>,
}

impl PluginManager {
    /// 创建插件管理器
    pub fn new(project_dir: PathBuf) -> Self {
        let scope_manager = Arc::new(ScopeManager::new(project_dir));
        
        Self {
            scope_manager,
            plugins: HashMap::new(),
            scope_plugins: HashMap::new(),
            name_plugins: HashMap::new(),
            tool_loader: super::tool_loader::ToolLoader::new(),
            skill_loader: SkillLoader::new(),
            hook_bus: Arc::new(super::hook_bus::HookBus::new()),
        }
    }
    
    /// 创建插件管理器并指定作用域
    pub fn new_with_scopes(project_dir: PathBuf, scopes: Vec<PluginScope>) -> Self {
        let scope_manager = Arc::new(ScopeManager::new_with_scopes(project_dir, scopes));
        
        Self {
            scope_manager,
            plugins: HashMap::new(),
            scope_plugins: HashMap::new(),
            name_plugins: HashMap::new(),
            tool_loader: super::tool_loader::ToolLoader::new(),
            skill_loader: SkillLoader::new(),
            hook_bus: Arc::new(super::hook_bus::HookBus::new()),
        }
    }
    
    /// 加载所有作用域的插件
    pub fn load_all_plugins(&mut self) -> Result<(), PluginError> {
        // 确保目录存在
        self.scope_manager.ensure_directories()
            .map_err(|e| PluginError::Io(e))?;
        
        // 按优先级顺序加载所有作用域的插件
        for scope in PluginScope::all_scopes() {
            if let Err(e) = self.load_plugins_in_scope(scope) {
                tracing::warn!("Failed to load plugins in scope {:?}: {}", scope, e);
            }
        }
        
        Ok(())
    }
    
    /// 加载指定作用域的插件
    pub fn load_plugins_in_scope(&mut self, scope: PluginScope) -> Result<(), PluginError> {
        let plugin_dir = self.scope_manager.plugin_dir(scope);
        
        // 检查插件目录是否存在
        if !plugin_dir.exists() || !plugin_dir.is_dir() {
            return Ok(());
        }
        
        // 遍历插件目录
        let entries = std::fs::read_dir(&plugin_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::Io(e))?;
            let plugin_path = entry.path();
            
            // 检查是否为目录
            if !plugin_path.is_dir() {
                continue;
            }
            
            // 尝试加载插件
            if let Err(e) = self.load_plugin_at_path(&plugin_path, scope) {
                tracing::warn!("Failed to load plugin at {:?}: {}", plugin_path, e);
            }
        }
        
        Ok(())
    }
    
    /// 从指定路径加载插件
    pub fn load_plugin_at_path(&mut self, plugin_path: &PathBuf, scope: PluginScope) -> Result<(), PluginError> {
        // 检查插件元数据文件
        let meta_path = plugin_path.join("plugin.toml");
        if !meta_path.exists() {
            return Err(PluginError::Validation(format!(
                "Plugin metadata not found at {:?}", meta_path
            )));
        }
        
        // 加载插件元数据
        let meta = PluginMeta::from_file(&meta_path)
            .map_err(|e| PluginError::MetadataParse(e.to_string()))?;
        
        // 验证插件元数据
        meta.validate()
            .map_err(|e| PluginError::Validation(e))?;
        
        // 检查并解决插件冲突（高优先级会覆盖低优先级同名插件）
        self.check_and_resolve_conflict(&meta, scope)?;
        
        // 创建插件实例并立即启用
        let plugin_id = meta.id();
        let plugin_name = meta.name.clone();
        let mut plugin_instance = PluginInstance::new(meta, scope, plugin_path.clone());
        plugin_instance.enable();
        
        // 注册插件
        self.plugins.insert(plugin_id.clone(), plugin_instance);
        
        // 更新作用域映射
        self.scope_plugins
            .entry(scope)
            .or_insert_with(HashSet::new)
            .insert(plugin_id.clone());
        
        // 更新名称映射
        self.name_plugins
            .entry(plugin_name)
            .or_insert_with(Vec::new)
            .push(plugin_id.clone());
        
        // 加载插件工具
        match self.tool_loader.load_tools_from_plugin(&plugin_id, plugin_path) {
            Ok(tools) => {
                tracing::info!("Loaded {} tools from plugin {}", tools.len(), plugin_id);
            }
            Err(e) => {
                tracing::warn!("Failed to load tools from plugin {}: {}", plugin_id, e);
            }
        }
        
        // 加载插件技能
        match self.skill_loader.load_skills_from_plugin(&plugin_id, plugin_path) {
            Ok(skills) => {
                tracing::info!("Loaded {} skills from plugin {}", skills.len(), plugin_id);
            }
            Err(e) => {
                tracing::warn!("Failed to load skills from plugin {}: {}", plugin_id, e);
            }
        }

        // 注册插件 hook
        self.hook_bus.register_plugin_hooks(&plugin_id, plugin_path);
        
        tracing::info!("Plugin {} loaded and enabled from scope {:?}", plugin_id, scope);
        
        Ok(())
    }
    
    /// 检查并解决插件冲突
    /// - 新插件优先级更高：移除旧插件（含工具/技能），允许加载
    /// - 新插件优先级更低：返回 Err，跳过加载
    /// - 同一作用域同名：返回 Err
    fn check_and_resolve_conflict(&mut self, meta: &PluginMeta, new_scope: PluginScope) -> Result<(), PluginError> {
        let plugin_name = meta.name.clone();
        
        // 克隆 ID 列表以避免与 &mut self 的借用冲突
        let existing_ids: Vec<String> = self.name_plugins
            .get(&plugin_name)
            .cloned()
            .unwrap_or_default();
        
        for plugin_id in existing_ids {
            let existing_scope = match self.plugins.get(&plugin_id) {
                Some(p) => p.scope,
                None => continue,
            };
            
            if new_scope.priority() < existing_scope.priority() {
                // 新插件优先级更高，移除旧插件后允许加载
                tracing::info!(
                    "插件 '{}' 在作用域 {:?} 中覆盖作用域 {:?} 的同名插件",
                    plugin_name, new_scope, existing_scope
                );
                self.remove_plugin_internal(&plugin_id);
            } else if new_scope.priority() > existing_scope.priority() {
                // 新插件优先级更低，跳过加载
                return Err(PluginError::Conflict(format!(
                    "插件 '{}' 已在更高优先级作用域 {:?} 中存在，跳过加载",
                    plugin_name, existing_scope
                )));
            } else {
                // 同一作用域，不允许重复
                return Err(PluginError::Conflict(format!(
                    "插件 '{}' 在作用域 {:?} 中已存在",
                    plugin_name, new_scope
                )));
            }
        }
        
        Ok(())
    }
    
    /// 内部移除插件（含工具、技能、hook 清理），供冲突解决和 unload 使用
    fn remove_plugin_internal(&mut self, plugin_id: &str) {
        if let Some(plugin) = self.plugins.remove(plugin_id) {
            if let Some(scope_plugins) = self.scope_plugins.get_mut(&plugin.scope) {
                scope_plugins.remove(plugin_id);
            }
            let name = plugin.name().to_string();
            if let Some(name_plugins) = self.name_plugins.get_mut(&name) {
                name_plugins.retain(|id| id != plugin_id);
                if name_plugins.is_empty() {
                    self.name_plugins.remove(&name);
                }
            }
            self.tool_loader.unload_plugin_tools(plugin_id);
            self.skill_loader.unload_plugin_skills(plugin_id);
            self.hook_bus.unregister_plugin_hooks(plugin_id);
        }
    }

    /// 返回 HookBus 的 Arc 引用，供 Agent 和 ToolExecutor 共享。
    pub fn get_hook_bus(&self) -> Arc<super::hook_bus::HookBus> {
        self.hook_bus.clone()
    }
    
    /// 加载项目内置技能（AGENT.md / .agent/skills/*.md）注册为 `@system` 伪插件。
    /// 同时扫描 SKILL.md（OpenClaw 格式）。调用方在 load_all_plugins 之后调用此方法，
    /// 使得 `load_skill` 工具可以通过统一接口查询到项目技能和插件技能。
    pub fn load_system_skills(&mut self, project_dir: &std::path::Path) {
        const SYSTEM_PLUGIN: &str = "@system";
        
        let loaded = crate::skills::load_skills(project_dir);
        
        // 1. 完整加载的技能（AGENT.md / SKILL.md）
        for skill in &loaded.skills {
            let desc = skill.content.lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .unwrap_or("")
                .to_string();
            let def = crate::plugin::skill_loader::SkillDefinition {
                name: skill.name.clone(),
                description: desc,
                content: skill.content.clone(),
                file_path: project_dir.join(&skill.source),
                plugin_id: SYSTEM_PLUGIN.to_string(),
                tags: vec![],
            };
            if let Err(e) = self.skill_loader.register_system_skill(def) {
                tracing::debug!("@system skill '{}' already registered: {}", skill.name, e);
            }
        }
        
        // 2. 按需索引的技能（.agent/skills/*.md）—— 加载完整正文后注册
        for entry in &loaded.index {
            if let Some(skill) = crate::skills::load_skill_by_name(project_dir, &entry.name) {
                let def = crate::plugin::skill_loader::SkillDefinition {
                    name: skill.name.clone(),
                    description: entry.description.clone(),
                    content: skill.content.clone(),
                    file_path: project_dir.join(&entry.source),
                    plugin_id: SYSTEM_PLUGIN.to_string(),
                    tags: vec![],
                };
                if let Err(e) = self.skill_loader.register_system_skill(def) {
                    tracing::debug!("@system skill '{}' already registered: {}", skill.name, e);
                }
            }
        }
        
        tracing::info!(
            "@system: registered {} skills from project directory",
            loaded.skills.len() + loaded.index.len()
        );
    }
    
    /// 按名称查找技能（统一入口：项目技能 + 所有插件技能）。
    /// 命中 `@system` 表示项目内置技能，命中其他 plugin_id 表示插件提供的技能。
    pub fn get_skill(&self, name: &str) -> Option<super::skill_loader::SkillDefinition> {
        self.skill_loader.get_skill(name).cloned()
    }
    
    /// 收集所有已加载插件中的 MCP 服务配置，返回 McpServerEntry 列表。
    /// 每个插件的 `mcp/` 目录下的 `.toml` 文件会被扫描，支持两种格式：
    ///   - 单 server：`name = "..."  command = "..."` 等顶级字段
    ///   - 多 server：`[[server]]` 数组（复用 .agent/mcp.toml 格式）
    /// 若插件未启用则跳过。
    pub fn collect_mcp_entries(&self) -> Vec<crate::mcp_client::McpServerEntry> {
        let mut entries = Vec::new();

        for plugin in self.plugins.values() {
            if !plugin.is_enabled() {
                continue;
            }
            let mcp_dir = plugin.path.join("mcp");
            if !mcp_dir.is_dir() {
                continue;
            }

            let Ok(dir_entries) = std::fs::read_dir(&mcp_dir) else { continue };
            for dir_entry in dir_entries.flatten() {
                let path = dir_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };

                // 先尝试多-server 格式 (reuse McpConfig)
                if let Ok(cfg) = toml::from_str::<crate::mcp_client::McpConfig>(&text) {
                    if !cfg.servers.is_empty() {
                        tracing::info!(
                            "Plugin '{}': loaded {} MCP server(s) from {}",
                            plugin.name(), cfg.servers.len(), path.display()
                        );
                        entries.extend(cfg.servers);
                        continue;
                    }
                }
                // 再尝试单-server 格式
                if let Ok(entry) = toml::from_str::<crate::mcp_client::McpServerEntry>(&text) {
                    tracing::info!(
                        "Plugin '{}': loaded MCP server '{}' from {}",
                        plugin.name(), entry.name, path.display()
                    );
                    entries.push(entry);
                } else {
                    tracing::warn!(
                        "Plugin '{}': failed to parse MCP config at {}",
                        plugin.name(), path.display()
                    );
                }
            }
        }

        entries
    }

    /// 收集所有已启用插件的 `system_prompt.md` 内容，按插件加载顺序拼接。
    /// 每段内容以插件名为标题分隔，便于追踪来源。
    /// 若插件未启用或不含该文件则跳过，不报错。
    pub fn collect_system_prompts(&self) -> String {
        let mut result = String::new();
        for plugin in self.plugins.values() {
            if !plugin.is_enabled() {
                continue;
            }
            let prompt_path = plugin.path.join("system_prompt.md");
            let Ok(content) = std::fs::read_to_string(&prompt_path) else {
                continue;
            };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            result.push_str(&format!(
                "\n\n--- Plugin: {} ---\n{}",
                plugin.name(),
                content
            ));
            tracing::info!("Plugin '{}': appended system_prompt.md ({} chars)", plugin.name(), content.len());
        }
        result
    }

    /// 扫描所有已启用插件的 `workspaces.toml`，合并 nodes + peers + cluster token。
    /// 插件中 `[[node]]` 的 `workdir` 若为相对路径，以插件目录为基准展开为绝对路径。
    /// 多个插件均设置了 cluster token 时，取第一个非空值。
    pub fn collect_workspace(&self) -> crate::workspaces::WorkspacesFile {
        use crate::workspaces::WorkspacesFile;
        let mut result = WorkspacesFile::default();

        for plugin in self.plugins.values() {
            if !plugin.is_enabled() {
                continue;
            }
            let ws_path = plugin.path.join("workspaces.toml");
            let Ok(text) = std::fs::read_to_string(&ws_path) else { continue };
            let Ok(mut ws) = toml::from_str::<WorkspacesFile>(&text) else {
                tracing::warn!("Plugin '{}': failed to parse workspaces.toml", plugin.name());
                continue;
            };
            tracing::info!(
                "Plugin '{}': loaded {} node(s), {} peer(s) from workspaces.toml",
                plugin.name(), ws.nodes.len(), ws.peers.len()
            );
            // 相对路径 workdir → 绝对路径
            for node in &mut ws.nodes {
                if let Some(wd) = &node.workdir {
                    if wd.is_relative() {
                        node.workdir = Some(plugin.path.join(wd));
                    }
                }
            }
            result.nodes.extend(ws.nodes);
            result.peers.extend(ws.peers);
            if result.cluster.token.is_none() {
                result.cluster.token = ws.cluster.token;
            }
        }
        result
    }

    /// 获取所有插件工具
    pub fn get_all_tools(&self) -> Vec<super::tool_loader::ToolDefinition> {
        self.tool_loader.all_tools()
            .iter()
            .map(|t| (*t).clone())
            .collect()
    }
    
    /// 执行插件工具
    pub async fn execute_tool(&mut self, tool_name: &str, parameters: &serde_json::Value) -> Result<serde_json::Value, PluginError> {
        self.tool_loader.execute_tool(tool_name, parameters).await
    }
    
    /// 获取所有插件技能
    pub fn get_all_skills(&self) -> Vec<super::skill_loader::SkillDefinition> {
        self.skill_loader.all_skills()
            .into_iter()
            .cloned()
            .collect()
    }
    
    /// 搜索插件技能（按相关性排序）
    pub fn search_skills(&self, query: &str) -> Vec<super::skill_loader::SkillDefinition> {
        self.skill_loader.search_skills(query)
            .into_iter()
            .cloned()
            .collect()
    }
    
    /// 列出所有插件（用于CLI命令）
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins.values()
            .map(|plugin| PluginInfo {
                id: plugin.meta.id(),
                name: plugin.meta.name.clone(),
                version: plugin.meta.version.clone(),
                description: plugin.meta.description.clone(),
                author: plugin.meta.author.clone(),
                enabled: plugin.is_enabled(),
                tools: self.tool_loader.get_plugin_tools(&plugin.meta.id())
                    .iter()
                    .map(|t| ToolInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                    })
                    .collect(),
            })
            .collect()
    }
    
    /// 获取插件信息
    pub fn get_plugin_info(&self, plugin_name: &str) -> Option<PluginInfo> {
        // 首先尝试通过ID查找
        if let Some(plugin) = self.plugins.get(plugin_name) {
            return Some(PluginInfo {
                id: plugin.meta.id(),
                name: plugin.meta.name.clone(),
                version: plugin.meta.version.clone(),
                description: plugin.meta.description.clone(),
                author: plugin.meta.author.clone(),
                enabled: plugin.is_enabled(),
                tools: self.tool_loader.get_plugin_tools(&plugin.meta.id())
                    .iter()
                    .map(|t| ToolInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                    })
                    .collect(),
            });
        }
        
        // 然后尝试通过名称查找
        if let Some(plugin_ids) = self.name_plugins.get(plugin_name) {
            if let Some(plugin_id) = plugin_ids.first() {
                if let Some(plugin) = self.plugins.get(plugin_id) {
                    return Some(PluginInfo {
                        id: plugin.meta.id(),
                        name: plugin.meta.name.clone(),
                        version: plugin.meta.version.clone(),
                        description: plugin.meta.description.clone(),
                        author: plugin.meta.author.clone(),
                        enabled: plugin.is_enabled(),
                        tools: self.tool_loader.get_plugin_tools(&plugin.meta.id())
                            .iter()
                            .map(|t| ToolInfo {
                                name: t.name.clone(),
                                description: t.description.clone(),
                            })
                            .collect(),
                    });
                }
            }
        }
        
        None
    }
    
    /// 启用插件
    pub fn enable_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.enable();
            tracing::info!("Enabled plugin {}", plugin_id);
            Ok(())
        } else {
            Err(PluginError::Load(format!("Plugin not found: {}", plugin_id)))
        }
    }
    
    /// 禁用插件
    pub fn disable_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.disable();
            tracing::info!("Disabled plugin {}", plugin_id);
            Ok(())
        } else {
            Err(PluginError::Load(format!("Plugin not found: {}", plugin_id)))
        }
    }
    
    /// 卸载插件
    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        if let Some(plugin) = self.plugins.remove(plugin_id) {
            // 从作用域映射中移除
            if let Some(scope_plugins) = self.scope_plugins.get_mut(&plugin.scope) {
                scope_plugins.remove(plugin_id);
            }
            
            // 从名称映射中移除
            if let Some(name_plugins) = self.name_plugins.get_mut(plugin.name()) {
                name_plugins.retain(|id| id != plugin_id);
                if name_plugins.is_empty() {
                    self.name_plugins.remove(plugin.name());
                }
            }
            
            // 卸载工具和技能
            self.tool_loader.unload_plugin_tools(plugin_id);
            self.skill_loader.unload_plugin_skills(plugin_id);
            
            tracing::info!("Unloaded plugin {}", plugin_id);
            Ok(())
        } else {
            Err(PluginError::Load(format!("Plugin not found: {}", plugin_id)))
        }
    }
    
    /// 获取插件统计信息
    pub fn stats(&self) -> PluginStats {
        let mut stats = PluginStats::default();
        
        for plugin in self.plugins.values() {
            stats.total += 1;
            
            match plugin.status {
                PluginStatus::Enabled => stats.enabled += 1,
                PluginStatus::Disabled => stats.disabled += 1,
                PluginStatus::Loaded => stats.loaded += 1,
                PluginStatus::Failed(_) => stats.failed += 1,
            }
            
            match plugin.scope {
                PluginScope::Global => stats.global += 1,
                PluginScope::Project => stats.project += 1,
                PluginScope::Temporary => stats.temporary += 1,
            }
        }
        
        stats
    }
}

/// 插件统计信息
#[derive(Debug, Clone, Default)]
pub struct PluginStats {
    /// 插件总数
    pub total: usize,
    /// 已启用插件数
    pub enabled: usize,
    /// 已禁用插件数
    pub disabled: usize,
    /// 已加载但未启用插件数
    pub loaded: usize,
    /// 加载失败插件数
    pub failed: usize,
    /// 全局插件数
    pub global: usize,
    /// 项目插件数
    pub project: usize,
    /// 临时插件数
    pub temporary: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_plugin_manager() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        
        let mut manager = PluginManager::new(project_dir);
        
        // 测试初始状态
        assert_eq!(manager.list_plugins().len(), 0);
        
        // 测试统计信息
        let stats = manager.stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.enabled, 0);
        assert_eq!(stats.disabled, 0);
    }
    
    #[test]
    fn test_plugin_instance() {
        let meta = PluginMeta {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            permissions: PluginPermissions::default(),
            dependencies: HashMap::new(),
            system_requirements: SystemRequirements::default(),
            config: HashMap::new(),
            components: Components::default(),
        };
        
        let path = PathBuf::from("/tmp/test-plugin");
        let instance = PluginInstance::new(meta, PluginScope::Global, path);
        
        assert_eq!(instance.id(), "test-plugin@1.0.0");
        assert_eq!(instance.name(), "test-plugin");
        assert!(!instance.is_enabled());
        
        // 测试启用/禁用
        let mut instance = instance;
        instance.enable();
        assert!(instance.is_enabled());
        
        instance.disable();
        assert!(!instance.is_enabled());
    }
}