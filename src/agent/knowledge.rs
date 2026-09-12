use super::*;

impl Agent {

    // ── Knowledge extraction ──────────────────────────────────────────────────

    /// Extract project facts from the recent conversation and store them in memory.
    ///
    /// Called automatically every 5 turns inside `process_message`. Uses a
    /// `SilentOutput` so it produces no visible output.
    pub(super) async fn extract_and_store_knowledge(&self) {
        // Collect the last 10 messages as source material
        let msgs = &self.conversation.messages;
        let window: Vec<_> = msgs.iter().rev().take(10).collect();
        if window.is_empty() {
            return;
        }

        let mut context = String::new();
        for msg in window.into_iter().rev() {
            let role = match msg.role {
                Role::User      => "User",
                Role::Assistant => "Assistant",
                Role::System    => continue,
            };
            context.push_str(&format!("{}: {}\n", role, msg.text_content()));
        }

        let prompt = format!(
            "From the conversation below, extract 1-3 concise project facts worth \
            remembering across sessions. Only facts that are durable (architecture, \
            file locations, key decisions, recurring patterns). \
            Return ONLY a bullet list with no preamble: one fact per line starting with `-`.\n\n{}",
            context
        );

        let mut conv = Conversation::new(&self.project_dir);
        conv.system_prompt = "You are a knowledge extractor. Be concise.".to_string();
        conv.add_message(Message::user(&prompt));

        let cfg = self.role_configs.get("summarizer").unwrap_or(&self.config);
        if let Ok(response) = self.call_llm_silent(cfg, &conv, &[]).await {
            let text: String = response.content.iter()
                .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
                .collect();
            let facts: Vec<String> = text.lines()
                .map(|l| l.trim().trim_start_matches('-').trim().to_string())
                .filter(|l| l.len() > 10)
                .take(3)
                .collect();
            if !facts.is_empty() {
                self.memory.record_event(crate::memory::MemoryEvent::KnowledgeExtracted { facts });
            }
        }
    }

    /// Manually consolidate memory: read recent session log and all existing knowledge,
    /// ask LLM to distill into improved knowledge entries and compress the log.
    ///
    /// Called by the `/consolidate` CLI command.
    pub async fn consolidate_memory(&self) -> anyhow::Result<usize> {
        let existing_knowledge = self.memory.knowledge();
        let session_log = self.memory.session_log();

        if session_log.is_empty() && existing_knowledge.is_empty() {
            return Ok(0);
        }

        let mut prompt = String::from(
            "You are distilling an AI agent's memory. \
            Given the session log and existing knowledge below, \
            produce an improved, deduplicated list of up to 10 project knowledge facts.\n\
            Rules: each fact must be a single sentence, durable across sessions, \
            no timestamps, no trivial observations.\n\
            Return ONLY the bullet list starting each line with `-`.\n\n"
        );

        if !existing_knowledge.is_empty() {
            prompt.push_str("## Existing Knowledge\n");
            for k in &existing_knowledge {
                prompt.push_str(&format!("- {}\n", k));
            }
        }
        if !session_log.is_empty() {
            prompt.push_str("\n## Session Log (most recent activity)\n");
            for entry in session_log.iter().rev().take(30) {
                prompt.push_str(&format!("- {}\n", entry));
            }
        }

        let mut conv = Conversation::new(&self.project_dir);
        conv.system_prompt = "You are a memory consolidation assistant.".to_string();
        conv.add_message(Message::user(&prompt));

        let cfg = self.role_configs.get("summarizer").unwrap_or(&self.config);
        let response = self.call_llm_silent(cfg, &conv, &[]).await?;

        let text: String = response.content.iter()
            .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
            .collect();

        let facts: Vec<String> = text.lines()
            .map(|l| l.trim().trim_start_matches('-').trim().to_string())
            .filter(|l| l.len() > 10)
            .take(10)
            .collect();

        let count = facts.len();
        if count > 0 {
            self.memory.record_event(crate::memory::MemoryEvent::KnowledgeExtracted { facts });
        }
        Ok(count)
    }
}
