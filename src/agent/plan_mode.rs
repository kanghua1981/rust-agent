//! Plan-mode session state: the agent analyzes read-only, then submits a plan
//! for approval before implementing. This is the dsh dsh-plan-mode shape
//! expressed here: plan mode is a session flag that restricts the model to a
//! read-only toolset plus the exit_plan_mode transport.

use crate::tools::ToolDefinition;

/// The exit_plan_mode tool: the model submits the full plan, which is presented
/// for approval; approval switches the agent out of plan mode.
pub fn exit_plan_mode_definition() -> ToolDefinition {
    ToolDefinition {
        name: "exit_plan_mode".to_string(),
        description: "Submit the complete implementation plan for user review and approval.             Call this ONLY when the plan is decision-complete. It is the only and final tool call             in that assistant response. Implementation begins only after the user approves the             plan. If it is rejected, incorporate the feedback and resubmit.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "The complete plan markdown, starting with a # title."
                }
            },
            "required": ["plan"]
        }),
    }
}

/// The read-only toolset plus the exit-plan transport, used while plan mode is on.
pub fn plan_mode_toolset(readonly: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    let mut defs = readonly;
    defs.push(exit_plan_mode_definition());
    defs
}

/// Guidance injected (prepended to the user turn) while plan mode is active.
pub fn plan_mode_guidance() -> &'static str {
    "You are in PLAN MODE. Analyze and plan only — do NOT modify files, run mutating commands,      or take irreversible actions. Use the read-only tools to explore the repository and ground      the plan in facts. When the plan is complete and decision-complete, call exit_plan_mode with      the full plan markdown (starting with a # title) as the ONLY and final tool call in that      response. You cannot leave plan mode any other way. A user's conversational agreement does      not end plan mode — only an approved exit_plan_mode does. If the plan is rejected,      incorporate the feedback and resubmit."
}
