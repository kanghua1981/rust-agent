//! Pipeline file loader — reads/writes `.agent/pipelines/*.toml` files.
//!
//! Lookup order (highest priority first):
//!   1. `${project_dir}/.agent/pipelines/{name}.toml`  — project-local
//!   2. `~/.config/rust_agent/pipelines/{name}.toml`   — global, shared across projects
//!   3. Built-in default (only for name == "default")   — hardcoded fallback

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::dag::{PipelineDef, PipelineInfo, StageDef, StageContext, ToolAccess};

/// Directory inside the project where pipeline definitions live.
const PIPELINES_DIR: &str = ".agent/pipelines";

/// Resolve the global pipelines directory (`~/.config/rust_agent/pipelines/`).
fn global_pipelines_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rust_agent").join("pipelines"))
}

/// Load a specific pipeline by name.
///
/// Lookup order:
///   1. `{project_dir}/.agent/pipelines/{name}.toml`
///   2. `~/.config/rust_agent/pipelines/{name}.toml`
///   3. Built-in default (only when name == "default")
pub fn load_pipeline(project_dir: &Path, name: &str) -> Result<PipelineDef> {
    // 1. Project-local
    let local_path = pipeline_path(project_dir, name);
    if local_path.exists() {
        let content = fs::read_to_string(&local_path)
            .with_context(|| format!("Failed to read pipeline file: {}", local_path.display()))?;
        let def: PipelineDef = toml::from_str(&content)
            .with_context(|| format!("Failed to parse pipeline TOML: {}", local_path.display()))?;
        tracing::info!("Loaded pipeline '{}' from {}", name, local_path.display());
        return Ok(def);
    }

    // 2. Global
    if let Some(ref global_dir) = global_pipelines_dir() {
        let global_path = global_dir.join(format!("{}.toml", sanitize_filename(name)));
        if global_path.exists() {
            let content = fs::read_to_string(&global_path)
                .with_context(|| format!("Failed to read global pipeline: {}", global_path.display()))?;
            let def: PipelineDef = toml::from_str(&content)
                .with_context(|| format!("Failed to parse global pipeline TOML: {}", global_path.display()))?;
            tracing::info!("Loaded pipeline '{}' from global {}", name, global_path.display());
            return Ok(def);
        }
    }

    // 3. Built-in default
    if name == "default" {
        tracing::info!("Pipeline '{}' not found, returning built-in default", name);
        return Ok(builtin_default());
    }

    anyhow::bail!(
        "Pipeline '{}' not found in project ({}) or global config",
        name,
        local_path.display()
    )
}

/// List all available pipelines (project-local + global, deduplicated by name).
/// Project-local pipelines shadow global ones with the same name.
pub fn list_pipelines(project_dir: &Path) -> Result<Vec<PipelineInfo>> {
    // Use a map keyed by name so project-local overrides global.
    let mut map: std::collections::BTreeMap<String, PipelineInfo> = std::collections::BTreeMap::new();

    // 1. Global pipelines (lower priority)
    if let Some(global_dir) = global_pipelines_dir() {
        if global_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&global_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "toml") {
                        if let Some(info) = read_pipeline_info(&path) {
                            map.insert(info.name.clone(), info);
                        }
                    }
                }
            }
        }
    }

    // 2. Project-local pipelines (higher priority — overwrites global)
    let local_dir = project_dir.join(PIPELINES_DIR);
    if local_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&local_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml") {
                    if let Some(info) = read_pipeline_info(&path) {
                        map.insert(info.name.clone(), info);
                    }
                }
            }
        }
    }

    // 3. Always include built-in default if nothing registered under "default"
    if !map.contains_key("default") {
        let builtin = builtin_default();
        map.insert(
            "default".to_string(),
            PipelineInfo {
                name: builtin.name,
                description: builtin.description,
                stage_count: builtin.stages.len(),
            },
        );
    }

    Ok(map.into_values().collect())
}

/// Save (create or update) a pipeline definition.
pub fn save_pipeline(project_dir: &Path, def: &PipelineDef) -> Result<()> {
    let dir = project_dir.join(PIPELINES_DIR);
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    // Use the pipeline `name` as the filename stem (sanitised).
    let stem = sanitize_filename(&def.name);
    let path = dir.join(format!("{}.toml", stem));

    let content = toml::to_string_pretty(def)
        .with_context(|| format!("Failed to serialize pipeline '{}'", def.name))?;

    fs::write(&path, &content)
        .with_context(|| format!("Failed to write pipeline to {}", path.display()))?;

    tracing::info!("Saved pipeline '{}' to {}", def.name, path.display());
    Ok(())
}

/// Delete a **project-local** pipeline file by name.
/// Global pipelines cannot be deleted via WebSocket.
pub fn delete_pipeline(project_dir: &Path, name: &str) -> Result<()> {
    // Guard: don't allow deleting the builtin default.
    if name == "default" {
        let path = pipeline_path(project_dir, name);
        if !path.exists() {
            // Check if it's global or builtin
            if let Some(ref global_dir) = global_pipelines_dir() {
                let global_path = global_dir.join(format!("{}.toml", sanitize_filename(name)));
                if global_path.exists() {
                    anyhow::bail!("Cannot delete global pipeline '{}' — it is shared across projects", name);
                }
            }
            anyhow::bail!("Cannot delete built-in default pipeline");
        }
    }

    let path = pipeline_path(project_dir, name);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete pipeline: {}", path.display()))?;
        tracing::info!("Deleted pipeline '{}' from {}", name, path.display());
    } else {
        // Check if it's global — don't let users delete global via project UI
        if let Some(ref global_dir) = global_pipelines_dir() {
            let global_path = global_dir.join(format!("{}.toml", sanitize_filename(name)));
            if global_path.exists() {
                anyhow::bail!(
                    "Pipeline '{}' is a global pipeline at {} — cannot delete from project WebUI",
                    name,
                    global_path.display()
                );
            }
        }
        tracing::warn!("Pipeline '{}' not found for deletion at {}", name, path.display());
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn pipeline_path(project_dir: &Path, name: &str) -> PathBuf {
    let stem = sanitize_filename(name);
    project_dir.join(PIPELINES_DIR).join(format!("{}.toml", stem))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Try to parse pipeline info from a .toml file path.
fn read_pipeline_info(path: &Path) -> Option<PipelineInfo> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<PipelineDef>(&content) {
            Ok(def) => Some(PipelineInfo {
                name: def.name,
                description: def.description,
                stage_count: def.stages.len(),
            }),
            Err(e) => {
                tracing::warn!("Skipping invalid pipeline {}: {}", stem, e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read pipeline {}: {}", stem, e);
            None
        }
    }
}

/// Built-in default 3-stage pipeline (Planner → Executor → Checker).
/// Used when no pipeline files exist in the project.
pub fn builtin_default() -> PipelineDef {
    PipelineDef {
        name: "default".to_string(),
        description: "Built-in standard three-stage pipeline".to_string(),
        stages: vec![
            StageDef {
                id: "planner".to_string(),
                name: "Planner".to_string(),
                role: Some("planner".to_string()),
                model: None,
                tools: ToolAccess::ReadOnly,
                context: StageContext::Shared,
                system_prompt: None,
                initial_message: Some(
                    r#"The user wants to accomplish the following task:

{{task}}

Please analyze the task carefully using the conversation context above and the read-only tools available. You may use the read-only tools to explore the codebase and gather any additional information you need.
You also have access to `run_command` — use it ONLY for read-only exploration commands such as:
  git status, git log, git diff, git show, git branch, git remote -v,
  find, cat, ls, wc, head, tail, cargo metadata, etc.
Do NOT run any command that mutates state (no commits, pushes, file writes, installs, builds).

IMPORTANT: If the task is ambiguous, missing key details, or requires user decisions (e.g. choice of approach, naming, scope), use the `ask_user` tool to ask clarifying questions BEFORE producing the plan. Do NOT guess — ask.

Then output a detailed, numbered step-by-step plan describing exactly what changes and actions are needed. For each step, specify:
1. What action to take (create/edit/delete file, run command, etc.)
2. Which file(s) are involved
3. A brief description of the change
4. Any dependencies on other steps

⚠️  Do NOT execute any modifications — only produce the plan."#
                        .to_string(),
                ),
                inputs: vec![],
                artifact: Some(".agent/artifacts/plan.md".to_string()),
                on_pass: "executor".to_string(),
                on_fail: "done".to_string(),
                max_retries: None,
            },
            StageDef {
                id: "executor".to_string(),
                name: "Executor".to_string(),
                role: Some("executor".to_string()),
                model: None,
                tools: ToolAccess::All,
                context: StageContext::Shared,
                system_prompt: None,
                initial_message: Some(
                    r#"Execute the following plan step by step.

**Rules you MUST follow:**
- Use the actual tools for every action — do NOT just describe what you would do.
- Before touching any file, READ it first to see its current state.
- After modifying a file, READ it back immediately to confirm the change is present.
- Run any build/test command specified in the plan and show the real output.
- If a step fails, diagnose from the actual error and fix it before continuing.

Original task: {{task}}

--- PLAN ---
{{inputs.plan}}
--- END PLAN ---

Begin execution now."#
                        .to_string(),
                ),
                inputs: vec!["plan.md".to_string()],
                artifact: Some(".agent/artifacts/result.md".to_string()),
                on_pass: "checker".to_string(),
                on_fail: "checker".to_string(),
                max_retries: None,
            },
            StageDef {
                id: "checker".to_string(),
                name: "Checker".to_string(),
                role: Some("checker".to_string()),
                model: None,
                tools: ToolAccess::ReadOnly,
                context: StageContext::Shared,
                system_prompt: None,
                initial_message: Some(
                    r#"You are an independent code reviewer. Your job is to verify the implementation.

Original task: {{task}}

--- ORIGINAL PLAN ---
{{inputs.plan}}
--- END PLAN ---

--- EXECUTOR SELF-REPORT (do NOT trust this — verify yourself) ---
{{inputs.result}}
--- END REPORT ---

**Your instructions:**
1. For every file the plan says should be modified, call read_file and read it now.
2. Run build/test commands to verify correctness.
3. Check each success criterion in the plan.

**If you find issues, your FAIL report MUST include:**
- The exact file path
- The relevant lines you actually read from the file (quote them)
- What those lines should say instead
This evidence is critical so the Executor cannot claim the change is already there.

**End your response with EXACTLY one of these two blocks — no exceptions:**

If everything is verified correct:
```
## REVIEW_ARTIFACT
### PASS ✅
```

If anything is wrong or unverified:
```
## REVIEW_ARTIFACT
### FAIL ❌
- Issue 1: `path/to/file` line N — current content: `<quoted lines>` — required: `<what it should be>`
- Issue 2: ...
```

Do NOT use both PASS ✅ and FAIL ❌ in the same response."#
                        .to_string(),
                ),
                inputs: vec!["plan.md".to_string(), "result.md".to_string()],
                artifact: Some(".agent/artifacts/review.md".to_string()),
                on_pass: "done".to_string(),
                on_fail: "executor".to_string(),
                max_retries: Some(3),
            },
        ],
    }
}
