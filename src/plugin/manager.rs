//! 插件管理器
//! 
//! 管理插件的加载、启用、禁用和查询。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use super::metadata::{PluginMeta, PluginPermissions, SystemRequirements, Components};
use super::scope::{PluginScope, ScopeManager};
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
#[derive(Debug, Clone)]
pub struct PluginManager {
    /// 作用域管理器
    scope_manager: Arc<ScopeManager>,
    /// 插件实例映射（插件ID -> 插件实例）
    plugins: HashMap<String, PluginInstance>,
    /// 作用域插件映射（作用域 -> 插件ID列表）
    scope_plugins: HashMap<PluginScope, HashSet<String>>,
    /// 名称插件映射（插件名称 -> 插件ID列表，用于冲突检测）
    name_plugins: HashMap<String, Vec<String>>,
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
        }
    }
    
    /// 获取作用域管理器
    pub fn scope_manager(&self) -> &ScopeManager {
        &self.scope_manager
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
        
        // 检查插件名称冲突
        self.check_plugin_conflict(&meta, scope)?;
        
        // 创建插件实例
        let plugin_id = meta.id();
        let plugin_name = meta.name.clone();
        let plugin_instance = PluginInstance::new(meta, scope, plugin_path.clone());
        
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
        
        tracing::info!("Loaded plugin {} from scope {:?}", plugin_id, scope);
        
        Ok(())
    }
    
    /// 检查插件冲突
    fn check_plugin_conflict(&self, meta: &PluginMeta, new_scope: PluginScope) -> Result<(), PluginError> {
        let plugin_name = &meta.name;
        
        // 检查是否有同名插件
        if let Some(existing_ids) = self.name_plugins.get(plugin_name) {
            for plugin_id in existing_ids {
                if let Some(existing_plugin) = self.plugins.get(plugin_id) {
                    // 检查作用域优先级
                    if new_scope.priority() < existing_plugin.scope.priority() {
                        // 新插件优先级更高，允许加载（会覆盖旧插件）
                        tracing::info!(
                            "Plugin {} in scope {:?} will override existing plugin in scope {:?}",
                            plugin_name, new_scope, existing_plugin.scope
                        );
                    } else if new_scope.priority() > existing_plugin.scope.priority() {
                        // 新插件优先级更低，不允许加载
                        return Err(PluginError::Conflict(format!(
                            "Plugin {} already exists in higher priority scope {:?}",
                            plugin_name, existing_plugin.scope
                        )));
                    } else {
                        // 同一作用域，不允许重复加载
                        return Err(PluginError::Conflict(format!(
                            "Plugin {} already exists in scope {:?}",
                            plugin_name, new_scope
                        )));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 获取所有插件
    pub fn all_plugins(&self) -> Vec<&PluginInstance> {
        self.plugins.values().collect()
    }
    
    /// 获取启用的插件
    pub fn enabled_plugins(&self) -> Vec<&PluginInstance> {
        self.plugins.values()
            .filter(|p| p.is_enabled())
            .collect()
    }
    
    /// 按名称获取插件
    pub fn get_plugin_by_name(&self, name: &str) -> Option<&PluginInstance> {
        // 按优先级顺序查找插件
        for scope in PluginScope::all_scopes() {
            if let Some(plugin_ids) = self.scope_plugins.get(&scope) {
                for plugin_id in plugin_ids {
                    if let Some(plugin) = self.plugins.get(plugin_id) {
                        if plugin.name() == name {
                            return Some(plugin);
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// 按ID获取插件
    pub fn get_plugin_by_id(&self, id: &str) -> Option<&PluginInstance> {
        self.plugins.get(id)
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
        assert_eq!(manager.all_plugins().len(), 0);
        assert_eq!(manager.enabled_plugins().len(), 0);
        
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