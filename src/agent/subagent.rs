use super::*;

impl Agent {

    /// Run an in-process sub-agent and return a [crate::tools::ToolResult].
    ///
    /// If output_schema is given, the sub-agent is instructed to return a JSON
    /// value conforming to that schema, and the result carries that JSON
    /// (pretty-printed), or a structured error if the output does not parse.
    /// Without a schema, it returns the sub-agent's final free-text output.
    pub(crate) async fn run_subagent(
        &self,
        task: &str,
        output_schema: Option<&serde_json::Value>,
        fork: bool,
    ) -> crate::tools::ToolResult {
        let effective_task = match output_schema {
            Some(schema) => format!(
                "{}\n\n[STRUCTURED OUTPUT]\nReturn your final answer as a single JSON value conforming to this JSON Schema. Do NOT wrap it in markdown code fences and do NOT add any other prose. Return ONLY the JSON value as your final message.\n\n{}\n",
                task, schema
            ),
            None => task.to_string(),
        };
        match self.spawn_subagent_session(&effective_task, fork).await {
            Ok((id, text)) => {
                if output_schema.is_some() {
                    match Self::extract_json_value(&text) {
                        Some(json) => crate::tools::ToolResult::success(
                            serde_json::json!({ "subagentId": id, "result": json }).to_string(),
                        ),
                        None => crate::tools::ToolResult::error(format!(
                            "Sub-agent did not return a JSON value matching the requested schema.\n\nRaw output:\n{}",
                            text
                        )),
                    }
                } else {
                    crate::tools::ToolResult::success(
                        serde_json::json!({ "subagentId": id, "result": text }).to_string(),
                    )
                }
            }
            Err(e) => crate::tools::ToolResult::error(format!("Sub-agent failed: {:#}", e)),
        }
    }

    /// Spawn a continuable in-process sub-agent and run task, returning a
    /// (subagent_id, final_text) pair. The child is registered in this agent's
    /// live sub-agent registry so a later subagent_followup can continue it.
    /// Honors the delegation-depth bound (max_subagent_depth).
    pub(crate) async fn spawn_subagent_session(
        &self,
        task: &str,
        fork: bool,
    ) -> Result<(String, String)> {
        if self.delegation_depth >= self.max_subagent_depth {
            anyhow::bail!(
                "sub-agent depth limit reached ({}) — refusing to spawn deeper",
                self.max_subagent_depth
            );
        }
        {
            let reg = self.subagents.lock().await;
            if reg.len() >= self.max_subagents {
                anyhow::bail!(
                    "sub-agent quota reached ({}) — terminate one via subagent_terminate",
                    self.max_subagents
                );
            }
        }
        let mut child = Agent::new(
            self.config.clone(),
            self.project_dir.clone(),
            self.output.clone(),
            self.sandbox.clone(),
            self.plugin_manager.clone(),
        );
        child.delegation_depth = self.delegation_depth + 1;
        child.max_subagent_depth = self.max_subagent_depth;
        child.max_subagents = self.max_subagents;
        if fork {
            child.conversation = self.conversation.fork(self.conversation.log.len() as u64);
        }
        child.conversation.delegation_depth = child.delegation_depth;
        let id = format!("sa-{}", uuid::Uuid::new_v4());
        let child_arc = Arc::new(tokio::sync::Mutex::new(child));
        self.subagents.lock().await.insert(id.clone(), child_arc.clone());
        let text = {
            let mut guard = child_arc.lock().await;
            // Box the recursive call: process_message -> run_tool_loop -> spawn.
            Box::pin(guard.process_message(task)).await?
        };
        Ok((id, text))
    }

    /// Send a follow-up to a live sub-agent (by id) and return its next result.
    /// The sub-agent keeps its conversation, so this is multi-turn delegation.
    pub(crate) async fn subagent_followup(
        &self,
        id: &str,
        message: &str,
        output_schema: Option<&serde_json::Value>,
    ) -> crate::tools::ToolResult {
        let child = self.subagents.lock().await.get(id).cloned();
        match child {
            Some(child_arc) => {
                let result = {
                    let mut guard = child_arc.lock().await;
                    Box::pin(guard.process_message(message)).await
                };
                match result {
                    Ok(text) => {
                        if output_schema.is_some() {
                            match Self::extract_json_value(&text) {
                                Some(json) => crate::tools::ToolResult::success(
                                    serde_json::json!({ "subagentId": id, "result": json }).to_string(),
                                ),
                                None => crate::tools::ToolResult::error(format!(
                                    "Sub-agent did not return JSON matching the schema.\n\nRaw output:\n{}",
                                    text
                                )),
                            }
                        } else {
                            crate::tools::ToolResult::success(
                                serde_json::json!({ "subagentId": id, "result": text }).to_string(),
                            )
                        }
                    }
                    Err(e) => crate::tools::ToolResult::error(format!("Sub-agent follow-up failed: {:#}", e)),
                }
            }
            None => crate::tools::ToolResult::error(format!("unknown sub-agent id: {}", id)),
        }
    }
    /// Remove a live sub-agent from the registry, freeing its quota slot.
    pub(crate) async fn terminate_subagent(&self, id: &str) -> bool {
        self.subagents.lock().await.remove(id).is_some()
    }

    /// Best-effort extraction of a JSON value from sub-agent output. Try a direct
    /// parse of the trimmed text, then fall back to the first balanced {..} or [..]
    /// block (covers a model that wraps the JSON in prose).
    pub(super) fn extract_json_value(text: &str) -> Option<serde_json::Value> {
        let t = text.trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
            return Some(v);
        }
        for open in ['{', '['] {
            if let Some(start) = t.find(open) {
                let close = if open == '{' { '}' } else { ']' };
                if let Some(end_rel) = t[start..].rfind(close) {
                    let end = start + end_rel + 1;
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t[start..end]) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }
}
