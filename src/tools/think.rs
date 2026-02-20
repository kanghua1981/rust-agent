//! Think tool: a no-side-effect tool that lets the LLM "pause and think"
//! before taking action. The thought is recorded but nothing is modified.
//!
//! Many agent frameworks (Claude Code, Cursor, etc.) have demonstrated that
//! giving the LLM an explicit thinking tool significantly improves planning
//! quality on complex multi-step tasks.

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;

pub struct ThinkTool;

#[async_trait::async_trait]
impl Tool for ThinkTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "think".to_string(),
            description: r#"Use this tool to think through a problem step-by-step before taking action. This tool has no side effects — it simply records your reasoning. Use it when:
- You need to analyze complex code relationships before editing
- You want to plan a multi-step approach
- You need to evaluate multiple possible solutions
- The task requires careful reasoning about edge cases
Your thought will be recorded but nothing will be modified."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "thought": {
                        "type": "string",
                        "description": "Your detailed reasoning, analysis, or plan"
                    }
                },
                "required": ["thought"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let thought = match input.get("thought").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: thought"),
        };

        // Simply acknowledge the thought — no side effects
        ToolResult::success(format!(
            "Thought recorded ({} chars). Continue with your plan.",
            thought.len()
        ))
    }
}
