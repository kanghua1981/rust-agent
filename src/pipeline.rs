//! Multi-role pipeline runner.
//!
//! When `[pipeline] enabled = true` in models.toml every user message is
//! automatically routed through a three-stage process instead of the normal
//! single-model loop:
//!
//!   Stage 1 – **Planner**  : read-only exploration → produces a PLAN_ARTIFACT
//!   Stage 2 – **Executor** : full toolset, follows the plan → RESULT_ARTIFACT
//!   Stage 3 – **Checker**  : read-only + commands, validates → REVIEW_ARTIFACT (PASS / FAIL)
//!
//! If Checker returns FAIL the Executor is retried up to `max_checker_retries`
//! times, with the Checker's feedback injected into the next attempt.
//!
//! All stages are fully transparent: the active role and model are shown in the
//! UI via `on_role_header()` before each LLM call.

use anyhow::Result;

use crate::agent::Agent;
use crate::confirm::ConfirmAction;

pub struct PipelineRunner;

impl PipelineRunner {
    /// Route a user message through the full multi-role pipeline.
    /// (Planner → Executor → Checker with feedback loop)
    pub async fn run(agent: &mut Agent, task: &str) -> Result<String> {
        let pipeline_cfg = agent.pipeline_config().cloned();

        // ── Stage 1: Planner ──────────────────────────────────────────────────
        // generate_plan() already uses call_llm_as_role("planner", ...) with
        // read-only tools, so no extra wiring is needed here.
        let plan = agent.generate_plan(task).await?;

        // ── Optional: confirm before executing ───────────────────────────────
        let require_confirm = pipeline_cfg
            .as_ref()
            .map(|p| p.confirm_plan())
            .unwrap_or(true);

        if require_confirm {
            let preview = crate::ui::truncate_str(&plan, 3000);
            let proceed = agent
                .output_arc()
                .confirm(&ConfirmAction::ReviewPlan { preview: preview.to_string() });
            if !proceed {
                agent.output_arc().on_warning("Pipeline cancelled by user.");
                return Ok("Pipeline cancelled.".to_string());
            }
        }

        // ── Stage 2 + 3: Executor → Checker feedback loop ────────────────────
        let max_retries = pipeline_cfg.as_ref().map(|p| p.max_retries()).unwrap_or(2);
        let mut last_result = String::new();
        let mut checker_feedback = String::new();

        for attempt in 0u32..=max_retries {
            // ── Executor ──────────────────────────────────────────────────────
            let exec_prompt = build_executor_prompt(task, &plan, attempt, &checker_feedback);
            last_result = agent
                .run_pipeline_stage("executor", &exec_prompt, false)
                .await?;
            agent.output_arc().on_stage_end("Executor");

            // ── Checker ───────────────────────────────────────────────────────
            let check_prompt = build_checker_prompt(task, &plan, &last_result);
            let review = agent
                .run_pipeline_stage("checker", &check_prompt, true)
                .await?;
            agent.output_arc().on_stage_end("Checker");

            if is_pass(&review) {
                agent.output_arc().on_warning(&format!(
                    "✅ Pipeline complete — Checker: PASS (attempt {})",
                    attempt + 1
                ));
                return Ok(last_result);
            }

            checker_feedback = review.clone();

            if attempt < max_retries {
                agent.output_arc().on_warning(&format!(
                    "🔄 Checker found issues (attempt {}/{}), retrying executor…",
                    attempt + 1,
                    max_retries + 1
                ));
            } else {
                agent.output_arc().on_warning(
                    "⚠️  Pipeline: Checker still reports issues after max retries. \
                     Returning last executor result.",
                );
            }
        }

        Ok(last_result)
    }

    /// Lightweight two-stage pipeline: Plan → Execute (no Checker).
    ///
    /// Suitable for medium-complexity tasks where verification overhead
    /// is not justified (e.g. multi-file refactors within a single module).
    pub async fn run_plan_and_execute(agent: &mut Agent, task: &str) -> Result<String> {
        // ── Stage 1: Planner ──────────────────────────────────────────────
        let plan = agent.generate_plan(task).await?;

        // ── Optional: confirm before executing ───────────────────────────
        let pipeline_cfg = agent.pipeline_config().cloned();
        let require_confirm = pipeline_cfg
            .as_ref()
            .map(|p| p.confirm_plan())
            .unwrap_or(true);

        if require_confirm {
            let preview = crate::ui::truncate_str(&plan, 3000);
            let proceed = agent
                .output_arc()
                .confirm(&ConfirmAction::ReviewPlan { preview: preview.to_string() });
            if !proceed {
                agent.output_arc().on_warning("Plan+Execute cancelled by user.");
                return Ok("Pipeline cancelled.".to_string());
            }
        }

        // ── Stage 2: Executor ─────────────────────────────────────────────
        let exec_prompt = build_executor_prompt(task, &plan, 0, "");
        let result = agent
            .run_pipeline_stage("executor", &exec_prompt, false)
            .await?;
        agent.output_arc().on_stage_end("Executor");

        agent.output_arc().on_warning("✅ Plan+Execute complete (no Checker stage).");
        Ok(result)
    }
}

// ── Prompt builders ───────────────────────────────────────────────────────────

fn build_executor_prompt(task: &str, plan: &str, attempt: u32, feedback: &str) -> String {
    if attempt == 0 {
        format!(
            "Execute the following plan step by step.\n\
             \n\
             **Rules you MUST follow:**\n\
             - Use the actual tools for every action — do NOT just describe what you would do.\n\
             - Before touching any file, READ it first to see its current state.\n\
             - After modifying a file, READ it back immediately to confirm the change is present.\n\
             - Run any build/test command specified in the plan and show the real output.\n\
             - If a step fails, diagnose from the actual error and fix it before continuing.\n\
             \n\
             Original task:\n{task}\n\n\
             --- PLAN ---\n{plan}\n--- END PLAN ---\n\n\
             Begin execution now."
        )
    } else {
        format!(
            "⛔ IMPORTANT: Your previous attempt was reviewed by an independent Checker \
             who READ THE ACTUAL FILES and found that the changes you claimed to have made \
             are NOT present in the files on disk. This is retry attempt {attempt}.\n\
             \n\
             The Checker's evidence (actual file content) is shown below. \
             Do NOT argue with it — the Checker read the real files.\n\
             \n\
             **MANDATORY steps before doing anything else:**\n\
             1. Use read_file to read each file listed in the Checker's issues RIGHT NOW.\n\
             2. Look at what the file ACTUALLY contains vs what the plan requires.\n\
             3. Use edit_file or write_file to make the missing change.\n\
             4. After the edit, read_file again to confirm the change is present in the file.\n\
             5. Run required build/test commands and show the real output.\n\
             \n\
             ⚠️  Any documentation or design files that say a feature is 'implemented' \
             are IRRELEVANT — only what is in the actual source files matters.\n\
             ⚠️  Do NOT claim a step is done without first reading the file \
             and showing the relevant lines that prove it.\n\
             \n\
             Original task:\n{task}\n\n\
             --- PLAN ---\n{plan}\n--- END PLAN ---\n\n\
             --- CHECKER FINDINGS (includes actual file content as evidence) ---\n\
             {feedback}\n\
             --- END CHECKER FINDINGS ---\n\n\
             Start by calling read_file on the first problematic file listed above, right now."
        )
    }
}

fn build_checker_prompt(task: &str, plan: &str, result: &str) -> String {
    format!(
        "You are an independent code reviewer. Your job is to verify the implementation.\n\n\
         Original task:\n{task}\n\n\
         --- ORIGINAL PLAN ---\n{plan}\n--- END PLAN ---\n\n\
         --- EXECUTOR SELF-REPORT (do NOT trust this — verify yourself) ---\n{result}\n--- END REPORT ---\n\n\
         **Your instructions:**\n\
         1. For every file the plan says should be modified, call read_file and read it now.\n\
         2. Run build/test commands to verify correctness.\n\
         3. Check each success criterion in the plan.\n\
         \n\
         **If you find issues, your FAIL report MUST include:**\n\
         - The exact file path\n\
         - The relevant lines you actually read from the file (quote them)\n\
         - What those lines should say instead\n\
         This evidence is critical so the Executor cannot claim the change is already there.\n\
         \n\
         **End your response with EXACTLY one of these two blocks — no exceptions:**\n\
         \n\
         If everything is verified correct:\n\
         ```\n\
         ## REVIEW_ARTIFACT\n\
         ### PASS ✅\n\
         ```\n\
         \n\
         If anything is wrong or unverified:\n\
         ```\n\
         ## REVIEW_ARTIFACT\n\
         ### FAIL ❌\n\
         - Issue 1: `path/to/file` line N — current content: `<quoted lines>` — required: `<what it should be>`\n\
         - Issue 2: ...\n\
         ```\n\
         \n\
         Do NOT use both PASS ✅ and FAIL ❌ in the same response."
    )
}

/// Parse the ## REVIEW_ARTIFACT section to determine PASS or FAIL.
/// Only looks inside the artifact block to avoid false matches in the analysis text.
fn is_pass(review: &str) -> bool {
    let upper = review.to_uppercase();
    // Find the REVIEW_ARTIFACT marker; fall back to whole text if absent.
    let start = upper.find("REVIEW_ARTIFACT").unwrap_or(0);
    let section = &upper[start..];
    let pass_pos = section.find("### PASS").or_else(|| section.find("PASS ✅"));
    let fail_pos = section.find("### FAIL").or_else(|| section.find("FAIL ❌"));
    match (pass_pos, fail_pos) {
        (Some(_), None) => true,           // PASS present, no FAIL
        (Some(pp), Some(fp)) => pp < fp,   // PASS comes before any FAIL mention
        _ => false,
    }
}
