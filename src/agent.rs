//! Agent core: orchestrates LLM calls, tool execution, streaming,
//! context window management, and operation confirmation.

use anyhow::Result;

use crate::config::{Config, Provider};
use crate::confirm::{self, ConfirmAction};
use crate::context;
use crate::conversation::{ContentBlock, Conversation, Message};
use crate::diff;
use crate::llm::{self, LlmClient};
use crate::memory::Memory;
use crate::streaming;
use crate::tools::ToolExecutor;
use crate::ui;

/// The main Agent that orchestrates LLM calls and tool execution
pub struct Agent {
    pub config: Config,
    client: Box<dyn LlmClient>,
    tool_executor: ToolExecutor,
    pub conversation: Conversation,
    pub memory: Memory,
    total_input_tokens: u64,
    total_output_tokens: u64,
    session_id: Option<String>,
}

impl Agent {
    pub fn new(config: Config) -> Self {
        let client = llm::create_client(&config);
        let memory = Memory::load(
            &std::env::current_dir().unwrap_or_default(),
        );
        Agent {
            config,
            client,
            tool_executor: ToolExecutor::new(),
            conversation: Conversation::new(),
            memory,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_id: None,
        }
    }

    /// Create agent with a restored conversation
    pub fn with_conversation(config: Config, conversation: Conversation, session_id: String) -> Self {
        let client = llm::create_client(&config);
        let memory = Memory::load(
            &std::env::current_dir().unwrap_or_default(),
        );
        Agent {
            config,
            client,
            tool_executor: ToolExecutor::new(),
            conversation,
            memory,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_id: Some(session_id),
        }
    }

    /// Get or create a session ID for persistence
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Set session ID
    pub fn set_session_id(&mut self, id: String) {
        self.session_id = Some(id);
    }

    /// Process a user message and return the final text response.
    /// This handles the full agent loop: send message → receive response →
    /// if tool use → execute tools → send results → repeat until done.
    pub async fn process_message(&mut self, user_input: &str) -> Result<String> {
        // Add user message
        self.conversation.add_message(Message::user(user_input));

        // Check context window before sending
        self.check_and_manage_context();

        let tool_defs = self.tool_executor.definitions();
        let mut iterations = 0;
        let max_iterations = self.config.max_tool_iterations;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                ui::print_warning(&format!(
                    "Reached maximum tool iterations ({}). Stopping.",
                    max_iterations
                ));
                break;
            }

            // Show thinking indicator
            ui::print_thinking();

            // Send to LLM (streaming for Anthropic, regular for others)
            let response = if self.config.provider == Provider::Anthropic {
                // Use streaming API
                streaming::stream_anthropic_response(
                    &self.config,
                    &self.conversation,
                    &tool_defs,
                )
                .await?
            } else {
                let resp = self.client
                    .send_message(&self.conversation, &tool_defs)
                    .await?;
                // Print text content for non-streaming providers
                for block in &resp.content {
                    if let ContentBlock::Text { text } = block {
                        if !text.is_empty() {
                            ui::print_assistant_text(text);
                        }
                    }
                }
                resp
            };

            // Track token usage
            if let Some(ref usage) = response.usage {
                self.total_input_tokens += usage.input_tokens as u64;
                self.total_output_tokens += usage.output_tokens as u64;
            }

            // Process the response
            let has_tool_use = response
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse { .. }));

            // Add assistant message to conversation
            let assistant_msg = Message::assistant(response.content.clone());
            self.conversation.add_message(assistant_msg);

            // If no tool use, we're done
            if !has_tool_use {
                break;
            }

            // Execute tools (with confirmation for dangerous operations)
            let tool_uses: Vec<_> = response
                .content
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::ToolUse { id, name, input } = block {
                        Some((id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            for (tool_id, tool_name, tool_input) in tool_uses {
                // Check if this tool needs confirmation
                if needs_confirmation(&tool_name) {
                    let action = build_confirm_action(&tool_name, &tool_input);
                    if !confirm::should_proceed(&action) {
                        // User declined - send a "skipped" result back to LLM
                        self.conversation.add_message(Message::tool_result(
                            &tool_id,
                            "User declined to execute this operation.",
                            true,
                        ));
                        continue;
                    }
                }

                ui::print_tool_use(&tool_name, &tool_input);

                // For edit_file and write_file, show diff preview
                let result = if tool_name == "edit_file" || tool_name == "write_file" {
                    self.execute_with_diff(&tool_name, &tool_input).await
                } else {
                    self.tool_executor.execute(&tool_name, &tool_input).await
                };

                ui::print_tool_result(&tool_name, &result);

                // Record to persistent memory
                self.record_tool_to_memory(&tool_name, &tool_input, &result);

                // Add tool result to conversation
                self.conversation.add_message(Message::tool_result(
                    &tool_id,
                    &result.output,
                    result.is_error,
                ));
            }

            // Check context window after tool results
            self.check_and_manage_context();

            // Check stop reason
            if let Some(ref reason) = response.stop_reason {
                if reason == "end_turn" && !has_tool_use {
                    break;
                }
            }
        }

        // Return the final text response
        let final_text = self
            .conversation
            .messages
            .iter()
            .rev()
            .find(|m| m.role == crate::conversation::Role::Assistant)
            .map(|m| m.text_content())
            .unwrap_or_default();

        Ok(final_text)
    }

    /// Execute a tool with diff preview for file modifications
    async fn execute_with_diff(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> crate::tools::ToolResult {
        let path = tool_input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let resolved = resolve_tool_path(path);
        let resolved_str = resolved.display().to_string();

        // Read old content if file exists
        let old_content = tokio::fs::read_to_string(&resolved_str).await.ok();

        // Execute the tool
        let result = self.tool_executor.execute(tool_name, tool_input).await;

        // If successful, show diff
        if !result.is_error {
            if let Ok(new_content) = tokio::fs::read_to_string(&resolved_str).await {
                match (tool_name, &old_content) {
                    ("edit_file", Some(old)) => {
                        diff::print_diff(path, old, &new_content);
                    }
                    ("write_file", Some(old)) => {
                        diff::print_diff(path, old, &new_content);
                    }
                    ("write_file", None) => {
                        diff::print_diff(path, "", &new_content);
                    }
                    _ => {}
                }
            }
        }

        result
    }

    /// Check context window and truncate if needed
    fn check_and_manage_context(&mut self) {
        let status = context::check_context(&self.conversation, &self.config.model);

        if status.needs_truncation {
            ui::print_context_warning(
                status.usage_percent,
                status.estimated_tokens,
                status.max_tokens,
            );
            context::truncate_conversation(&mut self.conversation, &self.config.model);
        }
    }

    /// Get total token usage
    pub fn token_usage(&self) -> (u64, u64) {
        (self.total_input_tokens, self.total_output_tokens)
    }

    /// Reset the conversation
    pub fn reset(&mut self) {
        self.conversation.clear();
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
    }

    /// Record a tool action to persistent memory.
    fn record_tool_to_memory(
        &mut self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        result: &crate::tools::ToolResult,
    ) {
        let path = tool_input.get("path").and_then(|v| v.as_str()).unwrap_or("");

        match tool_name {
            "read_file" => {
                if !path.is_empty() {
                    self.memory.touch_file(path, "read");
                }
            }
            "write_file" => {
                if !path.is_empty() && !result.is_error {
                    let lines = tool_input
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|c| c.lines().count())
                        .unwrap_or(0);
                    self.memory.touch_file(path, &format!("written ({} lines)", lines));
                    self.memory.log_action(&format!("wrote {}", path));
                }
            }
            "edit_file" => {
                if !path.is_empty() && !result.is_error {
                    self.memory.touch_file(path, "edited");
                    self.memory.log_action(&format!("edited {}", path));
                }
            }
            "run_command" => {
                if !result.is_error {
                    let cmd = tool_input
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    // Keep command log short
                    let short_cmd = if cmd.len() > 60 {
                        format!("{}...", &cmd[..57])
                    } else {
                        cmd.to_string()
                    };
                    self.memory.log_action(&format!("ran `{}`", short_cmd));
                }
            }
            _ => {}
        }

        // Auto-save memory (silent)
        if let Err(e) = self.memory.save() {
            tracing::warn!("Failed to save memory: {}", e);
        }
    }

    /// Save memory to disk (public, for use from CLI)
    #[allow(dead_code)]
    pub fn save_memory(&self) {
        if let Err(e) = self.memory.save() {
            tracing::warn!("Failed to save memory: {}", e);
        }
    }
}

/// Resolve a path (relative to cwd or absolute)
fn resolve_tool_path(path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// Check if a tool action needs user confirmation
fn needs_confirmation(tool_name: &str) -> bool {
    matches!(tool_name, "write_file" | "edit_file" | "run_command")
}

/// Build a ConfirmAction from tool name and input
fn build_confirm_action(tool_name: &str, input: &serde_json::Value) -> ConfirmAction {
    match tool_name {
        "write_file" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let lines = input
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| c.lines().count())
                .unwrap_or(0);
            ConfirmAction::WriteFile { path, lines }
        }
        "edit_file" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            ConfirmAction::EditFile { path }
        }
        "run_command" => {
            let command = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            ConfirmAction::RunCommand { command }
        }
        _ => ConfirmAction::RunCommand {
            command: format!("{}: {}", tool_name, input),
        },
    }
}
