use std::sync::Arc;

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;

pub struct LoadSkillTool {
    /// 插件管理器（在可用时统一查询项目技能 + 插件技能）
    plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>,
}

impl LoadSkillTool {
    pub fn new(plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>) -> Self {
        Self { plugin_manager }
    }
}

#[async_trait::async_trait]
impl Tool for LoadSkillTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "load_skill".to_string(),
            description: "Load the full content of a project skill by name. Skills are project-specific instructions stored in .agent/skills/*.md. Use this tool when you need detailed guidance from a specific skill listed in the system prompt's 'Available Skills' section.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of the skill to load (e.g. 'Cross Compile', 'Modify Dts Gpio'). Case-insensitive."
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("Missing required parameter: name"),
        };

        // 先查插件管理器（包含 @system 项目技能 + 所有插件技能）
        if let Some(pm) = &self.plugin_manager {
            let pm_lock = pm.lock().await;
            if let Some(skill) = pm_lock.get_skill(name) {
                let source = if skill.plugin_id == "@system" {
                    skill.file_path.to_string_lossy().to_string()
                } else {
                    format!("{} (plugin: {})", skill.file_path.display(), skill.plugin_id)
                };
                return ToolResult::success(format!("# Skill: {} (from {})\n\n{}", skill.name, source, skill.content));
            }
        }

        // 退回：查项目内 .agent/skills/（关闭插件系统时依然可用）
        match crate::skills::load_skill_by_name(project_dir, name) {
            Some(skill) => {
                ToolResult::success(format!("# Skill: {} (from {})\n\n{}", skill.name, skill.source, skill.content))
            }
            None => {
                // 构建帮助信息：列出所有可用技能
                let mut available: Vec<String> = crate::skills::list_skill_names(project_dir);
                if let Some(pm) = &self.plugin_manager {
                    let pm_lock = pm.lock().await;
                    for skill in pm_lock.get_all_skills() {
                        if skill.plugin_id != "@system" {
                            available.push(format!("{} (plugin: {})", skill.name, skill.plugin_id));
                        }
                    }
                }
                if available.is_empty() {
                    ToolResult::error(format!("Skill '{}' not found. No skills available.", name))
                } else {
                    ToolResult::error(format!("Skill '{}' not found. Available: {}", name, available.join(", ")))
                }
            }
        }
    }
    
    async fn execute_with_path_manager(
        &self,
        input: &serde_json::Value,
        path_manager: &crate::path_manager::PathManager,
    ) -> ToolResult {
        // 优先查插件管理器（和 execute 逻辑相同）
        if let Some(pm) = &self.plugin_manager {
            let name = match input.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return ToolResult::error("Missing required parameter: name"),
            };
            let pm_lock = pm.lock().await;
            if let Some(skill) = pm_lock.get_skill(name) {
                let source = if skill.plugin_id == "@system" {
                    skill.file_path.to_string_lossy().to_string()
                } else {
                    format!("{} (plugin: {})", skill.file_path.display(), skill.plugin_id)
                };
                return ToolResult::success(format!("# Skill: {} (from {})\n\n{}", skill.name, source, skill.content));
            }
        }
        self.execute(input, path_manager.working_dir()).await
    }
}
