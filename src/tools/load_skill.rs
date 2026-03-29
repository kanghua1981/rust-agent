use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;

pub struct LoadSkillTool;

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

        match crate::skills::load_skill_by_name(project_dir, name) {
            Some(skill) => {
                let header = format!(
                    "# Skill: {} (from {})\n\n",
                    skill.name, skill.source
                );
                ToolResult::success(format!("{}{}", header, skill.content))
            }
            None => {
                let available = crate::skills::list_skill_names(project_dir);
                if available.is_empty() {
                    ToolResult::error(format!(
                        "Skill '{}' not found. No skills are available in .agent/skills/.",
                        name
                    ))
                } else {
                    ToolResult::error(format!(
                        "Skill '{}' not found. Available skills: {}",
                        name,
                        available.join(", ")
                    ))
                }
            }
        }
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("Missing required parameter: name"),
        };

        match crate::skills::load_skill_by_name(path_manager.working_dir(), name) {
            Some(skill) => {
                let header = format!(
                    "# Skill: {} (from {})\n\n",
                    skill.name, skill.source
                );
                ToolResult::success(format!("{}{}", header, skill.content))
            }
            None => {
                let available = crate::skills::list_skill_names(path_manager.working_dir());
                if available.is_empty() {
                    ToolResult::error(format!(
                        "Skill '{}' not found. No skills are available in .agent/skills/.",
                        name
                    ))
                } else {
                    ToolResult::error(format!(
                        "Skill '{}' not found. Available skills: {}",
                        name,
                        available.join(", ")
                    ))
                }
            }
        }
    }
}
