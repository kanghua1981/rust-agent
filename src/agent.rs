//! Agent core: orchestrates LLM calls, tool execution, streaming,
//! context window management, and operation confirmation.

use std::sync::Arc;

use anyhow::Result;

use crate::config::{Config, Provider};
use crate::confirm::ConfirmAction;
use crate::context;
use crate::conversation::{ContentBlock, Conversation, Message};
use crate::llm::{self, LlmClient};
use crate::memory::Memory;
use crate::output::AgentOutput;
use crate::streaming;
use crate::tools::ToolExecutor;

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
    output: Arc<dyn AgentOutput>,
}

impl Agent {
    pub fn new(config: Config, output: Arc<dyn AgentOutput>) -> Self {
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
            output,
        }
    }

    /// Create agent with a restored conversation
    pub fn with_conversation(config: Config, conversation: Conversation, session_id: String, output: Arc<dyn AgentOutput>) -> Self {
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
            output,
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
                self.output.on_warning(&format!(
                    "Reached maximum tool iterations ({}). Stopping.",
                    max_iterations
                ));
                break;
            }

            // Show thinking indicator
            self.output.on_thinking();

            // Send to LLM (streaming for Anthropic, regular for others)
            let response = if self.config.provider == Provider::Anthropic {
                // Use streaming API
                streaming::stream_anthropic_response(
                    &self.config,
                    &self.conversation,
                    &tool_defs,
                    &*self.output,
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
                            self.output.on_assistant_text(text);
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
                    if !self.output.confirm(&action) {
                        // User declined - send a "skipped" result back to LLM
                        self.conversation.add_message(Message::tool_result(
                            &tool_id,
                            "User declined to execute this operation.",
                            true,
                        ));
                        continue;
                    }
                }

                self.output.on_tool_use(&tool_name, &tool_input);

                // For edit_file and write_file, show diff preview
                let result = if tool_name == "edit_file" || tool_name == "write_file" {
                    self.execute_with_diff(&tool_name, &tool_input).await
                } else {
                    self.tool_executor.execute(&tool_name, &tool_input).await
                };

                self.output.on_tool_result(&tool_name, &result);

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
                        self.output.on_diff(path, old, &new_content);
                    }
                    ("write_file", Some(old)) => {
                        self.output.on_diff(path, old, &new_content);
                    }
                    ("write_file", None) => {
                        self.output.on_diff(path, "", &new_content);
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
            self.output.on_context_warning(
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

    /// Scan the project directory and use the LLM to generate a project summary.
    /// The summary is saved to `.agent/summary.md` so subsequent sessions skip this step.
    pub async fn generate_project_summary(&mut self) -> Result<String> {
        let cwd = std::env::current_dir().unwrap_or_default();

        // Step 1: Scan directory structure (depth 2)
        let dir_result = self
            .tool_executor
            .execute(
                "list_directory",
                &serde_json::json!({ "path": ".", "recursive": true }),
            )
            .await;
        let dir_tree = if dir_result.is_error {
            "(failed to list directory)".to_string()
        } else {
            dir_result.output
        };

        // Step 2: Try to read key project files for extra context
        let key_files = [
            "README.md",
            "Cargo.toml",
            "package.json",
            "Makefile",
            "CMakeLists.txt",
            "pyproject.toml",
            "go.mod",
            "AGENT.md",
        ];
        let mut file_contents = Vec::new();
        for filename in &key_files {
            let path = cwd.join(filename);
            if path.exists() {
                let read_result = self
                    .tool_executor
                    .execute(
                        "read_file",
                        &serde_json::json!({
                            "path": filename,
                            "max_lines": 200
                        }),
                    )
                    .await;
                if !read_result.is_error {
                    file_contents.push(format!("--- {} ---\n{}", filename, read_result.output));
                }
            }
        }

        let files_context = if file_contents.is_empty() {
            String::new()
        } else {
            format!("\n\nKey file contents:\n{}", file_contents.join("\n\n"))
        };

        // Step 3: Build a one-shot prompt to generate the summary
        let prompt = format!(
            r#"Please analyze this project and generate a concise project summary (in the same language as any README or docs found). The summary should include:

1. **Project name and purpose** (1-2 sentences)
2. **Tech stack** (language, frameworks, key dependencies)
3. **Directory structure overview** (major modules/components)
4. **Build & run commands** (if discoverable)
5. **Key conventions** (coding style, patterns observed)

Keep it compact (under 30 lines). This summary will be stored and reused across sessions so the AI agent can quickly understand the project without re-reading everything.

Project directory: {}

Directory tree:
{}
{}"#,
            cwd.display(),
            dir_tree,
            files_context
        );

        // Step 4: Send to LLM using a temporary conversation (don't pollute main one)
        let mut summary_conversation = Conversation::new();
        summary_conversation.system_prompt =
            "You are a helpful assistant that generates concise project summaries. \
             Output only the summary content, no extra commentary."
                .to_string();
        summary_conversation.add_message(Message::user(&prompt));

        let response = if self.config.provider == Provider::Anthropic {
            streaming::stream_anthropic_response(
                &self.config,
                &summary_conversation,
                &[], // no tools
                &*self.output,
            )
            .await?
        } else {
            self.client
                .send_message(&summary_conversation, &[])
                .await?
        };

        // Track token usage
        if let Some(ref usage) = response.usage {
            self.total_input_tokens += usage.input_tokens as u64;
            self.total_output_tokens += usage.output_tokens as u64;
        }

        // Extract text
        let summary_text: String = response
            .content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let summary_text = summary_text.trim().to_string();
        if summary_text.is_empty() {
            anyhow::bail!("LLM returned an empty project summary");
        }

        // Step 5: Save to .agent/summary.md
        crate::summary::save(&cwd, &summary_text)?;

        // Step 6: Inject into current session's system prompt
        self.conversation
            .system_prompt
            .push_str(&crate::summary::to_system_prompt_section(&summary_text));

        Ok(summary_text)
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
