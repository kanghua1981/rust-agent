//! Data models for the global database.
//!
//! All models support Serialize/Deserialize for WebSocket JSON transport.

use serde::{Deserialize, Serialize};

// ── Preset ──────────────────────────────────────────────────────────────

/// A saved connection preset (replaces localStorage presets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub server_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub auto_approve: bool,
    pub agent_mode: String,
    pub isolation: String,
    #[serde(default)]
    pub new_session: bool,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sort_order: i32,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_icon() -> String { "🔧".into() }

impl Default for Preset {
    fn default() -> Self {
        Self {
            id:           uuid::Uuid::new_v4().to_string(),
            name:         String::new(),
            server_url:   String::new(),
            workdir:      None,
            model:        None,
            auto_approve: false,
            agent_mode:   "auto".into(),
            isolation:    "container".into(),
            new_session:  false,
            icon:         default_icon(),
            color:        None,
            tags:         vec![],
            sort_order:   0,
            created_at:   String::new(),
            updated_at:   String::new(),
        }
    }
}

// ── Workflow ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout")]
    pub default_timeout: i32,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// Stages are loaded separately
    #[serde(default)]
    pub stages: Vec<WorkflowStage>,
}

fn default_true() -> bool { true }
fn default_timeout() -> i32 { 600 }

// ── WorkflowStage ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub stage_order: i32,
    #[serde(default = "default_group")]
    pub stage_group: String,
    #[serde(default = "default_task_template")]
    pub input_template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_key: Option<String>,
    #[serde(default = "default_always")]
    pub condition: String,
    #[serde(default = "default_stage_timeout")]
    pub timeout_secs: i32,
    #[serde(default)]
    pub retry_count: i32,
    #[serde(default)]
    pub auto_approve: bool,
}

fn default_group() -> String { "default".into() }
fn default_task_template() -> String { "{{task}}".into() }
fn default_always() -> String { "always".into() }
fn default_stage_timeout() -> i32 { 300 }

// ── WorkflowRun ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub workflow_name: String,
    #[serde(default = "default_manual")]
    pub trigger: String,
    #[serde(default)]
    pub status: String,       // pending|running|success|failed|cancelled
    #[serde(default)]
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub stage_results: Vec<StageResult>,
}

fn default_manual() -> String { "manual".into() }

// ── StageResult ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub stage_id: String,
    #[serde(default)]
    pub stage_order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_name: Option<String>,
    #[serde(default)]
    pub status: String,       // pending|running|success|failed|skipped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    #[serde(default)]
    pub tokens_used: i64,
    #[serde(default)]
    pub tool_calls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub retry_attempt: i32,
}

// ── WorkflowInfo (list summary, lighter than Workflow) ──────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub stage_count: usize,
    pub last_run_status: Option<String>,
}

// ── WorkflowRunSummary (result of an execution) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunSummary {
    pub run_id: String,
    pub workflow_name: String,
    pub status: String,
    pub stage_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub total_tokens: i64,
    pub stage_summaries: Vec<StageResultSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResultSummary {
    pub stage_order: i32,
    pub preset_name: String,
    pub status: String,
    pub output_summary: String,
}
