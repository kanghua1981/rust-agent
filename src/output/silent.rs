use super::*;

// ── SilentOutput ─────────────────────────────────────────────────────────────

/// No-op output used for internal LLM calls that must not produce user-visible
/// output (router classification, background knowledge extraction, etc.).
pub struct SilentOutput;

impl AgentOutput for SilentOutput {
    fn on_thinking(&self) {}
    fn on_role_header(&self, _label: &str, _model: &str) {}
    fn on_stage_end(&self, _label: &str) {}
    fn on_assistant_text(&self, _text: &str) {}
    fn on_streaming_text(&self, _token: &str) {}
    fn on_stream_start(&self) {}
    fn on_stream_end(&self) {}
    fn on_tool_use(&self, _name: &str, _input: &serde_json::Value, _tool_id: &str) {}
    fn on_tool_result(&self, _name: &str, _result: &ToolResult) {}
    fn on_diff(&self, _path: &str, _old: &str, _new: &str) {}
    fn confirm(&self, _action: &ConfirmAction) -> crate::confirm::ConfirmResult {
        crate::confirm::ConfirmResult::Yes
    }
    fn ask_user(&self, _question: &str) -> String { String::new() }
    fn review_plan(&self, _plan_text: &str) -> PlanReview { PlanReview::Approve }
    fn inject_guidance(&self) -> Option<String> { None }
    fn on_warning(&self, _msg: &str) {}
    fn on_error(&self, _msg: &str) {}
    fn on_context_warning(&self, _usage_percent: f32, _estimated: usize, _max: usize) {}
}
