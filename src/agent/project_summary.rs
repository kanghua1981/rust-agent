use super::*;

impl Agent {

    /// Scan the project (directory tree + key files) and ask the LLM for a
    /// reusable project summary, persisted to `.agent/summary.md` by the caller.
    ///
    /// Uses the regular tool executor, so the scan is subject to the same
    /// sandbox and path rules as any other tool call.
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
