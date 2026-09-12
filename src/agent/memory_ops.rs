use super::*;

impl Agent {

    /// Record a tool action to persistent memory.
    pub(super) fn record_tool_to_memory(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        result: &crate::tools::ToolResult,
    ) {
        let path = tool_input.get("path").and_then(|v| v.as_str()).unwrap_or("");

        match tool_name {
            "read_file" => {
                if !path.is_empty() {
                    self.memory.record_event(MemoryEvent::FileRead { path: path.to_string() });
                }
            }
            "write_file" => {
                if !path.is_empty() && !result.is_error {
                    let lines = tool_input
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|c| c.lines().count())
                        .unwrap_or(0);
                    self.memory.record_event(MemoryEvent::FileWritten {
                        path: path.to_string(),
                        lines,
                    });
                }
            }
            "edit_file" => {
                if !path.is_empty() && !result.is_error {
                    self.memory.record_event(MemoryEvent::FileEdited { path: path.to_string() });
                }
            }
            "run_command" => {
                if !result.is_error {
                    let cmd = tool_input
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let short_cmd = crate::ui::truncate_str(cmd, 60).to_string();
                    self.memory.record_event(MemoryEvent::CommandRun { command: short_cmd });
                }
            }
            "grep_search" => {
                let pattern = tool_input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !pattern.is_empty() {
                    self.memory.record_event(MemoryEvent::GrepSearch {
                        pattern: pattern.to_string(),
                        path: if path.is_empty() { None } else { Some(path.to_string()) },
                    });
                }
            }
            "file_search" => {
                let pattern = tool_input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !pattern.is_empty() {
                    self.memory.record_event(MemoryEvent::FileFind { pattern: pattern.to_string() });
                }
            }
            "list_directory" => {
                let dir = if path.is_empty() { "." } else { path };
                self.memory.record_event(MemoryEvent::DirectoryListed { path: dir.to_string() });
            }
            "multi_edit_file" => {
                if !path.is_empty() && !result.is_error {
                    let edits = tool_input
                        .get("edits")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    self.memory.record_event(MemoryEvent::FileMultiEdited {
                        path: path.to_string(),
                        edits,
                    });
                }
            }
            "batch_read_files" => {
                if let Some(paths) = tool_input.get("paths").and_then(|v| v.as_array()) {
                    let collected: Vec<String> = paths
                        .iter()
                        .filter_map(|p| p.as_str().map(|s| s.to_string()))
                        .collect();
                    if !collected.is_empty() {
                        self.memory.record_event(MemoryEvent::BatchFilesRead { paths: collected });
                    }
                }
            }
            "read_pdf" => {
                if !path.is_empty() {
                    self.memory.record_event(MemoryEvent::PdfRead { path: path.to_string() });
                }
            }
            "think" => {
                // No memory update for think — it's internal reasoning
            }
            _ => {}
        }
        // Auto-save is handled inside LocalFileMemory::record_event — no explicit flush needed.
    }

    /// Save memory to disk (public, for use from CLI)
    #[allow(dead_code)]
    pub fn save_memory(&self) {
        if let Err(e) = self.memory.flush() {
            tracing::warn!("Failed to save memory: {}", e);
        }
    }

    /// Generate a plan for the given task without executing it.
    ///
    /// During the planning phase the LLM only has access to **read-only** tools
    /// (read_file, list_directory, grep_search, file_search) so it can explore
    /// the codebase but cannot modify anything.  The resulting plan text is
    /// stored in `self.pending_plan` and returned.
    pub async fn generate_project_summary(&mut self) -> Result<String> {
        let cwd = self.project_dir.clone();

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
        let mut summary_conversation = Conversation::new(&self.project_dir);
        summary_conversation.system_prompt =
            "You are a helpful assistant that generates concise project summaries. \
             Output only the summary content, no extra commentary."
                .to_string();
        summary_conversation.add_message(Message::user(&prompt));

        let response = self.call_llm_as_role("agent", &summary_conversation, &[]).await?;

        // Track token usage
        if let Some(ref usage) = response.usage {
            self.track_tokens("summary", usage);
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
