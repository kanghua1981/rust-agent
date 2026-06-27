//! Unified tool loop: powers `process_message` (BasicLoop),
//! `run_pipeline_stage` (Executor / Checker), and `generate_plan` (Planner).
//!
//! Each caller selects the subset of features via `ToolLoopOptions` while
//! the core LLM→tools→results→context cycle is shared.

use std::collections::HashMap;

use crate::confirm;
use crate::context;
use crate::conversation::{ContentBlock, Conversation, ImageSource, Message, Role};
use crate::output::AgentOutput;

// ═══════════════════════════════════════════════════════════════════════════
//  ToolLoopOptions
// ═══════════════════════════════════════════════════════════════════════════

/// Feature flags controlling the unified tool loop.
///
/// The same `run_tool_loop()` powers `process_message` (BasicLoop),
/// `run_pipeline_stage` (Executor / Checker), and `generate_plan` (Planner).
/// Each caller picks the subset of features it needs.
#[derive(Clone)]
pub struct ToolLoopOptions {
    /// Role name for LLM calls and token tracking ("agent", "planner", "executor", "checker").
    pub role: String,
    /// Enable loop-detection guardrails (tool-hash + mutation tracking).
    pub enable_guardrails: bool,
    /// Check for Ctrl-\ guidance injection between iterations.
    pub enable_guidance: bool,
    /// Emit `on_file_created` notifications after write/edit/multi_edit (only meaningful
    /// for the main conversation).
    pub notify_file_created: bool,
    /// Handle `upload_image` tool results by injecting base64 image blocks into the
    /// conversation.
    pub handle_upload_image: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
//  Agent methods: unified tool loop + guardrails
// ═══════════════════════════════════════════════════════════════════════════

use super::Agent;

impl Agent {
    /// Single unified tool loop.
    ///
    /// Sends `conversation` to the LLM with the given `tools`, executes any
    /// requested tool calls, feeds results back, and repeats until the model
    /// produces a final text response (or hits iteration / interrupt limits).
    pub(crate) async fn run_tool_loop(
        &mut self,
        conversation: &mut Conversation,
        tools: &[crate::tools::ToolDefinition],
        opts: &ToolLoopOptions,
    ) -> anyhow::Result<String> {
        let max_iterations = self.config.max_tool_iterations;
        let mut final_text = String::new();
        let mut iterations = 0;

        loop {
            // ── 1. Interrupt check ────────────────────────────────────────
            if self.is_interrupted() {
                self.output.on_warning("Interrupted by user.");
                break;
            }

            // ── 2. Guardrails (optional) ─────────────────────────────────
            if opts.enable_guardrails {
                self.check_loop_guardrails(iterations);
            }

            // ── 3. Service event drain ───────────────────────────────────
            self.drain_service_events();

            // ── 4. Iteration guard ───────────────────────────────────────
            iterations += 1;
            if iterations > max_iterations {
                self.output.on_warning(&format!(
                    "Reached maximum tool iterations ({}). Stopping.",
                    max_iterations
                ));
                break;
            }

            // ── 5. Guidance injection (optional, Ctrl-\) ─────────────────
            if opts.enable_guidance && super::interrupt::is_guidance_requested() {
                super::interrupt::clear_guidance();
                if let Some(text) = self.output.inject_guidance() {
                    conversation.system_prompt.push_str(&format!(
                        "\n\n[⚡ USER GUIDANCE]: {}",
                        text
                    ));
                    self.output
                        .on_warning("💡 Guidance injected into executor context.");
                }
            }

            // ── 6. Tool pair integrity ───────────────────────────────────
            context::ensure_tool_pair_integrity(&mut conversation.messages);

            // ── 7. LLM call ──────────────────────────────────────────────
            let response =
                self.call_llm_as_role(&opts.role, conversation, tools).await?;

            // ── 8. Token tracking ────────────────────────────────────────
            if let Some(ref usage) = response.usage {
                self.track_tokens(&opts.role, usage);
            }

            // ── 9. Post-streaming interrupt ──────────────────────────────
            if self.is_interrupted() {
                let partial: Vec<ContentBlock> = response
                    .content
                    .into_iter()
                    .filter(|b| matches!(b, ContentBlock::Text { text } if !text.is_empty()))
                    .collect();
                if !partial.is_empty() {
                    conversation.add_message(Message::assistant(partial));
                }
                break;
            }

            // ── 10. Collect text & detect tool use ───────────────────────
            let has_tool_use = response
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolUse { .. }));

            for block in &response.content {
                if let ContentBlock::Text { text } = block {
                    if !text.is_empty() {
                        final_text = text.clone();
                        let role_cfg = self
                            .role_configs
                            .get(&opts.role)
                            .unwrap_or(&self.config);
                        if role_cfg.provider != crate::config::Provider::Anthropic {
                            self.output.on_assistant_text(text);
                        }
                    }
                }
            }

            // ── 11. Add assistant message ────────────────────────────────
            conversation.add_message(Message::assistant(response.content.clone()));

            if !has_tool_use {
                break;
            }

            // ── 12. Extract tool uses ────────────────────────────────────
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

            // ── call_node parallel pre-execution ─────────────────────────
            let parallel_call_nodes: Vec<_> = tool_uses
                .iter()
                .filter(|(_, name, _)| name == "call_node")
                .collect();
            let mut call_node_cache: HashMap<String, crate::tools::ToolResult> =
                HashMap::new();
            if parallel_call_nodes.len() > 1 {
                self.output.on_warning(&format!(
                    "[call_node] Running {} nodes in parallel…",
                    parallel_call_nodes.len()
                ));
                let futs = parallel_call_nodes.iter().map(|(id, name, input)| {
                    let id = id.clone();
                    let exec = self.tool_executor.execute(name, input);
                    async move { (id, exec.await) }
                });
                let paired = futures::future::join_all(futs).await;
                for (id, result) in paired {
                    call_node_cache.insert(id, result);
                }
            }

            // ── 13. Execute each tool ────────────────────────────────────
            for (tool_id, tool_name, tool_input) in tool_uses {
                self.output
                    .on_tool_use(&tool_name, &tool_input, &tool_id);

                // Virtual tool: ask_user
                if tool_name == "ask_user" {
                    let question = tool_input
                        .get("question")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Could you clarify?");
                    let answer = self.output.ask_user(question);
                    conversation.add_message(Message::tool_result(
                        &tool_id,
                        &format!("User's answer: {}", answer),
                        false,
                    ));
                    continue;
                }

                // Confirmation for dangerous tools
                let confirm_level =
                    super::confirmation::needs_confirmation(&tool_name, &tool_input);
                if confirm_level != super::confirmation::ConfirmationLevel::None {
                    if confirm_level == super::confirmation::ConfirmationLevel::HighRisk {
                        self.output.on_warning(&format!(
                            "⚠️ HIGH RISK: '{}' targets a protected path or dangerous command pattern.",
                            tool_name
                        ));
                    }
                    let hook_decision =
                        self.check_confirm_via_hook(&tool_name, &tool_input).await;
                    let approved = match hook_decision {
                        Some(decision) => decision,
                        None => {
                            let action =
                                super::confirmation::build_confirm_action(&tool_name, &tool_input);
                            let mut ui_approved = false;
                            loop {
                                let result = self.output.confirm(&action);
                                match result {
                                    confirm::ConfirmResult::Yes
                                    | confirm::ConfirmResult::AlwaysYes => {
                                        ui_approved = true;
                                        break;
                                    }
                                    confirm::ConfirmResult::No => break,
                                    confirm::ConfirmResult::Clarify(question) => {
                                        let explanation = self
                                            .explain_tool_action(
                                                &tool_name,
                                                &tool_input,
                                                &question,
                                            )
                                            .await;
                                        self.output.on_assistant_text(&explanation);
                                    }
                                }
                            }
                            ui_approved
                        }
                    };
                    if !approved {
                        conversation.add_message(Message::tool_result(
                            &tool_id,
                            "User declined to execute this operation.",
                            true,
                        ));
                        continue;
                    }
                }

                // Execute (with cached call_node result or diff preview)
                let result = if let Some(cached) = call_node_cache.remove(&tool_id) {
                    cached
                } else if matches!(
                    tool_name.as_str(),
                    "edit_file" | "multi_edit_file" | "write_file"
                ) {
                    self.execute_with_diff(&tool_name, &tool_input).await
                } else {
                    self.tool_executor.execute(&tool_name, &tool_input).await
                };

                self.output.on_tool_result(&tool_name, &result);

                // file_created notification (optional)
                if opts.notify_file_created
                    && !result.is_error
                    && matches!(
                        tool_name.as_str(),
                        "write_file" | "edit_file" | "multi_edit_file"
                    )
                {
                    if let Some(path_str) =
                        tool_input.get("path").and_then(|v| v.as_str())
                    {
                        let abs_path = self.project_dir.join(path_str);
                        self.output
                            .on_file_created(&abs_path.to_string_lossy());
                    }
                }

                // Guardrail tracking (optional)
                if opts.enable_guardrails {
                    let args_hash =
                        Self::hash_tool_args(&tool_name, &tool_input);
                    let entry = self
                        .turn_tool_call_hashes
                        .entry((tool_name.clone(), args_hash))
                        .or_insert(0);
                    *entry += 1;

                    let is_mutation = !result.is_error
                        && matches!(
                            tool_name.as_str(),
                            "write_file" | "edit_file" | "multi_edit_file"
                        );
                    if is_mutation {
                        self.turns_without_mutation = 0;
                    }
                }

                // Persistent memory recording
                self.record_tool_to_memory(&tool_name, &tool_input, &result);

                // upload_image handling (optional)
                if opts.handle_upload_image
                    && tool_name == "upload_image"
                    && !result.is_error
                {
                    if let Some(path_value) = tool_input.get("path") {
                        if let Some(path_str) = path_value.as_str() {
                            let path = if path_str.starts_with('/') {
                                std::path::Path::new(path_str).to_path_buf()
                            } else {
                                self.project_dir.join(path_str)
                            };
                            if let Ok((mime_type, base64_data)) =
                                self.read_image_to_base64(&path)
                            {
                                let image_block = ContentBlock::Image {
                                    source: ImageSource::Base64 {
                                        media_type: mime_type,
                                        data: base64_data,
                                    },
                                    mime_type: None,
                                };
                                let image_message = Message {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    role: Role::User,
                                    content: vec![image_block],
                                };
                                conversation.add_message(image_message);
                            }
                        }
                    }
                }

                // Add tool result to conversation
                conversation.add_message(Message::tool_result(
                    &tool_id,
                    &result.output,
                    result.is_error,
                ));
            }

            // ── 14. Per-iteration loop-detection update (optional) ────────
            if opts.enable_guardrails {
                let current_hashes =
                    std::mem::take(&mut self.turn_tool_call_hashes);
                let is_identical_to_prev = !current_hashes.is_empty()
                    && current_hashes == self.prev_turn_hashes;
                if is_identical_to_prev {
                    self.identical_tool_turns += 1;
                } else if !current_hashes.is_empty() {
                    self.identical_tool_turns = 0;
                }
                self.prev_turn_hashes = current_hashes;

                self.turns_without_mutation += 1;
            }

            // ── 15. Context management ───────────────────────────────────
            let status =
                context::check_context(conversation, &self.config.model);
            if status.needs_truncation {
                context::truncate_conversation(
                    conversation,
                    &self.config.model,
                    self.memory.as_ref(),
                );
            }

            // ── 16. Stop reason ──────────────────────────────────────────
            if let Some(ref reason) = response.stop_reason {
                if reason == "end_turn" && !has_tool_use {
                    break;
                }
            }
        }

        Ok(final_text)
    }

    // ── Loop detection / guardrail helpers ──────────────────────────────

    /// Compute a stable hash of (tool_name, serialized_args) for loop detection.
    pub(crate) fn hash_tool_args(tool_name: &str, input: &serde_json::Value) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tool_name.hash(&mut hasher);
        // Use canonical JSON to ensure deterministic hashing
        input.to_string().hash(&mut hasher);
        hasher.finish()
    }

    /// Check loop guardrails and inject warnings if the agent appears stuck.
    ///
    /// Three detection layers:
    /// - Same tool+args called 3+ times within a single turn → warn
    /// - 3+ consecutive turns with identical tool call patterns → warn (cross-turn loop)
    /// - 5+ iterations without any file mutation → warn
    pub(crate) fn check_loop_guardrails(&mut self, iteration: usize) {
        // ── Layer 1: repeated identical tool calls within this turn ─────
        for ((ref name, _), count) in &self.turn_tool_call_hashes {
            if *count >= 3 {
                self.output.on_warning(&format!(
                    "⚠️ Loop detected: '{}' has been called {} times with identical arguments. \
                     You may be stuck. Consider trying a different approach.",
                    name, count
                ));
                // Reset to avoid spamming
                break;
            }
        }

        // ── Layer 2: identical tool call pattern across consecutive turns ──
        if self.identical_tool_turns >= 3 {
            self.output.on_warning(&format!(
                "⚠️ Cross-turn loop detected: identical tool calls repeated for {} consecutive turns. \
                 The agent appears to be stuck. Trying a completely different approach is strongly recommended.",
                self.identical_tool_turns
            ));
            // Reset so we only warn once per detected loop
            self.identical_tool_turns = 0;
        }

        // ── Layer 3: extended period without file mutations ─────────────
        if self.turns_without_mutation >= 5 && iteration > 5 {
            self.output.on_warning(&format!(
                "⚠️ No file changes in the last {} iterations. \
                 If you're stuck in analysis, try taking action: write code, edit files, or run commands.",
                self.turns_without_mutation
            ));
            // Reset counter so we don't spam every iteration
            self.turns_without_mutation = 0;
        }
    }
}
