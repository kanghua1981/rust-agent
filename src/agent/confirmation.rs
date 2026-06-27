//! Tool confirmation system: decides whether a tool action needs user
//! approval, builds confirm prompts, and handles hook-driven auto-approval.
//!
//! Also contains the virtual `ask_user` tool definition and the
//! `execute_with_diff` helper that snapshots files before mutation.

use std::path::Path;

use anyhow::Context;

use crate::confirm::ConfirmAction;
use crate::conversation::{ContentBlock, Conversation, ImageSource, Message, Role};
use crate::output::AgentOutput;

// ═══════════════════════════════════════════════════════════════════════════
//  Confirmation level
// ═══════════════════════════════════════════════════════════════════════════

/// Confirmation level for a tool action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationLevel {
    /// Normal confirmation (can be auto-approved with --yes)
    Normal,
    /// High-risk operation — warning is shown even in auto-approve mode,
    /// and the user must explicitly confirm unless --yes is passed.
    HighRisk,
    /// No confirmation needed (read-only operations)
    None,
}

/// Check if a tool action needs user confirmation, and at what level.
pub fn needs_confirmation(tool_name: &str, input: &serde_json::Value) -> ConfirmationLevel {
    match tool_name {
        "write_file" | "edit_file" | "multi_edit_file" => {
            // Check if the path is sensitive
            if let Some(path_str) = input.get("path").and_then(|v| v.as_str()) {
                let path = std::path::Path::new(path_str);
                let home = std::env::var("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
                if crate::security::check_write_safety(path, &home).is_err() {
                    return ConfirmationLevel::HighRisk;
                }
            }
            ConfirmationLevel::Normal
        }
        "run_command" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                if crate::security::is_high_risk_command(cmd) {
                    return ConfirmationLevel::HighRisk;
                }
            }
            ConfirmationLevel::Normal
        }
        _ => ConfirmationLevel::None,
    }
}

/// Build a ConfirmAction from tool name and input.
pub fn build_confirm_action(tool_name: &str, input: &serde_json::Value) -> ConfirmAction {
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
        "edit_file" | "multi_edit_file" => {
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

// ═══════════════════════════════════════════════════════════════════════════
//  ask_user virtual tool
// ═══════════════════════════════════════════════════════════════════════════

/// Build the tool definition for the virtual `ask_user` tool.
///
/// This tool is NOT registered in `ToolExecutor`; it's intercepted in the
/// agent loop and handled via `AgentOutput::ask_user()`.
pub fn ask_user_definition() -> crate::tools::ToolDefinition {
    crate::tools::ToolDefinition {
        name: "ask_user".to_string(),
        description: "Ask the user a clarifying question when you need more information \
            to complete the task. Use this when the request is ambiguous, missing \
            key details, or when you need the user to choose between alternatives. \
            The user's answer will be returned as the tool result."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user. Be specific and concise."
                }
            },
            "required": ["question"]
        }),
    }
}

/// Append the `ask_user` tool to a list of tool definitions.
pub fn with_ask_user(mut defs: Vec<crate::tools::ToolDefinition>) -> Vec<crate::tools::ToolDefinition> {
    defs.push(ask_user_definition());
    defs
}

// ═══════════════════════════════════════════════════════════════════════════
//  Agent methods
// ═══════════════════════════════════════════════════════════════════════════

use super::Agent;

impl Agent {
    /// Generate a brief explanation of what a tool is about to do and answer
    /// the user's question. Uses a lightweight LLM call (no tools).
    pub(crate) async fn explain_tool_action(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        user_question: &str,
    ) -> String {
        let prompt = format!(
            "The AI assistant is about to execute the following tool action:\n\
             Tool: {}\n\
             Parameters: {}\n\n\
             The user asked: {}\n\n\
             Please briefly explain what this tool action will do and answer the user's question. \
             Keep your answer concise (2-4 sentences). Answer in the same language as the user's question.",
            tool_name,
            serde_json::to_string_pretty(tool_input).unwrap_or_default(),
            user_question
        );

        let mut explain_conv = Conversation::new(&self.project_dir);
        explain_conv.system_prompt =
            "You are a helpful assistant explaining tool actions to the user. Be concise."
                .to_string();
        explain_conv.add_message(Message::user(&prompt));

        match self.call_llm_as_role("agent", &explain_conv, &[]).await {
            Ok(response) => {
                response
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
                    .join("")
            }
            Err(e) => format!("(Failed to generate explanation: {})", e),
        }
    }

    /// Query the hook system to see if a confirmation should be auto-approved,
    /// auto-denied, or left to the normal UI flow.
    ///
    /// Returns:
    ///   `Some(true)`  — hook approved; skip UI prompt
    ///   `Some(false)` — hook denied;   skip UI prompt
    ///   `None`        — no hook verdict; fall through to normal UI
    pub(crate) async fn check_confirm_via_hook(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> Option<bool> {
        let bus = self.hook_bus.as_ref()?;

        let session_id = self.session_id.clone().unwrap_or_else(|| "none".to_string());

        let action_type = match tool_name {
            "write_file"                    => "write_file",
            "edit_file" | "multi_edit_file" => "edit_file",
            "run_command"                   => "run_command",
            "delete_file"                   => "delete_file",
            _                               => tool_name,
        };

        let path    = tool_input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let command = tool_input.get("command").and_then(|v| v.as_str()).unwrap_or("");

        let event = crate::plugin::hook_bus::HookEvent::new(
            "confirm.before",
            session_id,
            serde_json::json!({
                "tool_name":   tool_name,
                "action_type": action_type,
                "path":        path,
                "command":     command,
            }),
        );

        match bus.emit_intercepting(event).await {
            crate::plugin::hook_bus::HookResult::Approved { message } => {
                if let Some(msg) = &message {
                    self.output.on_warning(&format!("[hook:confirm] ✅ {}", msg));
                }
                Some(true)
            }
            crate::plugin::hook_bus::HookResult::Cancel { reason } => {
                self.output.on_warning(&format!("[hook:confirm] 🚫 blocked: {}", reason));
                Some(false)
            }
            _ => None, // Continue → fall through to UI
        }
    }

    /// Execute a tool with diff preview for file modifications.
    ///
    /// Snapshots the target file before mutation, then runs the tool.
    /// On success, computes and emits a diff between old and new content.
    pub(crate) async fn execute_with_diff(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> crate::tools::ToolResult {
        let path = tool_input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Use path manager for path resolution
        let resolved = self.path_manager.resolve(path);
        let resolved_str = resolved.display().to_string();

        // Sandbox: snapshot the file before modification
        self.sandbox.before_write(&resolved).await;

        // Read old content if file exists
        let old_content: Option<String> = tokio::fs::read_to_string(&resolved_str).await.ok();

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

    /// Read an image file and convert it to base64 format.
    pub(crate) fn read_image_to_base64(&self, path: &Path) -> anyhow::Result<(String, String)> {
        use base64::{engine::general_purpose::STANDARD, Engine};

        // Read file
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read image file: {}", path.display()))?;

        // Detect MIME type from file extension
        let mime_type = match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) => match ext.to_lowercase().as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                "tiff" | "tif" => "image/tiff",
                _ => anyhow::bail!("Unsupported image format: {}", ext),
            },
            None => anyhow::bail!("File has no extension: {}", path.display()),
        };

        // Encode to base64
        let base64_data = STANDARD.encode(&data);

        Ok((mime_type.to_string(), base64_data))
    }
}
