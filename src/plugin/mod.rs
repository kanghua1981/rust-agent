//! 插件系统核心模块
//! 
//! 提供插件加载、管理、执行等功能，支持多作用域插件管理。

pub mod metadata;
pub mod scope;
pub mod manager;
pub mod loader;
pub mod tool_loader;
pub mod skill_loader;
pub mod cache;

// 重新导出常用类型
pub use metadata::PluginMeta;
pub use scope::PluginScope;
pub use manager::PluginManager;
pub use loader::PluginLoader;

/// 插件系统错误类型
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("插件元数据解析失败: {0}")]
    MetadataParse(String),
    
    #[error("插件验证失败: {0}")]
    Validation(String),
    
    #[error("插件加载失败: {0}")]
    Load(String),
    
    #[error("插件冲突: {0}")]
    Conflict(String),
    
    #[error("插件依赖未满足: {0}")]
    Dependency(String),
    
    #[error("权限不足: {0}")]
    Permission(String),
    
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("TOML解析错误: {0}")]
    Toml(#[from] toml::de::Error),
    
    #[error("JSON解析错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 插件系统结果类型
pub type PluginResult<T> = Result<T, PluginError>;