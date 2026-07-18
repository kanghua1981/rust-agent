//! DAG-based pipeline execution engine.
//!
//! Replaces the hardcoded 3-stage Planner→Executor→Checker loop with a
//! fully configurable DAG defined in `.agent/pipelines/*.toml`.
//!
//! Each stage specifies:
//!   - Which role/model to use
//!   - Tool access level (all / read-only)
//!   - Context mode (shared / isolated)
//!   - Custom system_prompt + initial_message (with `{{variables}}`)
//!   - Input artifacts to inject
//!   - Output artifact to enforce
//!   - Routing: on_pass / on_fail → next stage or "done"
//!   - max_retries for failure loops (e.g. checker → executor)

use std::path::Path;

use anyhow::Result;

use crate::agent::Agent;
use crate::agent::{clear_interrupt, is_interrupted};
use crate::conversation::Conversation;
use crate::conversation::Message;

use super::dag::{ArtifactMap, PipelineDef, StageContext, StageDef, StageVerdict, ToolAccess};

/// Outcome of executing a single stage.
struct StageResult {
    /// Full text output from the LLM.
    text: String,
}

/// Execute a pipeline DAG for the given task.
///
/// # Flow
/// 1. Start at `pipeline.stages[0]`.
/// 2. Execute each stage, accumulating artifacts.
/// 3. Follow `on_pass` / `on_fail` links with retry support.
/// 4. Stop when a stage links to `"done"`.
pub async fn run(agent: &mut Agent, pipeline: &PipelineDef, task: &str) -> Result<String> {
    let entry = match pipeline.entry_stage() {
        Some(s) => s.id.clone(),
        None => {
            agent.output_arc().on_warning("Pipeline has no stages — falling back to basic loop.");
            return Ok(String::new());
        }
    };

    let mut artifacts = ArtifactMap::default();
    let mut current_id = entry;
    let mut last_output = String::new();
    let mut previous_stage_output: Option<String> = None;

    // Guard against infinite loops.
    let max_hops = pipeline.stages.len() * 10;
    let mut hops = 0;

    while current_id != "done" {
        hops += 1;
        if hops > max_hops {
            agent
                .output_arc()
                .on_warning("⚠️ Pipeline: max hop limit reached — stopping.");
            break;
        }

        let stage = match pipeline.stage(&current_id) {
            Some(s) => s,
            None => {
                agent.output_arc().on_warning(&format!(
                    "⚠️ Pipeline: stage '{}' not found — stopping.",
                    current_id
                ));
                break;
            }
        };

        let max_retries = stage.max_retries.unwrap_or(0);
        let mut retry_count: u32 = 0;
        let mut stage_output = String::new();

        loop {
            // ── Sync interrupts ──────────────────────────────────────────
            if is_interrupted() {
                agent.request_interrupt();
                clear_interrupt();
            }

            // ── Execute the stage ────────────────────────────────────────
            let result = execute_stage(
                agent,
                stage,
                task,
                &artifacts,
                &previous_stage_output,
                retry_count,
            )
            .await?;
            stage_output = result.text;

            // ── Enforce artifact ─────────────────────────────────────────
            if let Some(ref artifact_path) = stage.artifact {
                let full_path = agent.project_dir.join(artifact_path);
                if let Err(e) =
                    enforce_artifact(agent, stage, &stage_output, &full_path).await
                {
                    // Artifact enforcement failure → treat as FAIL
                    let reason = format!("Artifact enforcement failed: {}", e);
                    agent.output_arc().on_warning(&format!("❌ {}", reason));
                    let fail_id = &stage.on_fail;
                    if fail_id == "done" {
                        return Ok(stage_output);
                    }
                    current_id = fail_id.clone();
                    last_output = stage_output;
                    break;
                }
                // Record artifact for downstream stages.
                artifacts.insert(&stage.id, full_path, Some(stage_output.clone()));
            }

            // ── PASS/FAIL routing ────────────────────────────────────────
            let verdict = super::deprecated::parse_verdict(&stage_output);
            match verdict {
                StageVerdict::Pass => {
                    agent
                        .output_arc()
                        .on_warning(&format!("✅ Stage '{}': PASS", stage.name));
                    current_id = stage.on_pass.clone();
                    last_output = stage_output.clone();
                    previous_stage_output = Some(stage_output.clone());
                    break;
                }
                StageVerdict::Fail { reason } => {
                    agent
                        .output_arc()
                        .on_warning(&format!("❌ Stage '{}': FAIL — {}", stage.name, reason));
                    retry_count += 1;
                    if retry_count <= max_retries {
                        agent.output_arc().on_warning(&format!(
                            "🔄 Retrying stage '{}' ({}/{})...",
                            stage.name, retry_count, max_retries
                        ));
                        continue;
                    }
                    // Exhausted retries → follow on_fail.
                    current_id = stage.on_fail.clone();
                    last_output = stage_output.clone();
                    previous_stage_output = Some(stage_output.clone());
                    break;
                }
            }
        }
    }

    agent.output_arc().on_warning("✅ Pipeline complete.");
    Ok(last_output)
}

/// Execute a single pipeline stage.
async fn execute_stage(
    agent: &mut Agent,
    stage: &StageDef,
    task: &str,
    artifacts: &ArtifactMap,
    previous_output: &Option<String>,
    retry_attempt: u32,
) -> Result<StageResult> {
    // ── Build initial message with template substitution ────────────────
    let mut msg = build_stage_message(stage, task, previous_output, retry_attempt);

    // ── Resolve {{inputs.xxx}} with actual artifact content ─────────────
    msg = resolve_inputs(&msg, stage, artifacts, &agent.project_dir);

    // ── Resolve role for UI display ─────────────────────────────────────
    let display_role = stage.role.as_deref().unwrap_or(&stage.id);

    // ── Set up tool definitions ─────────────────────────────────────────
    let readonly = matches!(stage.tools, ToolAccess::ReadOnly);

    match stage.context {
        StageContext::Shared => {
            // Shared mode: use the agent's existing conversation.
            // The stage output is appended to the main conversation history.
            let text = agent
                .run_pipeline_stage(display_role, &msg, readonly)
                .await?;
            Ok(StageResult { text })
        }
        StageContext::Isolated => {
            // Isolated mode: fresh Conversation, no history from prior stages.
            // Only system_prompt + initial_message + inputs are visible.
            let sp = stage.system_prompt.clone().unwrap_or_else(|| {
                format!("You are the {} stage.", stage.name)
            });

            let mut isolated_conv = Conversation::with_system_prompt(sp);
            isolated_conv.add_message(Message::user(&msg));

            // Temporarily swap out the agent's conversation.
            let saved_conv = std::mem::replace(
                &mut agent.conversation,
                Conversation::new(&agent.project_dir),
            );

            let tool_defs = if readonly {
                crate::agent::with_ask_user(
                    agent.tool_executor.readonly_definitions(),
                )
            } else {
                crate::agent::with_ask_user(agent.tool_executor.definitions())
            };

            let opts = crate::agent::ToolLoopOptions {
                role: display_role.to_string(),
                enable_guardrails: false,
                enable_guidance: true,
                notify_file_created: false,
                handle_upload_image: false,
            };

            let result = agent.run_tool_loop(&mut isolated_conv, &tool_defs, &opts).await;

            // Restore the main conversation.
            agent.conversation = saved_conv;

            match result {
                Ok(text) => {
                    // Append a brief summary to the main conversation so the
                    // user can see the stage completed.
                    agent.conversation.add_message(Message::user(&format!(
                        "[Isolated stage '{}' completed. See artifact for full output.]",
                        stage.name
                    )));

                    Ok(StageResult { text })
                }
                Err(e) => {
                    agent.conversation.add_message(Message::user(&format!(
                        "[Isolated stage '{}' FAILED: {}]",
                        stage.name, e
                    )));
                    Err(e)
                }
            }
        }
    }
}

/// Substitute template variables in the stage's initial message.
///
/// Supported variables:
///   `{{task}}`             — the original user task
///   `{{stage.previous}}`   — full text output from the previous stage
///   `{{retry_attempt}}`    — current retry count (0-based)
///   `{{inputs.xxx}}`       — resolved later by `resolve_inputs()`
fn build_stage_message(
    stage: &StageDef,
    task: &str,
    previous_output: &Option<String>,
    retry_attempt: u32,
) -> String {
    let template = stage.initial_message.as_deref().unwrap_or("{{task}}");

    let mut result = template.to_string();

    // {{task}}
    result = result.replace("{{task}}", task);

    // {{retry_attempt}}
    result = result.replace("{{retry_attempt}}", &retry_attempt.to_string());

    // {{stage.previous}}
    if let Some(ref prev) = previous_output {
        result = result.replace("{{stage.previous}}", prev);
    }

    result
}

/// Resolve {{inputs.xxx}} and {{artifact.xxx}} variables with actual artifact content.
fn resolve_inputs(
    message: &str,
    stage: &StageDef,
    artifacts: &ArtifactMap,
    project_dir: &Path,
) -> String {
    let mut result = message.to_string();
    for input_name in &stage.inputs {
        let placeholder = format!("{{{{inputs.{}}}}}", input_name);
        if result.contains(&placeholder) {
            if let Some(content) = artifacts.resolve_input(input_name, project_dir) {
                result = result.replace(&placeholder, &content);
            } else {
                // File not found — leave a note in the message.
                result = result.replace(
                    &placeholder,
                    &format!("(artifact '{}' not yet available)", input_name),
                );
            }
        }
        // Also try {{artifact.xxx}} variant
        let alt_placeholder = format!("{{{{artifact.{}}}}}", input_name);
        if result.contains(&alt_placeholder) {
            if let Some(content) = artifacts.resolve_input(input_name, project_dir) {
                result = result.replace(&alt_placeholder, &content);
            } else {
                result = result.replace(
                    &alt_placeholder,
                    &format!("(artifact '{}' not yet available)", input_name),
                );
            }
        }
    }

    // Also handle any {{artifact.xxx}} that aren't in inputs
    let re = regex::Regex::new(r"\{\{artifact\.([a-zA-Z0-9_\-\./]+)\}\}").ok();
    if let Some(re) = re {
        let mut replacements: Vec<(String, String)> = Vec::new();
        for cap in re.captures_iter(&result) {
            let full = cap.get(0).unwrap().as_str().to_string();
            let name = cap.get(1).unwrap().as_str().to_string();
            if !replacements.iter().any(|(m, _)| m == &full) {
                if let Some(content) = artifacts.resolve_input(&name, project_dir) {
                    replacements.push((full, content));
                } else {
                    replacements.push((full, format!("(artifact '{}' not available)", name)));
                }
            }
        }
        for (pat, repl) in replacements {
            result = result.replace(&pat, &repl);
        }
    }

    result
}

/// Enforce artifact output: check if the LLM wrote to the artifact path.
/// If not, auto-write the stage output as fallback.
async fn enforce_artifact(
    agent: &mut Agent,
    stage: &StageDef,
    stage_output: &str,
    artifact_path: &Path,
) -> Result<()> {
    // Ensure the parent directory exists.
    if let Some(parent) = artifact_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Strategy 1: Check if the LLM already wrote to the path.
    if artifact_path.is_file() {
        let content = std::fs::read_to_string(artifact_path)?;
        if !content.trim().is_empty() {
            tracing::info!(
                "Artifact '{}' for stage '{}' exists and is non-empty ({} bytes)",
                artifact_path.display(),
                stage.id,
                content.len()
            );
            return Ok(());
        }
    }

    // Strategy 2: Auto-write the stage output as fallback.
    agent.output_arc().on_warning(&format!(
        "📄 Auto-writing artifact for stage '{}' → {}",
        stage.name,
        artifact_path.display()
    ));
    std::fs::write(artifact_path, stage_output)?;

    tracing::info!(
        "Artifact '{}' written for stage '{}' ({} bytes)",
        artifact_path.display(),
        stage.id,
        stage_output.len()
    );
    Ok(())
}
