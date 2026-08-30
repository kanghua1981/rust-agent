//! Compositional orchestration helpers: plan mode (read-only analysis + an
//! approved plan before implementation) and in-process sub-agent delegation.
//!
//! This replaces the retired pipeline DAG: order is decided by the model (or by
//! entering plan mode), not by a fixed stage machine.

use crate::tools::ToolDefinition;

/// The exit_plan_mode tool: the model submits the full plan, which is presented
/// for approval; approval switches the agent out of plan mode.
pub fn exit_plan_mode_definition() -> ToolDefinition {
    ToolDefinition {
        name: "exit_plan_mode".to_string(),
        description: "Submit the complete implementation plan for user review and approval. \
            Call this ONLY when the plan is decision-complete. It is the only and final tool call \
            in that assistant response. Implementation begins only after the user approves the \
            plan. If it is rejected, incorporate the feedback and resubmit.".to_string(),
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

/// The subagent tool: delegate a focused task to a fresh in-process child agent.
pub fn subagent_definition() -> ToolDefinition {
    ToolDefinition {
        name: "subagent".to_string(),
        description: "Delegate a focused, self-contained task to a fresh in-process sub-agent \
            (a new session with the same model/directory/sandbox). The sub-agent reads its own \
            conversation from scratch — give it everything it needs. Use it for independent \
            verifications, parallel sub-tasks, or offloading a self-contained job; it cannot see \
            this conversation's history.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "The self-contained task for the sub-agent." }
            },
            "required": ["task"]
        }),
    }
}

/// The subagent_fork tool: delegate with the current conversation's history (a
/// fork), so the child sees what has happened so far — dsh's fork semantic.
pub fn subagent_fork_definition() -> ToolDefinition {
    ToolDefinition {
        name: "subagent_fork".to_string(),
        description: "Delegate a task to an in-process sub-agent whose conversation is forked \
            from this session, so it inherits the current history and context. Use it when the \
            sub-task depends on what was already discussed or done.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "The task for the forked sub-agent." }
            },
            "required": ["task"]
        }),
    }
}

/// Guidance injected (prepended to the user turn) while plan mode is active.
pub fn plan_mode_guidance() -> &'static str {
    "You are in PLAN MODE. Analyze and plan only — do NOT modify files, run mutating commands, \
     or take irreversible actions. Use the read-only tools to explore the repository and ground \
     the plan in facts. When the plan is complete and decision-complete, call exit_plan_mode with \
     the full plan markdown (starting with a # title) as the ONLY and final tool call in that \
     response. You cannot leave plan mode any other way. A user's conversational agreement does \
     not end plan mode — only an approved exit_plan_mode does. If the plan is rejected, \
     incorporate the feedback and resubmit."
}
