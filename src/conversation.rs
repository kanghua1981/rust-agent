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
        let mut system_prompt = Self::default_system_prompt(project_dir);

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

        Conversation {
            messages: Vec::new(),
            system_prompt,
        }
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
- Consider edge cases"#,
            project_dir.display(),
            std::env::consts::OS
        )
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get messages formatted for the API (excluding system prompt)
    pub fn api_messages(&self) -> Vec<serde_json::Value> {
        self.messages
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "role": msg.role,
                    "content": msg.content,
                })
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
