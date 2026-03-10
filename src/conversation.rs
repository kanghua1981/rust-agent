use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Role in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A content block in a message (text or tool use/result)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },

    /// DeepSeek reasoner thinking / extended thinking content.
    /// Stored in conversation history so it can be echoed back in the next
    /// request as `reasoning_content` (OpenAI-compatible path) or skipped
    /// when building Anthropic-format messages.
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

/// A single message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content,
        }
    }

    pub fn tool_result(tool_use_id: &str, result: &str, is_error: bool) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: result.to_string(),
                is_error: if is_error { Some(true) } else { None },
            }],
        }
    }

    /// Extract text content from this message
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if this message contains tool use requests
    #[allow(dead_code)]
    pub fn has_tool_use(&self) -> bool {
        self.content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
    }

    /// Extract all tool use blocks
    #[allow(dead_code)]
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some((id.as_str(), name.as_str(), input))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Manages the conversation history
#[derive(Debug)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub system_prompt: String,
}

impl Conversation {
    pub fn new(project_dir: &Path) -> Self {
        let mut system_prompt = Self::build_system_prompt(project_dir);

        // Load project summary (from .agent/summary.md)
        if let Some(summary) = crate::summary::load(project_dir) {
            system_prompt.push_str(&crate::summary::to_system_prompt_section(&summary));
            tracing::info!("Loaded project summary into system prompt");
        }

        // Load project skills
        let loaded = crate::skills::load_skills(project_dir);
        if !loaded.is_empty() {
            system_prompt.push_str(&loaded.to_system_prompt_section());
            tracing::info!("Loaded {} skill(s) into system prompt", loaded.len());
        }

        // Load persistent memory
        let mem = crate::memory::Memory::load(project_dir);
        if !mem.is_empty() {
            system_prompt.push_str(&mem.to_system_prompt_section());
            tracing::info!("Loaded {} memory entries into system prompt", mem.entry_count());
        }

        // Add sub-agents information to system prompt
        let models_cfg = crate::model_manager::load();
        if !models_cfg.sub_agents.is_empty() {
            system_prompt.push_str("\n\n## Available Sub-Agents\n");
            system_prompt.push_str("You can delegate specialized tasks to the following sub-agents running in server mode. Use the `call_sub_agent` tool with the corresponding `server_url`.\n\n");
            for (name, sa) in &models_cfg.sub_agents {
                let role_info = if let Some(role) = &sa.role {
                    format!(" (Role: {})", role)
                } else {
                    String::new()
                };
                system_prompt.push_str(&format!("- **{}**: ws://localhost:{}{}\n", name, sa.port, role_info));
            }
            system_prompt.push_str("\nWhen delegating, prefer using the `target_dir` parameter to isolate the sub-agent's work to a specific directory.\n");
        }

        Conversation {
            messages: Vec::new(),
            system_prompt,
        }
    }

    /// Build the system prompt with support for user customization.
    ///
    /// Loading order (later sources append to or override earlier ones):
    ///   1. Built-in default prompt
    ///   2. Global custom prompt: `~/.config/rust_agent/system_prompt.md`
    ///   3. Project custom prompt: `<project>/.agent/system_prompt.md`
    ///
    /// If a custom prompt file starts with `# OVERRIDE`, it completely replaces
    /// all previous prompt content. Otherwise it appends.
    fn build_system_prompt(project_dir: &Path) -> String {
        let mut prompt = Self::default_system_prompt(project_dir);

        // Global custom system prompt
        if let Some(config_dir) = dirs::config_dir() {
            let global_path = config_dir.join("rust_agent").join("system_prompt.md");
            if let Ok(content) = std::fs::read_to_string(&global_path) {
                let content = content.trim();
                if !content.is_empty() {
                    if content.starts_with("# OVERRIDE") {
                        // Strip the marker line and use the rest as full replacement
                        let body = content.strip_prefix("# OVERRIDE").unwrap_or(content).trim();
                        prompt = body.to_string();
                        tracing::info!("Global system_prompt.md OVERRIDES default prompt");
                    } else {
                        prompt.push_str("\n\n");
                        prompt.push_str(content);
                        tracing::info!("Appended global system_prompt.md");
                    }
                }
            }
        }

        // Project-level custom system prompt (takes highest priority)
        let project_path = project_dir.join(".agent").join("system_prompt.md");
        if let Ok(content) = std::fs::read_to_string(&project_path) {
            let content = content.trim();
            if !content.is_empty() {
                if content.starts_with("# OVERRIDE") {
                    let body = content.strip_prefix("# OVERRIDE").unwrap_or(content).trim();
                    prompt = body.to_string();
                    tracing::info!("Project system_prompt.md OVERRIDES all previous prompts");
                } else {
                    prompt.push_str("\n\n");
                    prompt.push_str(content);
                    tracing::info!("Appended project system_prompt.md");
                }
            }
        }

        prompt
    }

    fn default_system_prompt(project_dir: &Path) -> String {
        format!(
            r#"You are an expert AI coding assistant running in a terminal environment.
You have access to tools that let you read files, write files, run commands, search code, and more.

Current working directory: {}
Operating system: {}

Guidelines:
- Use tools to explore and understand the codebase before making changes
- Always read relevant files before editing them
- Make minimal, targeted changes
- Run tests after making changes when possible
- Explain what you're doing and why
- If you're unsure, ask for clarification
- Use the appropriate tool for each task

When writing or editing code:
- Follow existing code style and conventions
- Add appropriate error handling
- Write clean, idiomatic code
- Consider edge cases

Skills management:
- When you discover a reusable workflow, build process, or project-specific procedure
  that would be valuable across sessions, save it as a skill using the `create_skill` tool.
- ALWAYS use the `create_skill` tool to create or update skills. NEVER use `write_file`
  or `edit_file` to directly create/modify files in `.agent/skills/`. The `create_skill`
  tool automatically generates the required YAML frontmatter format.
- Good candidates for skills: build/deploy steps, cross-compilation recipes, hardware
  configuration procedures, testing workflows, and any multi-step process you had to
  figure out by exploring the project.
- Before creating a skill, check the Available Skills list in this prompt to avoid duplicates.
- To read the full content of an existing skill, use the `load_skill` tool."#,
            project_dir.display(),
            std::env::consts::OS
        )
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get messages formatted for the API (excluding system prompt).
    ///
    /// This method merges consecutive messages with the same role into a
    /// single message.  The Anthropic API requires strict user/assistant
    /// alternation and that **all** `tool_result` blocks for a given
    /// assistant response appear in the single next user message.  Without
    /// merging, truncation notices or per-tool-result messages can violate
    /// these constraints and trigger 400 errors such as:
    ///   "tool_use ids were found without tool_result blocks immediately after"
    pub fn api_messages(&self) -> Vec<serde_json::Value> {
        if self.messages.is_empty() {
            return Vec::new();
        }

        let mut merged: Vec<serde_json::Value> = Vec::new();

        for msg in &self.messages {
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };

            // Serialize content blocks for this message.
            // Filter out any None / failed-serialization results rather than
            // letting unwrap_or_default() inject a null element into the
            // content array, which would cause Anthropic 400 errors.
            let blocks: Vec<serde_json::Value> = msg
                .content
                .iter()
                .filter_map(|b| serde_json::to_value(b).ok())
                .collect();

            // Skip messages whose content blocks all failed to serialize.
            if blocks.is_empty() {
                continue;
            }

            // Try to merge with the previous message if roles match
            if let Some(last) = merged.last_mut() {
                if last.get("role").and_then(|r| r.as_str()) == Some(role_str) {
                    // Same role — append content blocks to existing message
                    if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                        arr.extend(blocks);
                        continue;
                    }
                }
            }

            // Different role or first message — create a new entry
            merged.push(serde_json::json!({
                "role": role_str,
                "content": blocks,
            }));
        }

        merged
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
