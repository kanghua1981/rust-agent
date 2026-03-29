//! 插件加载器
//! 
//! 负责插件的扫描、加载和验证。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::metadata::PluginMeta;
use super::scope::{PluginScope, ScopeManager};
use super::PluginError;

/// 插件加载器
#[derive(Debug, Clone)]
pub struct PluginLoader {
    /// 作用域管理器
    scope_manager: ScopeManager,
    /// 已加载的插件元数据（插件ID -> 插件元数据）
    loaded_plugins: HashMap<String, PluginMeta>,
    /// 插件路径映射（插件ID -> 插件路径）
    plugin_paths: HashMap<String, PathBuf>,
}

impl PluginLoader {
    /// 创建插件加载器
    pub fn new(scope_manager: ScopeManager) -> Self {
        Self {
            scope_manager,
            loaded_plugins: HashMap::new(),
            plugin_paths: HashMap::new(),
        }
    }
    
    /// 扫描并加载所有插件
    pub fn load_all_plugins(&mut self) -> Result<(), PluginError> {
        // 确保目录存在
        self.scope_manager.ensure_directories()
            .map_err(|e| PluginError::Io(e))?;
        
        // 按优先级顺序扫描所有作用域
        for scope in PluginScope::all_scopes() {
            if let Err(e) = self.scope_plugins(scope) {
                tracing::warn!("Failed to scan plugins in scope {:?}: {}", scope, e);
            }
        }
        
        tracing::info!("Loaded {} plugins from all scopes", self.loaded_plugins.len());
        Ok(())
    }
    
    /// 扫描指定作用域的插件
    pub fn scope_plugins(&mut self, scope: PluginScope) -> Result<Vec<PluginMeta>, PluginError> {
        let plugin_dir = self.scope_manager.plugin_dir(scope);
        
        // 检查插件目录是否存在
        if !plugin_dir.exists() || !plugin_dir.is_dir() {
            return Ok(Vec::new());
        }
        
        let mut plugins = Vec::new();
        
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
            match self.load_plugin_from_dir(&plugin_path, scope) {
                Ok(plugin) => {
                    plugins.push(plugin.clone());
                    tracing::debug!("Loaded plugin {} from {:?}", plugin.id(), plugin_path);
                }
                Err(e) => {
                    tracing::warn!("Failed to load plugin from {:?}: {}", plugin_path, e);
                }
            }
        }
        
        Ok(plugins)
    }
    
    /// 从目录加载插件
    pub fn load_plugin_from_dir(&mut self, plugin_dir: &Path, scope: PluginScope) -> Result<PluginMeta, PluginError> {
        // 检查插件元数据文件
        let meta_path = plugin_dir.join("plugin.toml");
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
        
        // 检查插件冲突
        self.check_plugin_conflict(&meta, scope)?;
        
        let plugin_id = meta.id();
        
        // 保存插件元数据和路径
        self.loaded_plugins.insert(plugin_id.clone(), meta.clone());
        self.plugin_paths.insert(plugin_id, plugin_dir.to_path_buf());
        
        Ok(meta)
    }
    
    /// 检查插件冲突
    fn check_plugin_conflict(&self, meta: &PluginMeta, new_scope: PluginScope) -> Result<(), PluginError> {
        let plugin_id = meta.id();
        
        // 检查是否已加载同名插件
        if let Some(existing_meta) = self.loaded_plugins.get(&plugin_id) {
            // 如果插件ID相同，说明是同一插件的不同版本
            // 这里可以根据需要实现版本冲突处理
            return Err(PluginError::Conflict(format!(
                "Plugin {} already loaded", plugin_id
            )));
        }
        
        // 检查同名插件在不同作用域的冲突
        for (existing_id, existing_meta) in &self.loaded_plugins {
            if existing_meta.name == meta.name {
                // 这里可以根据作用域优先级决定是否允许加载
                // 目前简单处理：不允许同名插件
                return Err(PluginError::Conflict(format!(
                    "Plugin with name '{}' already exists as {}", meta.name, existing_id
                )));
            }
        }
        
        Ok(())
    }
    
    /// 获取所有已加载的插件
    pub fn all_plugins(&self) -> Vec<&PluginMeta> {
        self.loaded_plugins.values().collect()
    }
    
    /// 按名称查找插件
    pub fn find_plugin_by_name(&self, name: &str) -> Option<&PluginMeta> {
        self.loaded_plugins.values()
            .find(|meta| meta.name == name)
    }
    
    /// 按ID查找插件
    pub fn find_plugin_by_id(&self, id: &str) -> Option<&PluginMeta> {
        self.loaded_plugins.get(id)
    }
    
    /// 获取插件路径
    pub fn get_plugin_path(&self, plugin_id: &str) -> Option<&PathBuf> {
        self.plugin_paths.get(plugin_id)
    }
    
    /// 获取插件目录内容
    pub fn get_plugin_directory_contents(&self, plugin_id: &str) -> Result<Vec<PathBuf>, PluginError> {
        let plugin_path = self.plugin_paths.get(plugin_id)
            .ok_or_else(|| PluginError::Load(format!("Plugin not found: {}", plugin_id)))?;
        
        let mut contents = Vec::new();
        
        // 遍历插件目录
        let entries = std::fs::read_dir(plugin_path)
            .map_err(|e| PluginError::Io(e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::Io(e))?;
            contents.push(entry.path());
        }
        
        Ok(contents)
    }
    
    /// 检查插件组件
    pub fn check_plugin_components(&self, plugin_id: &str) -> Result<PluginComponents, PluginError> {
        let plugin_path = self.plugin_paths.get(plugin_id)
            .ok_or_else(|| PluginError::Load(format!("Plugin not found: {}", plugin_id)))?;
        
        let mut components = PluginComponents::default();
        
        // 检查工具目录
        let tools_dir = plugin_path.join("tools");
        if tools_dir.exists() && tools_dir.is_dir() {
            components.has_tools = true;
            components.tool_count = self.count_files_in_dir(&tools_dir, ".json")?;
        }
        
        // 检查技能目录
        let skills_dir = plugin_path.join("skills");
        if skills_dir.exists() && skills_dir.is_dir() {
            components.has_skills = true;
            components.skill_count = self.count_files_in_dir(&skills_dir, ".md")?;
        }
        
        // 检查MCP目录
        let mcp_dir = plugin_path.join("mcp");
        if mcp_dir.exists() && mcp_dir.is_dir() {
            components.has_mcp = true;
            components.mcp_count = self.count_files_in_dir(&mcp_dir, ".toml")?;
        }
        
        // 检查Hook目录
        let hooks_dir = plugin_path.join("hooks");
        if hooks_dir.exists() && hooks_dir.is_dir() {
            components.has_hooks = true;
            components.hook_count = self.count_files_in_dir(&hooks_dir, "")?; // 不限制扩展名
        }
        
        // 检查资源目录
        let resources_dir = plugin_path.join("resources");
        if resources_dir.exists() && resources_dir.is_dir() {
            components.has_resources = true;
        }
        
        // 检查示例目录
        let examples_dir = plugin_path.join("examples");
        if examples_dir.exists() && examples_dir.is_dir() {
            components.has_examples = true;
        }
        
        Ok(components)
    }
    
    /// 统计目录中指定扩展名的文件数量
    fn count_files_in_dir(&self, dir: &Path, extension: &str) -> Result<usize, PluginError> {
        let mut count = 0;
        
        if !dir.exists() || !dir.is_dir() {
            return Ok(0);
        }
        
        let entries = std::fs::read_dir(dir)
            .map_err(|e| PluginError::Io(e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::Io(e))?;
            let path = entry.path();
            
            if path.is_file() {
                if extension.is_empty() {
                    count += 1;
                } else if let Some(ext) = path.extension() {
                    if ext == extension {
                        count += 1;
                    }
                }
            }
        }
        
        Ok(count)
    }
    
    /// 清除所有已加载的插件
    pub fn clear(&mut self) {
        self.loaded_plugins.clear();
        self.plugin_paths.clear();
    }
}

/// 插件组件信息
#[derive(Debug, Clone, Default)]
pub struct PluginComponents {
    /// 是否有工具
    pub has_tools: bool,
    /// 工具数量
    pub tool_count: usize,
    /// 是否有技能
    pub has_skills: bool,
    /// 技能数量
    pub skill_count: usize,
    /// 是否有MCP配置
    pub has_mcp: bool,
    /// MCP配置数量
    pub mcp_count: usize,
    /// 是否有Hook脚本
    pub has_hooks: bool,
    /// Hook脚本数量
    pub hook_count: usize,
    /// 是否有资源文件
    pub has_resources: bool,
    /// 是否有示例
    pub has_examples: bool,
}

impl PluginComponents {
    /// 获取组件摘要
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        
        if self.has_tools {
            parts.push(format!("{} tools", self.tool_count));
        }
        
        if self.has_skills {
            parts.push(format!("{} skills", self.skill_count));
        }
        
        if self.has_mcp {
            parts.push(format!("{} MCP servers", self.mcp_count));
        }
        
        if self.has_hooks {
            parts.push(format!("{} hooks", self.hook_count));
        }
        
        if self.has_resources {
            parts.push("resources".to_string());
        }
        
        if self.has_examples {
            parts.push("examples".to_string());
        }
        
        if parts.is_empty() {
            "no components".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_plugin_loader() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        
        let scope_manager = ScopeManager::new(project_dir);
        let mut loader = PluginLoader::new(scope_manager);
        
        // 测试初始状态
        assert_eq!(loader.all_plugins().len(), 0);
        
        // 测试加载所有插件（应该成功，即使目录为空）
        assert!(loader.load_all_plugins().is_ok());
    }
    
    #[test]
    fn test_plugin_components() {
        let components = PluginComponents {
            has_tools: true,
            tool_count: 3,
            has_skills: true,
            skill_count: 2,
            has_mcp: false,
            mcp_count: 0,
            has_hooks: true,
            hook_count: 1,
            has_resources: true,
            has_examples: false,
        };
        
        let summary = components.summary();
        assert!(summary.contains("3 tools"));
        assert!(summary.contains("2 skills"));
        assert!(summary.contains("1 hooks"));
        assert!(summary.contains("resources"));
        assert!(!summary.contains("MCP"));
        assert!(!summary.contains("examples"));
    }
}