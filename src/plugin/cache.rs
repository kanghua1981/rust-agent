//! 插件缓存系统
//! 
//! 缓存插件加载结果，提高后续加载速度。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, Duration};

use serde::{Deserialize, Serialize};

use super::metadata::PluginMeta;
use super::PluginError;

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 插件元数据
    pub meta: PluginMeta,
    /// 插件路径
    pub path: PathBuf,
    /// 缓存时间
    pub cached_at: SystemTime,
    /// 缓存有效期（秒）
    pub ttl: u64,
    /// 插件组件信息
    pub components: PluginComponents,
}

impl CacheEntry {
    /// 创建新的缓存条目
    pub fn new(meta: PluginMeta, path: PathBuf, components: PluginComponents) -> Self {
        Self {
            meta,
            path,
            cached_at: SystemTime::now(),
            ttl: 3600, // 默认1小时
            components,
        }
    }
    
    /// 检查缓存是否过期
    pub fn is_expired(&self) -> bool {
        match self.cached_at.elapsed() {
            Ok(elapsed) => elapsed > Duration::from_secs(self.ttl),
            Err(_) => true, // 如果获取时间失败，认为缓存已过期
        }
    }
    
    /// 获取剩余有效期（秒）
    pub fn remaining_ttl(&self) -> Option<u64> {
        match self.cached_at.elapsed() {
            Ok(elapsed) => {
                if elapsed < Duration::from_secs(self.ttl) {
                    Some(self.ttl - elapsed.as_secs())
                } else {
                    Some(0)
                }
            }
            Err(_) => None,
        }
    }
}

/// 插件组件信息（用于缓存）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// 插件缓存
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCache {
    /// 缓存条目（插件ID -> 缓存条目）
    entries: HashMap<String, CacheEntry>,
    /// 缓存版本
    version: String,
    /// 创建时间
    created_at: SystemTime,
    /// 最后更新时间
    updated_at: SystemTime,
}

impl PluginCache {
    /// 创建新的缓存
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            version: "1.0".to_string(),
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
        }
    }
    
    /// 从文件加载缓存
    pub fn load_from_file(path: &Path) -> Result<Self, PluginError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = std::fs::read_to_string(path)
            .map_err(|e| PluginError::Io(e))?;
        
        let cache: Self = serde_json::from_str(&content)
            .map_err(|e| PluginError::Json(e))?;
        
        Ok(cache)
    }
    
    /// 保存缓存到文件
    pub fn save_to_file(&self, path: &Path) -> Result<(), PluginError> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| PluginError::Json(e))?;
        
        // 确保目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PluginError::Io(e))?;
        }
        
        std::fs::write(path, content)
            .map_err(|e| PluginError::Io(e))?;
        
        Ok(())
    }
    
    /// 添加或更新缓存条目
    pub fn put(&mut self, plugin_id: String, entry: CacheEntry) {
        self.entries.insert(plugin_id, entry);
        self.updated_at = SystemTime::now();
    }
    
    /// 获取缓存条目
    pub fn get(&self, plugin_id: &str) -> Option<&CacheEntry> {
        self.entries.get(plugin_id)
    }
    
    /// 移除缓存条目
    pub fn remove(&mut self, plugin_id: &str) -> Option<CacheEntry> {
        let result = self.entries.remove(plugin_id);
        if result.is_some() {
            self.updated_at = SystemTime::now();
        }
        result
    }
    
    /// 检查缓存条目是否存在且未过期
    pub fn has_valid(&self, plugin_id: &str) -> bool {
        if let Some(entry) = self.entries.get(plugin_id) {
            !entry.is_expired()
        } else {
            false
        }
    }
    
    /// 清理过期缓存
    pub fn cleanup_expired(&mut self) -> usize {
        let before_count = self.entries.len();
        
        self.entries.retain(|_, entry| !entry.is_expired());
        
        let after_count = self.entries.len();
        let removed = before_count - after_count;
        
        if removed > 0 {
            self.updated_at = SystemTime::now();
        }
        
        removed
    }
    
    /// 获取所有缓存条目
    pub fn all_entries(&self) -> Vec<&CacheEntry> {
        self.entries.values().collect()
    }
    
    /// 获取有效缓存条目
    pub fn valid_entries(&self) -> Vec<&CacheEntry> {
        self.entries.values()
            .filter(|entry| !entry.is_expired())
            .collect()
    }
    
    /// 获取缓存统计信息
    pub fn stats(&self) -> CacheStats {
        let mut stats = CacheStats::default();
        
        stats.total = self.entries.len();
        
        for entry in self.entries.values() {
            if entry.is_expired() {
                stats.expired += 1;
            } else {
                stats.valid += 1;
            }
            
            // 统计组件
            if entry.components.has_tools {
                stats.tools += entry.components.tool_count;
            }
            if entry.components.has_skills {
                stats.skills += entry.components.skill_count;
            }
            if entry.components.has_mcp {
                stats.mcp_servers += entry.components.mcp_count;
            }
            if entry.components.has_hooks {
                stats.hooks += entry.components.hook_count;
            }
        }
        
        stats
    }
    
    /// 清除所有缓存
    pub fn clear(&mut self) {
        self.entries.clear();
        self.updated_at = SystemTime::now();
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// 缓存条目总数
    pub total: usize,
    /// 有效缓存条目数
    pub valid: usize,
    /// 过期缓存条目数
    pub expired: usize,
    /// 工具总数
    pub tools: usize,
    /// 技能总数
    pub skills: usize,
    /// MCP服务器总数
    pub mcp_servers: usize,
    /// Hook脚本总数
    pub hooks: usize,
}

/// 缓存管理器
#[derive(Debug, Clone)]
pub struct CacheManager {
    /// 缓存文件路径
    cache_path: PathBuf,
    /// 内存缓存
    cache: PluginCache,
}

impl CacheManager {
    /// 创建缓存管理器
    pub fn new(cache_dir: &Path) -> Result<Self, PluginError> {
        let cache_path = cache_dir.join("plugin_cache.json");
        let cache = PluginCache::load_from_file(&cache_path)?;
        
        Ok(Self {
            cache_path,
            cache,
        })
    }
    
    /// 获取插件缓存条目
    pub fn get_plugin(&self, plugin_id: &str) -> Option<&CacheEntry> {
        if self.cache.has_valid(plugin_id) {
            self.cache.get(plugin_id)
        } else {
            None
        }
    }
    
    /// 缓存插件
    pub fn cache_plugin(&mut self, plugin_id: String, meta: PluginMeta, path: PathBuf, components: PluginComponents) {
        let entry = CacheEntry::new(meta, path, components);
        self.cache.put(plugin_id, entry);
    }
    
    /// 保存缓存到磁盘
    pub fn save(&self) -> Result<(), PluginError> {
        self.cache.save_to_file(&self.cache_path)
    }
    
    /// 定期保存缓存（如果自上次保存后有变化）
    pub fn save_if_modified(&mut self) -> Result<bool, PluginError> {
        // 这里可以添加更复杂的修改检测逻辑
        // 目前简单实现：总是保存
        self.save()?;
        Ok(true)
    }
    
    /// 清理过期缓存
    pub fn cleanup(&mut self) -> usize {
        self.cache.cleanup_expired()
    }
    
    /// 获取缓存统计信息
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
    
    /// 获取缓存文件路径
    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_cache_entry() {
        let meta = PluginMeta {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            permissions: super::super::metadata::PluginPermissions::default(),
            dependencies: HashMap::new(),
            system_requirements: super::super::metadata::SystemRequirements::default(),
            config: HashMap::new(),
            components: super::super::metadata::Components::default(),
        };
        
        let path = PathBuf::from("/tmp/test-plugin");
        let components = PluginComponents::default();
        
        let entry = CacheEntry::new(meta, path, components);
        
        // 新创建的缓存应该未过期
        assert!(!entry.is_expired());
        
        // 剩余有效期应该在TTL附近
        if let Some(remaining) = entry.remaining_ttl() {
            assert!(remaining <= 3600);
        }
    }
    
    #[test]
    fn test_plugin_cache() {
        let mut cache = PluginCache::new();
        
        // 测试初始状态
        assert_eq!(cache.all_entries().len(), 0);
        assert_eq!(cache.valid_entries().len(), 0);
        
        // 测试添加缓存
        let meta = PluginMeta {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            permissions: super::super::metadata::PluginPermissions::default(),
            dependencies: HashMap::new(),
            system_requirements: super::super::metadata::SystemRequirements::default(),
            config: HashMap::new(),
            components: super::super::metadata::Components::default(),
        };
        
        let path = PathBuf::from("/tmp/test-plugin");
        let components = PluginComponents::default();
        let entry = CacheEntry::new(meta, path, components);
        
        cache.put("test-plugin@1.0.0".to_string(), entry);
        
        assert_eq!(cache.all_entries().len(), 1);
        assert!(cache.has_valid("test-plugin@1.0.0"));
        
        // 测试获取缓存
        assert!(cache.get("test-plugin@1.0.0").is_some());
        assert!(cache.get("nonexistent").is_none());
        
        // 测试移除缓存
        cache.remove("test-plugin@1.0.0");
        assert_eq!(cache.all_entries().len(), 0);
    }
    
    #[test]
    fn test_cache_manager() -> Result<(), PluginError> {
        let temp_dir = tempdir().unwrap();
        let cache_dir = temp_dir.path();
        
        let manager = CacheManager::new(cache_dir)?;
        
        // 测试初始状态
        let stats = manager.stats();
        assert_eq!(stats.total, 0);
        
        // 测试缓存文件路径
        assert!(manager.cache_path().ends_with("plugin_cache.json"));
        
        Ok(())
    }
}