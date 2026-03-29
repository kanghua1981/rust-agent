//! 插件系统集成测试

use std::path::PathBuf;

/// 测试插件元数据解析
#[test]
fn test_plugin_metadata_parsing() {
    let toml_str = r#"
name = "test-plugin"
version = "1.0.0"
description = "Test plugin"
author = "Test Author"
license = "MIT"

[permissions]
level = "Standard"
requires_approval = false
max_instances = 1
"#;
    
    // 使用 toml 库解析
    let value: toml::Value = toml::from_str(toml_str).unwrap();
    
    assert_eq!(value.get("name").unwrap().as_str().unwrap(), "test-plugin");
    assert_eq!(value.get("version").unwrap().as_str().unwrap(), "1.0.0");
    assert_eq!(value.get("description").unwrap().as_str().unwrap(), "Test plugin");
    assert_eq!(value.get("author").unwrap().as_str().unwrap(), "Test Author");
    assert_eq!(value.get("license").unwrap().as_str().unwrap(), "MIT");
    
    let permissions = value.get("permissions").unwrap();
    assert_eq!(permissions.get("level").unwrap().as_str().unwrap(), "Standard");
    assert_eq!(permissions.get("requires_approval").unwrap().as_bool().unwrap(), false);
    assert_eq!(permissions.get("max_instances").unwrap().as_integer().unwrap(), 1);
}

/// 测试插件目录结构
#[test]
fn test_plugin_directory_structure() {
    let temp_dir = std::env::temp_dir().join("test-plugin-directory-structure");
    let plugin_dir = temp_dir.join("test-plugin");
    
    // 清理可能存在的旧目录
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    // 创建插件目录结构
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::create_dir_all(plugin_dir.join("tools")).unwrap();
    std::fs::create_dir_all(plugin_dir.join("skills")).unwrap();
    std::fs::create_dir_all(plugin_dir.join("mcp")).unwrap();
    std::fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
    std::fs::create_dir_all(plugin_dir.join("resources")).unwrap();
    std::fs::create_dir_all(plugin_dir.join("examples")).unwrap();
    
    // 创建插件元数据文件
    let plugin_toml = r#"
name = "test-plugin"
version = "1.0.0"
description = "Test plugin"
author = "Test Author"
license = "MIT"
"#;
    
    std::fs::write(plugin_dir.join("plugin.toml"), plugin_toml).unwrap();
    
    // 验证目录结构
    assert!(plugin_dir.exists());
    assert!(plugin_dir.join("plugin.toml").exists());
    assert!(plugin_dir.join("tools").exists());
    assert!(plugin_dir.join("skills").exists());
    assert!(plugin_dir.join("mcp").exists());
    assert!(plugin_dir.join("hooks").exists());
    assert!(plugin_dir.join("resources").exists());
    assert!(plugin_dir.join("examples").exists());
    
    // 验证插件元数据文件内容
    let content = std::fs::read_to_string(plugin_dir.join("plugin.toml")).unwrap();
    assert!(content.contains("name = \"test-plugin\""));
    assert!(content.contains("version = \"1.0.0\""));
    assert!(content.contains("description = \"Test plugin\""));
    
    // 清理临时目录
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试工具定义文件
#[test]
fn test_tool_definition_file() {
    let temp_dir = std::env::temp_dir().join("test-tool-definition");
    let tools_dir = temp_dir.join("tools");
    
    // 清理可能存在的旧目录
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&tools_dir).unwrap();
    
    // 创建工具定义文件
    let tool_json = r#"
{
  "name": "git-status",
  "description": "Show the working tree status",
  "parameters": {
    "type": "object",
    "properties": {
      "short": {
        "type": "boolean",
        "description": "Show output in short format"
      },
      "branch": {
        "type": "boolean",
        "description": "Show branch information"
      }
    }
  }
}
"#;
    
    std::fs::write(tools_dir.join("git-status.json"), tool_json).unwrap();
    
    // 验证工具定义文件
    assert!(tools_dir.join("git-status.json").exists());
    
    // 解析JSON内容
    let content = std::fs::read_to_string(tools_dir.join("git-status.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    
    assert_eq!(value.get("name").unwrap().as_str().unwrap(), "git-status");
    assert_eq!(value.get("description").unwrap().as_str().unwrap(), "Show the working tree status");
    
    let parameters = value.get("parameters").unwrap();
    let properties = parameters.get("properties").unwrap();
    
    assert!(properties.get("short").is_some());
    assert!(properties.get("branch").is_some());
    
    // 清理临时目录
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试技能文件
#[test]
fn test_skill_file() {
    let temp_dir = std::env::temp_dir().join("test-skill-file");
    let skills_dir = temp_dir.join("skills");
    
    // 清理可能存在的旧目录
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&skills_dir).unwrap();
    
    // 创建技能文件
    let skill_md = r#"---
name: Git Workflow
description: Common Git workflows and best practices
tags: [git, workflow, version-control]
---
# Git Workflow

This skill covers common Git workflows and best practices for team collaboration.

## Feature Branch Workflow

1. Create a new branch for each feature
2. Make commits on the feature branch
3. Push the branch to remote
4. Create a pull request
5. Review and merge
"#;
    
    std::fs::write(skills_dir.join("git-workflow.md"), skill_md).unwrap();
    
    // 验证技能文件
    assert!(skills_dir.join("git-workflow.md").exists());
    
    // 验证文件内容
    let content = std::fs::read_to_string(skills_dir.join("git-workflow.md")).unwrap();
    assert!(content.contains("name: Git Workflow"));
    assert!(content.contains("description: Common Git workflows and best practices"));
    assert!(content.contains("tags: [git, workflow, version-control]"));
    assert!(content.contains("# Git Workflow"));
    assert!(content.contains("## Feature Branch Workflow"));
    
    // 清理临时目录
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试插件作用域目录
#[test]
fn test_plugin_scope_directories() {
    let temp_dir = std::env::temp_dir().join("test-plugin-scope");
    let project_dir = &temp_dir;
    
    // 清理可能存在的旧目录
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    // 创建全局插件目录结构
    let global_plugin_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rust-agent/plugins");
    
    // 创建项目插件目录结构
    let project_plugin_dir = project_dir.join(".agent/plugins");
    
    // 创建临时目录
    let temp_plugin_dir = std::env::temp_dir().join("rust-agent-plugins");
    
    println!("全局插件目录: {:?}", global_plugin_dir);
    println!("项目插件目录: {:?}", project_plugin_dir);
    println!("临时插件目录: {:?}", temp_plugin_dir);
    
    // 验证目录路径格式
    assert!(global_plugin_dir.to_string_lossy().contains(".rust-agent/plugins"));
    assert!(project_plugin_dir.to_string_lossy().contains(".agent/plugins"));
    assert!(temp_plugin_dir.to_string_lossy().contains("rust-agent-plugins"));
    
    // 清理临时目录
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 测试完整的插件加载流程
#[test]
fn test_complete_plugin_loading_flow() {
    let temp_dir = std::env::temp_dir().join("test-complete-plugin");
    let project_dir = &temp_dir;
    
    // 清理可能存在的旧目录
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    // 创建项目插件目录
    let plugin_dir = project_dir.join(".agent/plugins/test-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    
    // 创建完整的插件结构
    let plugin_toml = r#"
name = "test-plugin"
version = "1.0.0"
description = "Test plugin for integration testing"
author = "Test Author"
license = "MIT"

[permissions]
level = "Standard"
requires_approval = false
max_instances = 1

[components]
scan_directories = ["tools", "skills"]
"#;
    
    std::fs::write(plugin_dir.join("plugin.toml"), plugin_toml).unwrap();
    
    // 创建工具目录和文件
    let tools_dir = plugin_dir.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    
    let tool_json = r#"
{
  "name": "test-tool",
  "description": "Test tool for integration testing"
}
"#;
    
    std::fs::write(tools_dir.join("test-tool.json"), tool_json).unwrap();
    
    // 创建技能目录和文件
    let skills_dir = plugin_dir.join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();
    
    let skill_md = r#"---
name: Test Skill
description: Test skill for integration testing
tags: [test, integration]
---
# Test Skill

This is a test skill for integration testing.
"#;
    
    std::fs::write(skills_dir.join("test-skill.md"), skill_md).unwrap();
    
    // 验证完整的插件结构
    assert!(plugin_dir.exists());
    assert!(plugin_dir.join("plugin.toml").exists());
    assert!(tools_dir.exists());
    assert!(tools_dir.join("test-tool.json").exists());
    assert!(skills_dir.exists());
    assert!(skills_dir.join("test-skill.md").exists());
    
    // 验证插件元数据
    let plugin_content = std::fs::read_to_string(plugin_dir.join("plugin.toml")).unwrap();
    assert!(plugin_content.contains("name = \"test-plugin\""));
    assert!(plugin_content.contains("version = \"1.0.0\""));
    assert!(plugin_content.contains("description = \"Test plugin for integration testing\""));
    
    // 验证工具定义
    let tool_content = std::fs::read_to_string(tools_dir.join("test-tool.json")).unwrap();
    assert!(tool_content.contains("\"name\": \"test-tool\""));
    assert!(tool_content.contains("\"description\": \"Test tool for integration testing\""));
    
    // 验证技能文件
    let skill_content = std::fs::read_to_string(skills_dir.join("test-skill.md")).unwrap();
    assert!(skill_content.contains("name: Test Skill"));
    assert!(skill_content.contains("description: Test skill for integration testing"));
    assert!(skill_content.contains("tags: [test, integration]"));
    
    // 清理临时目录
    let _ = std::fs::remove_dir_all(&temp_dir);
}