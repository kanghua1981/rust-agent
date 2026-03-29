use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;

pub struct CreateSkillTool;

#[async_trait::async_trait]
impl Tool for CreateSkillTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_skill".to_string(),
            description: "Create or update a project skill file in .agent/skills/. Skills are reusable instructions that persist across sessions. The tool automatically handles file naming, directory creation, and proper formatting. Example: name='Cross Compile ARM', description='Steps for cross-compiling to ARM targets', content='## Prerequisites\n...'".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Human-readable skill name (e.g. 'Cross Compile ARM'). Will be converted to a kebab-case filename automatically."
                    },
                    "description": {
                        "type": "string",
                        "description": "A one-line summary of what this skill covers. This is shown in the skill index in the system prompt."
                    },
                    "content": {
                        "type": "string",
                        "description": "The full Markdown content of the skill (instructions, steps, examples, etc.). Do NOT include the YAML frontmatter or title heading — they are added automatically."
                    }
                },
                "required": ["name", "description", "content"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: name"),
        };

        let description = match input.get("description").and_then(|v| v.as_str()) {
            Some(d) if !d.trim().is_empty() => d.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: description"),
        };

        let content = match input.get("content").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: content"),
        };

        // Convert name to kebab-case filename
        let file_stem = to_kebab_case(name);
        if file_stem.is_empty() {
            return ToolResult::error("Skill name must contain at least one alphanumeric character");
        }

        let skills_dir = project_dir.join(".agent").join("skills");

        // Ensure directory exists
        if let Err(e) = std::fs::create_dir_all(&skills_dir) {
            return ToolResult::error(format!("Failed to create skills directory: {}", e));
        }

        let file_path = skills_dir.join(format!("{}.md", file_stem));
        let is_update = file_path.exists();

        // Build the file content with YAML frontmatter:
        //   ---
        //   name: <name>
        //   description: <description>
        //   ---
        //
        //   # Title
        //
        //   content
        let file_content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n{}\n",
            name, description, name, content
        );

        if let Err(e) = std::fs::write(&file_path, &file_content) {
            return ToolResult::error(format!("Failed to write skill file: {}", e));
        }

        let relative_path = format!(".agent/skills/{}.md", file_stem);
        let action = if is_update { "Updated" } else { "Created" };

        ToolResult::success(format!(
            "{} skill '{}' at {}\nThe skill is now available via `load_skill` tool.",
            action, name, relative_path
        ))
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: name"),
        };

        let description = match input.get("description").and_then(|v| v.as_str()) {
            Some(d) if !d.trim().is_empty() => d.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: description"),
        };

        let content = match input.get("content").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c.trim(),
            _ => return ToolResult::error("Missing or empty required parameter: content"),
        };

        // Convert name to kebab-case filename
        let file_stem = to_kebab_case(name);
        if file_stem.is_empty() {
            return ToolResult::error("Skill name must contain at least one alphanumeric character");
        }

        // Build the relative path for permission checking
        let relative_path = format!(".agent/skills/{}.md", file_stem);
        
        // Check write permission
        if let Err(e) = path_manager.check_write_permission(&relative_path) {
            return ToolResult::error(format!("Permission denied: {}", e));
        }

        let skills_dir = path_manager.working_dir().join(".agent").join("skills");

        // Ensure directory exists
        if let Err(e) = std::fs::create_dir_all(&skills_dir) {
            return ToolResult::error(format!("Failed to create skills directory: {}", e));
        }

        let file_path = skills_dir.join(format!("{}.md", file_stem));
        let is_update = file_path.exists();

        // Build the file content with YAML frontmatter:
        //   ---
        //   name: <name>
        //   description: <description>
        //   ---
        //
        //   # Title
        //
        //   content
        let file_content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n{}\n",
            name, description, name, content
        );

        if let Err(e) = std::fs::write(&file_path, &file_content) {
            return ToolResult::error(format!("Failed to write skill file: {}", e));
        }

        let action = if is_update { "Updated" } else { "Created" };

        ToolResult::success(format!(
            "{} skill '{}' at {}\nThe skill is now available via `load_skill` tool.",
            action, name, relative_path
        ))
    }
}

/// Convert a human-readable name to a kebab-case filename stem.
///
/// "Cross Compile ARM" → "cross-compile-arm"
/// "Modify DTS GPIO"   → "modify-dts-gpio"
fn to_kebab_case(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                '-'
            }
        })
        .collect::<String>()
        // Collapse consecutive dashes
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
