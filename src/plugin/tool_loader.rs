//! 插件工具加载器
//! 
//! 从插件目录加载工具定义和脚本。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::PluginError;

/// 工具定义
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 工具参数定义（JSON Schema）
    pub parameters: Option<Value>,
    /// 工具脚本路径
    pub script_path: Option<PathBuf>,
    /// 工具脚本内容（如果脚本很小，可以直接加载）
    pub script_content: Option<String>,
    /// 工具类型
    pub tool_type: ToolType,
    /// 所属插件
    pub plugin_id: String,
}

/// 工具类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolType {
    /// Shell脚本工具
    Shell,
    /// Python脚本工具
    Python,
    /// JavaScript脚本工具
    JavaScript,
    /// Rust工具（编译为二进制）
    Rust,
    /// 内置工具（由插件直接实现）
    Builtin,
}

impl ToolType {
    /// 从文件扩展名推断工具类型
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "sh" | "bash" => Some(ToolType::Shell),
            "py" | "python" => Some(ToolType::Python),
            "js" | "javascript" => Some(ToolType::JavaScript),
            "rs" | "rust" => Some(ToolType::Rust),
            _ => None,
        }
    }
    
    /// 获取工具类型的默认文件扩展名
    pub fn default_extension(&self) -> &'static str {
        match self {
            ToolType::Shell => "sh",
            ToolType::Python => "py",
            ToolType::JavaScript => "js",
            ToolType::Rust => "rs",
            ToolType::Builtin => "",
        }
    }
}

/// 工具加载器
#[derive(Debug, Clone)]
pub struct ToolLoader {
    /// 已加载的工具（工具全名 -> 工具定义）
    loaded_tools: HashMap<String, ToolDefinition>,
    /// 插件工具映射（插件ID -> 工具名称列表）
    plugin_tools: HashMap<String, Vec<String>>,
}

impl ToolLoader {
    /// 创建工具加载器
    pub fn new() -> Self {
        Self {
            loaded_tools: HashMap::new(),
            plugin_tools: HashMap::new(),
        }
    }
    
    /// 从插件目录加载工具
    pub fn load_tools_from_plugin(&mut self, plugin_id: &str, plugin_path: &Path) -> Result<Vec<ToolDefinition>, PluginError> {
        let tools_dir = plugin_path.join("tools");
        
        // 检查工具目录是否存在
        if !tools_dir.exists() || !tools_dir.is_dir() {
            return Ok(Vec::new());
        }
        
        let mut tools = Vec::new();
        
        // 遍历工具目录
        let entries = std::fs::read_dir(&tools_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::Io(e))?;
            let path = entry.path();
            
            // 只处理JSON文件（工具定义）
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                match self.load_tool_from_json(&path, plugin_id) {
                    Ok(tool) => {
                        tools.push(tool.clone());
                        self.register_tool(tool, plugin_id)?;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load tool from {:?}: {}", path, e);
                    }
                }
            }
        }
        
        tracing::info!("Loaded {} tools from plugin {}", tools.len(), plugin_id);
        Ok(tools)
    }
    
    /// 从JSON文件加载工具定义
    fn load_tool_from_json(&self, json_path: &Path, plugin_id: &str) -> Result<ToolDefinition, PluginError> {
        // 读取JSON文件
        let json_content = std::fs::read_to_string(json_path)
            .map_err(|e| PluginError::Io(e))?;
        
        let json_value: Value = serde_json::from_str(&json_content)
            .map_err(|e| PluginError::Json(e))?;
        
        // 解析工具定义
        let name = json_value.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PluginError::Validation("Tool name is required".to_string()))?
            .to_string();
        
        let description = json_value.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        let parameters = json_value.get("parameters").cloned();
        
        // 查找对应的脚本文件
        let script_path = self.find_script_file(json_path, &name)?;
        let script_content = if let Some(ref path) = script_path {
            // 如果脚本文件很小，可以预加载内容
            if path.metadata().map(|m| m.len() < 1024 * 10).unwrap_or(false) {
                std::fs::read_to_string(path)
                    .map(Some)
                    .unwrap_or(None)
            } else {
                None
            }
        } else {
            None
        };
        
        // 推断工具类型
        let tool_type = if let Some(ref path) = script_path {
            path.extension()
                .and_then(|ext| ext.to_str())
                .and_then(|ext| ToolType::from_extension(ext))
                .unwrap_or(ToolType::Shell)
        } else {
            ToolType::Builtin
        };
        
        Ok(ToolDefinition {
            name,
            description,
            parameters,
            script_path,
            script_content,
            tool_type,
            plugin_id: plugin_id.to_string(),
        })
    }
    
    /// 查找脚本文件
    fn find_script_file(&self, json_path: &Path, tool_name: &str) -> Result<Option<PathBuf>, PluginError> {
        let dir = json_path.parent().unwrap_or(Path::new("."));
        
        // 尝试查找同名的脚本文件（不同扩展名）
        let possible_extensions = ["sh", "bash", "py", "python", "js", "javascript", "rs", "rust"];
        
        for ext in possible_extensions.iter() {
            let script_path = dir.join(format!("{}.{}", tool_name, ext));
            if script_path.exists() && script_path.is_file() {
                return Ok(Some(script_path));
            }
        }
        
        // 如果没有找到，检查JSON中是否指定了脚本路径
        Ok(None)
    }
    
    /// 注册工具
    fn register_tool(&mut self, tool: ToolDefinition, plugin_id: &str) -> Result<(), PluginError> {
        let tool_full_name = format!("{}@{}", tool.name, plugin_id);
        
        // 检查工具是否已存在
        if self.loaded_tools.contains_key(&tool_full_name) {
            return Err(PluginError::Conflict(format!(
                "Tool {} already registered", tool_full_name
            )));
        }
        
        // 注册工具
        self.loaded_tools.insert(tool_full_name.clone(), tool);
        
        // 更新插件工具映射
        self.plugin_tools
            .entry(plugin_id.to_string())
            .or_insert_with(Vec::new)
            .push(tool_full_name);
        
        Ok(())
    }
    
    /// 获取所有已加载的工具
    pub fn all_tools(&self) -> Vec<&ToolDefinition> {
        self.loaded_tools.values().collect()
    }
    
    /// 按名称获取工具
    pub fn get_tool(&self, tool_name: &str) -> Option<&ToolDefinition> {
        // 首先尝试精确匹配
        if let Some(tool) = self.loaded_tools.get(tool_name) {
            return Some(tool);
        }
        
        // 如果没有@符号，尝试模糊匹配
        if !tool_name.contains('@') {
            // 查找所有匹配的工具
            for (full_name, tool) in &self.loaded_tools {
                if full_name.starts_with(&format!("{}@", tool_name)) {
                    return Some(tool);
                }
            }
        }
        
        None
    }
    
    /// 获取插件的所有工具
    pub fn get_plugin_tools(&self, plugin_id: &str) -> Vec<&ToolDefinition> {
        let mut tools = Vec::new();
        
        if let Some(tool_names) = self.plugin_tools.get(plugin_id) {
            for tool_name in tool_names {
                if let Some(tool) = self.loaded_tools.get(tool_name) {
                    tools.push(tool);
                }
            }
        }
        
        tools
    }
    
    /// 执行工具
    pub async fn execute_tool(&self, tool_name: &str, parameters: &Value) -> Result<Value, PluginError> {
        let tool = self.get_tool(tool_name)
            .ok_or_else(|| PluginError::Load(format!("Tool not found: {}", tool_name)))?;
        
        // 根据工具类型执行
        match tool.tool_type {
            ToolType::Shell => self.execute_shell_tool(tool, parameters).await,
            ToolType::Python => self.execute_python_tool(tool, parameters).await,
            ToolType::JavaScript => self.execute_javascript_tool(tool, parameters).await,
            ToolType::Rust => self.execute_rust_tool(tool, parameters).await,
            ToolType::Builtin => self.execute_builtin_tool(tool, parameters).await,
        }
    }
    
    /// 执行Shell工具
    async fn execute_shell_tool(&self, tool: &ToolDefinition, parameters: &Value) -> Result<Value, PluginError> {
        let script_path = tool.script_path.as_ref()
            .ok_or_else(|| PluginError::Load("Shell tool requires script path".to_string()))?;
        
        // 构建命令参数
        let args = self.build_command_arguments(parameters)?;
        
        // 执行命令
        let output = tokio::process::Command::new("bash")
            .arg(script_path)
            .args(args)
            .output()
            .await
            .map_err(|e| PluginError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        // 解析输出
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        let result = serde_json::json!({
            "success": output.status.success(),
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": stdout,
            "stderr": stderr,
        });
        
        Ok(result)
    }
    
    /// 执行Python工具
    async fn execute_python_tool(&self, tool: &ToolDefinition, parameters: &Value) -> Result<Value, PluginError> {
        let script_path = tool.script_path.as_ref()
            .ok_or_else(|| PluginError::Load("Python tool requires script path".to_string()))?;
        
        // 构建命令参数
        let args = self.build_command_arguments(parameters)?;
        
        // 执行命令
        let output = tokio::process::Command::new("python3")
            .arg(script_path)
            .args(args)
            .output()
            .await
            .map_err(|e| PluginError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        // 解析输出
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        let result = serde_json::json!({
            "success": output.status.success(),
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": stdout,
            "stderr": stderr,
        });
        
        Ok(result)
    }
    
    /// 执行JavaScript工具
    async fn execute_javascript_tool(&self, tool: &ToolDefinition, parameters: &Value) -> Result<Value, PluginError> {
        let script_path = tool.script_path.as_ref()
            .ok_or_else(|| PluginError::Load("JavaScript tool requires script path".to_string()))?;
        
        // 构建命令参数
        let args = self.build_command_arguments(parameters)?;
        
        // 执行命令
        let output = tokio::process::Command::new("node")
            .arg(script_path)
            .args(args)
            .output()
            .await
            .map_err(|e| PluginError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        // 解析输出
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        let result = serde_json::json!({
            "success": output.status.success(),
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": stdout,
            "stderr": stderr,
        });
        
        Ok(result)
    }
    
    /// 执行Rust工具
    async fn execute_rust_tool(&self, tool: &ToolDefinition, parameters: &Value) -> Result<Value, PluginError> {
        // Rust工具需要先编译，这里简化处理
        Err(PluginError::Load("Rust tool execution not yet implemented".to_string()))
    }
    
    /// 执行内置工具
    async fn execute_builtin_tool(&self, tool: &ToolDefinition, parameters: &Value) -> Result<Value, PluginError> {
        // 内置工具需要插件提供实现
        Err(PluginError::Load("Builtin tool execution requires plugin implementation".to_string()))
    }
    
    /// 构建命令参数
    fn build_command_arguments(&self, parameters: &Value) -> Result<Vec<String>, PluginError> {
        let mut args = Vec::new();
        
        if let Some(obj) = parameters.as_object() {
            for (key, value) in obj {
                if let Some(str_val) = value.as_str() {
                    args.push(format!("--{}", key));
                    args.push(str_val.to_string());
                } else if let Some(num) = value.as_number() {
                    args.push(format!("--{}", key));
                    args.push(num.to_string());
                } else if let Some(bool_val) = value.as_bool() {
                    if bool_val {
                        args.push(format!("--{}", key));
                    }
                }
            }
        }
        
        Ok(args)
    }
    
    /// 清除所有已加载的工具
    pub fn clear(&mut self) {
        self.loaded_tools.clear();
        self.plugin_tools.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tool_type_from_extension() {
        assert_eq!(ToolType::from_extension("sh"), Some(ToolType::Shell));
        assert_eq!(ToolType::from_extension("py"), Some(ToolType::Python));
        assert_eq!(ToolType::from_extension("js"), Some(ToolType::JavaScript));
        assert_eq!(ToolType::from_extension("rs"), Some(ToolType::Rust));
        assert_eq!(ToolType::from_extension("txt"), None);
    }
    
    #[test]
    fn test_tool_loader() {
        let loader = ToolLoader::new();
        
        // 测试初始状态
        assert_eq!(loader.all_tools().len(), 0);
        
        // 测试获取不存在的工具
        assert!(loader.get_tool("nonexistent").is_none());
    }
}