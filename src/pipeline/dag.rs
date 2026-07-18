//! Pipeline data structures — serde-compatible definitions for `.agent/pipelines/*.toml`.
//!
//! ```toml
//! name = "default"
//! description = "Standard three-stage pipeline"
//!
//! [[stages]]
//! id = "planner"
//! name = "Planner"
//! role = "planner"
//! tools = "read_only"
//! context = "shared"
//! artifact = ".agent/artifacts/plan.md"
//! on_pass = "executor"
//!
//! [[stages]]
//! id = "executor"
//! name = "Executor"
//! role = "executor"
//! tools = "all"
//! inputs = ["plan.md"]
//! artifact = ".agent/artifacts/result.md"
//! on_pass = "checker"
//! on_fail = "checker"
//!
//! [[stages]]
//! id = "checker"
//! name = "Checker"
//! role = "checker"
//! tools = "read_only"
//! inputs = ["plan.md", "result.md"]
//! artifact = ".agent/artifacts/review.md"
//! on_pass = "done"
//! on_fail = "executor"
//! max_retries = 3
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A complete pipeline definition (one per .toml file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDef {
    /// Display name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Ordered list of stages forming a DAG.
    pub stages: Vec<StageDef>,
}

impl PipelineDef {
    /// Find a stage by id.
    pub fn stage(&self, id: &str) -> Option<&StageDef> {
        self.stages.iter().find(|s| s.id == id)
    }

    /// The first stage drives the entry point.
    pub fn entry_stage(&self) -> Option<&StageDef> {
        self.stages.first()
    }
}

/// A single stage in the pipeline DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDef {
    /// Unique id within the pipeline (used for `on_pass` / `on_fail` routing).
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Reference to a role defined in `models.toml` `[roles.xxx]`.
    /// When set, inherits model + system_prompt defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Override or directly specify the model alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Tool access level for this stage.
    #[serde(default)]
    pub tools: ToolAccess,
    /// Context sharing mode.
    #[serde(default)]
    pub context: StageContext,
    /// Custom system prompt — overrides the role default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Stage initial user message. Supports `{{task}}`, `{{inputs.xxx}}`,
    /// `{{stage.previous}}`, `{{artifact.xxx}}` template variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_message: Option<String>,
    /// Artifact file paths to read and inject into `{{inputs.xxx}}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// Artifact file path to enforce at stage completion.
    /// The LLM is instructed to write its final output here and the file
    /// is verified after the stage finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
    /// Stage id to jump to on successful completion.
    /// Use `"done"` to terminate the pipeline.
    #[serde(default = "default_on_pass")]
    pub on_pass: String,
    /// Stage id to jump to on failure (or after max_retries exhausted).
    /// Use `"done"` to terminate on failure.
    #[serde(default = "default_on_fail")]
    pub on_fail: String,
    /// Maximum retry attempts before following `on_fail`. Default: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

// Default values for serde
fn default_on_pass() -> String {
    "done".to_string()
}
fn default_on_fail() -> String {
    "done".to_string()
}

/// Tool access level for a pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccess {
    /// All tools available (read + write + execute).
    #[default]
    All,
    /// Read-only tools + safe commands (no mutations).
    ReadOnly,
}

/// Context sharing mode between pipeline stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StageContext {
    /// Share the same Conversation — this stage sees all previous stages' tool calls.
    #[default]
    Shared,
    /// Fresh Conversation — only system_prompt + initial_message + inputs.
    /// Results are bridged via artifact files.
    Isolated,
}

/// After a stage completes: PASS or FAIL verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageVerdict {
    Pass,
    Fail { reason: String },
}

/// Accumulated artifacts during pipeline execution.
/// Maps stage_id → resolved artifact path (relative to project_dir).
#[derive(Debug, Clone, Default)]
pub struct ArtifactMap {
    /// stage_id → (absolute artifact path on disk, content cached if read)
    entries: HashMap<String, (PathBuf, Option<String>)>,
}

impl ArtifactMap {
    pub fn insert(&mut self, stage_id: &str, path: PathBuf, content: Option<String>) {
        self.entries
            .insert(stage_id.to_string(), (path, content));
    }

    pub fn get_path(&self, stage_id: &str) -> Option<&PathBuf> {
        self.entries.get(stage_id).map(|(p, _)| p)
    }

    /// Resolve `{{inputs.xxx}}` by matching filename stems against known artifacts.
    /// Walks the map looking for an artifact whose filename (without dir) matches `name`.
    pub fn resolve_input(&self, name: &str, project_dir: &std::path::Path) -> Option<String> {
        // Direct lookup by stage_id
        if let Some((path, cached)) = self.entries.get(name) {
            if let Some(ref content) = cached {
                return Some(content.clone());
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                return Some(content);
            }
        }
        // Try by artifact path filename match
        for (_sid, (path, cached)) in &self.entries {
            let fname = path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
            if fname == name || fname == format!("{}.md", name) {
                if let Some(ref content) = cached {
                    return Some(content.clone());
                }
                if let Ok(content) = std::fs::read_to_string(path) {
                    return Some(content);
                }
            }
        }
        // Fallback: try as a relative path from project_dir
        let candidate = project_dir.join(name);
        if candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                return Some(content);
            }
        }
        None
    }
}

/// Lightweight pipeline info for listing (without full stage definitions).
#[derive(Debug, Clone, Serialize)]
pub struct PipelineInfo {
    pub name: String,
    pub description: String,
    pub stage_count: usize,
}
