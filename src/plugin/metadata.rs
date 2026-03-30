//! 插件元数据定义和解析
//! 
//! 定义插件元数据结构，解析 plugin.toml 文件。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 插件权限级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginPermissionLevel {
    /// 安全插件：只读操作，无副作用
    Safe,
    /// 标准插件：文件操作，需要基本权限
    Standard,
    /// 危险插件：系统命令，网络访问，需要明确授权
    Dangerous,
}

impl Default for PluginPermissionLevel {
    fn default() -> Self {
        Self::Standard
    }
}

/// 插件权限配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPermissions {
    /// 权限级别
    pub level: PluginPermissionLevel,
    /// 启用时需要客户端确认
    pub requires_approval: bool,
    /// 最大并发实例数
    pub max_instances: u32,
}

impl Default for PluginPermissions {
    fn default() -> Self {
        Self {
            level: PluginPermissionLevel::Standard,
            requires_approval: false,
            max_instances: 1,
        }
    }
}

/// 系统要求
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemRequirements {
    /// Git版本要求
    pub git: Option<String>,
    /// Bash版本要求
    pub bash: Option<String>,
    /// Python版本要求
    pub python: Option<String>,
    /// Node.js版本要求
    pub node: Option<String>,
    /// 其他命令要求
    pub commands: Vec<String>,
}

/// 组件清单
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Components {
    /// 需要扫描的目录列表
    pub scan_directories: Vec<String>,
}

/// 插件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMeta {
    /// 插件标识符（唯一）
    pub name: String,
    /// 语义化版本
    pub version: String,
    /// 插件描述
    pub description: String,
    /// 作者信息
    pub author: String,
    /// 许可证
    pub license: String,
    /// 仓库地址
    pub repository: Option<String>,
    /// 插件权限配置
    #[serde(default)]
    pub permissions: PluginPermissions,
    /// 依赖的其他插件
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,
    /// 系统要求
    #[serde(default)]
    pub system_requirements: SystemRequirements,
    /// 默认配置
    #[serde(default)]
    pub config: std::collections::HashMap<String, toml::Value>,
    /// 组件清单
    #[serde(default)]
    pub components: Components,
}

impl PluginMeta {
    /// 从 TOML 字符串解析插件元数据
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
    
    /// 从文件路径加载插件元数据
    pub fn from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let meta = Self::from_toml(&content)?;
        Ok(meta)
    }
    
    /// 验证插件元数据
    pub fn validate(&self) -> Result<(), String> {
        // 验证名称
        if self.name.is_empty() {
            return Err("插件名称不能为空".to_string());
        }
        
        // 验证名称格式（只允许字母、数字、连字符）
        if !self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("插件名称 '{}' 只能包含字母、数字和连字符", self.name));
        }
        
        // 验证版本格式（语义化版本）
        if !self.version.chars().any(|c| c.is_ascii_digit()) {
            return Err(format!("插件版本 '{}' 格式不正确", self.version));
        }
        
        // 验证描述
        if self.description.is_empty() {
            return Err("插件描述不能为空".to_string());
        }
        
        // 验证作者
        if self.author.is_empty() {
            return Err("插件作者不能为空".to_string());
        }
        
        // 验证许可证
        if self.license.is_empty() {
            return Err("插件许可证不能为空".to_string());
        }
        
        Ok(())
    }
    
    /// 获取插件唯一标识符（仅用 name，字符集限制为 [a-zA-Z0-9_-]，
    /// 以确保作为工具名后缀时符合 Anthropic / OpenAI API 约束）。
    pub fn id(&self) -> String {
        self.name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_meta_from_toml() {
        let toml_str = r#"
name = "git-tools"
version = "1.0.0"
description = "Git version control tools and automation"
author = "Rust Agent Team <team@rust-agent.dev>"
license = "MIT"
repository = "https://github.com/rust-agent/git-tools-plugin"

[permissions]
level = "Standard"
requires_approval = false
max_instances = 1

[dependencies]
file-utils = ">=1.0.0"

[system_requirements]
git = ">=2.30.0"
bash = ">=4.0"
commands = []

[config]
auto_commit = true
commit_template = "feat: ${description}"

[components]
scan_directories = ["mcp", "tools", "skills", "hooks"]
"#;
        
        let meta = PluginMeta::from_toml(toml_str).unwrap();
        assert_eq!(meta.name, "git-tools");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.description, "Git version control tools and automation");
        assert_eq!(meta.permissions.level, PluginPermissionLevel::Standard);
        assert_eq!(meta.permissions.requires_approval, false);
        assert_eq!(meta.dependencies.len(), 1);
        assert_eq!(meta.dependencies.get("file-utils").unwrap(), ">=1.0.0");
        assert_eq!(meta.id(), "git-tools@1.0.0");
    }
    
    #[test]
    fn test_plugin_meta_validation() {
        let mut meta = PluginMeta {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            repository: None,
            permissions: PluginPermissions::default(),
            dependencies: std::collections::HashMap::new(),
            system_requirements: SystemRequirements::default(),
            config: std::collections::HashMap::new(),
            components: Components::default(),
        };
        
        // 验证成功
        assert!(meta.validate().is_ok());
        
        // 验证失败：空名称
        meta.name = "".to_string();
        assert!(meta.validate().is_err());
        meta.name = "test-plugin".to_string();
        
        // 验证失败：无效名称
        meta.name = "test_plugin".to_string(); // 包含下划线
        assert!(meta.validate().is_err());
        meta.name = "test-plugin".to_string();
        
        // 验证失败：空描述
        meta.description = "".to_string();
        assert!(meta.validate().is_err());
        meta.description = "Test plugin".to_string();
        
        // 验证失败：空作者
        meta.author = "".to_string();
        assert!(meta.validate().is_err());
        meta.author = "Test Author".to_string();
        
        // 验证失败：空许可证
        meta.license = "".to_string();
        assert!(meta.validate().is_err());
        meta.license = "MIT".to_string();
    }
}