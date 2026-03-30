//! 插件作用域管理
//! 
//! 定义插件作用域类型，管理不同作用域的插件加载优先级。

use std::path::PathBuf;

/// 插件作用域
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginScope {
    /// 全局作用域：用户级别，所有项目共享
    Global,
    /// 项目作用域：项目级别，仅当前项目使用
    Project,
    /// 临时作用域：会话级别，内存中不持久化
    Temporary,
}

impl PluginScope {
    /// 获取作用域优先级（数值越小优先级越高）
    pub fn priority(&self) -> u8 {
        match self {
            PluginScope::Temporary => 0,  // 最高优先级
            PluginScope::Project => 1,
            PluginScope::Global => 2,     // 最低优先级
        }
    }
    
    /// 获取作用域目录路径
    pub fn directory(&self, base_dirs: &BaseDirectories) -> PathBuf {
        match self {
            PluginScope::Global => base_dirs.global_plugin_dir.clone(),
            PluginScope::Project => base_dirs.project_dir.join(".agent/plugins"),
            PluginScope::Temporary => base_dirs.temp_dir.clone(),
        }
    }
    
    /// 获取所有作用域（按优先级排序）
    pub fn all_scopes() -> Vec<PluginScope> {
        vec![
            PluginScope::Temporary,
            PluginScope::Project,
            PluginScope::Global,
        ]
    }
    
    /// 获取作用域名称
    pub fn name(&self) -> &'static str {
        match self {
            PluginScope::Global => "global",
            PluginScope::Project => "project",
            PluginScope::Temporary => "temporary",
        }
    }
    
    /// 从字符串解析作用域
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "global" => Some(PluginScope::Global),
            "project" => Some(PluginScope::Project),
            "temporary" => Some(PluginScope::Temporary),
            _ => None,
        }
    }
}

/// 基础目录结构
#[derive(Debug, Clone)]
pub struct BaseDirectories {
    /// 全局插件目录
    pub global_plugin_dir: PathBuf,
    /// 项目目录
    pub project_dir: PathBuf,
    /// 临时目录
    pub temp_dir: PathBuf,
}

impl BaseDirectories {
    /// 创建基础目录结构
    pub fn new(project_dir: PathBuf) -> Self {
        // Global plugins live inside the XDG config dir so that the container
        // bind-mount of ~/.config/rust_agent/ automatically covers them.
        let global_plugin_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config"))
            .join("rust_agent/plugins");

        let temp_dir = std::env::temp_dir().join("rust-agent-plugins");
        
        Self {
            global_plugin_dir,
            project_dir,
            temp_dir,
        }
    }
    
    /// 确保所有目录存在
    pub fn ensure_directories(&self) -> std::io::Result<()> {
        // 创建全局插件目录
        if !self.global_plugin_dir.exists() {
            std::fs::create_dir_all(&self.global_plugin_dir)?;
        }
        
        // 创建项目插件目录
        let project_plugin_dir = self.project_dir.join(".agent/plugins");
        if !project_plugin_dir.exists() {
            std::fs::create_dir_all(&project_plugin_dir)?;
        }
        
        // 创建临时目录
        if !self.temp_dir.exists() {
            std::fs::create_dir_all(&self.temp_dir)?;
        }
        
        Ok(())
    }
}

/// 作用域管理器
#[derive(Debug, Clone)]
pub struct ScopeManager {
    base_dirs: BaseDirectories,
    /// 限制加载的作用域范围。None 表示加载全部作用域。
    active_scopes: Option<Vec<PluginScope>>,
}

impl ScopeManager {
    /// 创建作用域管理器（加载全部作用域）
    pub fn new(project_dir: PathBuf) -> Self {
        let base_dirs = BaseDirectories::new(project_dir);
        Self { base_dirs, active_scopes: None }
    }
    
    /// 创建作用域管理器并限制到指定的作用域集合
    pub fn new_with_scopes(project_dir: PathBuf, scopes: Vec<PluginScope>) -> Self {
        let base_dirs = BaseDirectories::new(project_dir);
        let active_scopes = if scopes.is_empty() { None } else { Some(scopes) };
        Self { base_dirs, active_scopes }
    }
    
    /// 获取基础目录
    pub fn base_dirs(&self) -> &BaseDirectories {
        &self.base_dirs
    }
    
    /// 确保所有目录存在
    pub fn ensure_directories(&self) -> std::io::Result<()> {
        self.base_dirs.ensure_directories()
    }
    
    /// 获取指定作用域的插件目录
    pub fn plugin_dir(&self, scope: PluginScope) -> PathBuf {
        scope.directory(&self.base_dirs)
    }
    
    /// 获取所有作用域的插件目录（按优先级排序）
    /// 若通过 `new_with_scopes` 限制了作用域，则只返回指定作用域的目录。
    pub fn all_plugin_dirs(&self) -> Vec<(PluginScope, PathBuf)> {
        PluginScope::all_scopes()
            .into_iter()
            .filter(|scope| {
                self.active_scopes.as_ref()
                    .map_or(true, |active| active.contains(scope))
            })
            .map(|scope| (scope, scope.directory(&self.base_dirs)))
            .collect()
    }
    
    /// 检查插件目录是否存在
    pub fn plugin_dir_exists(&self, scope: PluginScope) -> bool {
        let dir = scope.directory(&self.base_dirs);
        dir.exists() && dir.is_dir()
    }
    
    /// 获取插件文件路径
    pub fn plugin_path(&self, scope: PluginScope, plugin_name: &str) -> PathBuf {
        scope.directory(&self.base_dirs).join(plugin_name)
    }
    
    /// 获取插件元数据文件路径
    pub fn plugin_meta_path(&self, scope: PluginScope, plugin_name: &str) -> PathBuf {
        self.plugin_path(scope, plugin_name).join("plugin.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_plugin_scope_priority() {
        assert_eq!(PluginScope::Temporary.priority(), 0);
        assert_eq!(PluginScope::Project.priority(), 1);
        assert_eq!(PluginScope::Global.priority(), 2);
    }
    
    #[test]
    fn test_plugin_scope_from_str() {
        assert_eq!(PluginScope::from_str("global"), Some(PluginScope::Global));
        assert_eq!(PluginScope::from_str("project"), Some(PluginScope::Project));
        assert_eq!(PluginScope::from_str("temporary"), Some(PluginScope::Temporary));
        assert_eq!(PluginScope::from_str("invalid"), None);
        
        // 测试大小写不敏感
        assert_eq!(PluginScope::from_str("GLOBAL"), Some(PluginScope::Global));
        assert_eq!(PluginScope::from_str("Project"), Some(PluginScope::Project));
    }
    
    #[test]
    fn test_scope_manager() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        
        let manager = ScopeManager::new(project_dir.clone());
        
        // 测试目录获取
        let global_dir = manager.plugin_dir(PluginScope::Global);
        let project_dir_path = manager.plugin_dir(PluginScope::Project);
        let temp_dir_path = manager.plugin_dir(PluginScope::Temporary);
        
        assert!(global_dir.to_string_lossy().contains(".rust-agent/plugins"));
        assert!(project_dir_path.to_string_lossy().contains(".agent/plugins"));
        assert!(temp_dir_path.to_string_lossy().contains("rust-agent-plugins"));
        
        // 测试所有目录
        let all_dirs = manager.all_plugin_dirs();
        assert_eq!(all_dirs.len(), 3);
        
        // 测试优先级排序
        assert_eq!(all_dirs[0].0, PluginScope::Temporary);
        assert_eq!(all_dirs[1].0, PluginScope::Project);
        assert_eq!(all_dirs[2].0, PluginScope::Global);
    }
}