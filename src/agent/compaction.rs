use super::*;

impl Agent {

    /// Execute a tool with diff preview for file modifications

    /// Track token usage for a specific role.
    pub(super) fn track_tokens(&mut self, role: &str, usage: &crate::llm::Usage) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        let entry = self.role_token_usage.entry(role.to_string()).or_insert((0, 0));
        entry.0 += usage.input_tokens as u64;
        entry.1 += usage.output_tokens as u64;
    }

    /// Check context window and truncate if needed.
    ///
    /// When truncation is required, attempts an LLM-driven summarization
    /// of the removed messages to preserve reasoning context. Falls back
    /// to a mechanical summary if the LLM call fails.
    pub(super) async fn check_and_manage_context(&mut self) {
        let status = self.context_engine.check_context(&self.conversation, &self.config.model);

        if !status.needs_truncation {
            return;
        }

        self.output.on_context_warning(
            status.usage_percent,
            status.estimated_tokens,
            status.max_tokens,
        );

        // Emit hook: context.warning / context.critical
        if let Some(bus) = &self.hook_bus {
            let event_name = if status.usage_percent >= 90.0 {
                "context.critical"
            } else {
                "context.warning"
            };
            let session_id = self.session_id.as_deref().unwrap_or("none").to_string();
            let bus = bus.clone();
            let event = crate::plugin::hook_bus::HookEvent::new(
                event_name,
                session_id,
                serde_json::json!({
                    "used_tokens":  status.estimated_tokens,
                    "max_tokens":   status.max_tokens,
                    "ratio":        status.usage_percent / 100.0,
                    "model":        self.config.model,
                }),
            );
            bus.emit_blocking(event).await;
        }

        // Plan what to truncate (using the pluggable engine)
        let plan = match self.context_engine.plan_truncation(&self.conversation, &self.config.model) {
            Some(plan) => plan,
            None => {
                // Not enough messages to truncate meaningfully — fall back to
                // the legacy truncation which handles small-conversation edge cases.
                context::truncate_conversation(&mut self.conversation, &self.config.model, self.memory.as_ref());
                return;
            }
        };

        // Ask memory providers for pre-compression insights before messages are lost.
        let memory_insights = self.memory.on_pre_compress(
            &self.conversation.messages[plan.remove_start..plan.remove_end],
        );

        // Build condensed context from the messages about to be removed
        let mut truncation_context = context::build_truncation_context(
            &self.conversation.messages[plan.remove_start..plan.remove_end],
        );
        if !memory_insights.is_empty() {
            truncation_context.push_str("\n\n## Memory Provider Insights\n");
            truncation_context.push_str(&memory_insights);
        }

        // Try LLM-driven summarization
        let summary = match self.generate_truncation_summary(&truncation_context).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("LLM summarization failed ({}), using engine's mechanical summary", e);
                self.context_engine.summarize_removed(
                    &self.conversation.messages[plan.remove_start..plan.remove_end],
                )
            }
        };

        self.context_engine.apply_truncation(&mut self.conversation, &plan, &summary, self.memory.as_ref());
    }

    /// Use the LLM to generate a narrative summary of truncated messages.
    ///
    /// This produces a much richer summary than the mechanical fallback,
    /// capturing intent, discoveries, and progress rather than just listing
    /// tool names.
    pub(super) async fn generate_truncation_summary(&mut self, truncation_context: &str) -> Result<String> {
        let prompt = format!(
            r#"The following is a condensed log of a conversation between a user and an AI coding assistant that is about to be removed from context to save space. Generate a concise narrative summary (3-8 sentences) that captures:

1. What the user's goal/task was
2. What actions were taken (files read, edited, commands run)
3. What was discovered or accomplished
4. Any unresolved issues or next steps

Be factual and specific (include file names, function names, error messages). This summary will be injected back into the conversation so the assistant can maintain continuity.

--- CONVERSATION LOG ---
{}
--- END LOG ---

Summary:"#,
            truncation_context
        );

        let mut summary_conv = Conversation::new(&self.project_dir);
        summary_conv.system_prompt =
            "You are a precise technical summarizer. Output only the summary, no extra commentary. \
             Keep it under 200 words.".to_string();
        summary_conv.add_message(Message::user(&prompt));

        // Run silently (using the "summarizer" role model if configured) so the
        // background compaction does not interleave with the interactive transcript.
        let cfg = self.role_configs.get("summarizer").unwrap_or(&self.config);
        let response = self.call_llm_silent(cfg, &summary_conv, &[]).await?;

        if let Some(ref usage) = response.usage {
            self.track_tokens("summarizer", usage);
        }

        let text: String = response
            .content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let text = text.trim().to_string();
        if text.is_empty() {
            anyhow::bail!("LLM returned empty summary");
        }

        Ok(text)
    }
}
