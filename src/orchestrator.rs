//! Workflow orchestrator — runs multi-stage pipelines across remote Agent nodes.
//!
//! Each stage connects to a different Agent server (via Preset URL), sends the
//! resolved prompt, collects output, and records results in `global.db`.
//!
//! # Template variables
//!
//! | Variable | Description |
//! |----------|-------------|
//! | `{{task}}` | Original user task |
//! | `{{stage.<output_key>.output}}` | Output from a previous stage (by its outputKey) |
//! | `{{stage.<output_key>.summary}}` | Summary from a previous stage |

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde_json::json;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::db::GlobalDb;
use crate::db::Preset;
use crate::db::StageResult;
use crate::db::Workflow;
use crate::db::WorkflowRun;
use crate::db::WorkflowStage;
use crate::output::AgentOutput;

const PING_INTERVAL: Duration = Duration::from_secs(2);
const INACTIVITY_ABORT: Duration = Duration::from_secs(120); // 2 min no msg → abort

// ── Orchestrator ───────────────────────────────────────────────────────────

/// Run a workflow to completion, reporting progress to `output`.
pub async fn run_workflow(
    db: &GlobalDb,
    workflow_id: &str,
    task: &str,
    output: &Arc<dyn AgentOutput>,
) -> Result<WorkflowRun> {
    // 1. Load workflow + stages
    let wf = db.get_workflow(workflow_id)
        .context("Failed to load workflow")?
        .context("Workflow not found")?;

    if !wf.enabled {
        anyhow::bail!("Workflow '{}' is disabled", wf.name);
    }

    // 2. Create run record
    let mut run = WorkflowRun {
        id: uuid::Uuid::new_v4().to_string(),
        workflow_id: wf.id.clone(),
        workflow_name: wf.name.clone(),
        trigger: "manual".into(),
        status: "running".into(),
        task: task.to_string(),
        ..Default::default()
    };
    db.create_run(&run)?;

    let run_id = run.id.clone();
    output.on_warning(&format!("🚀 开始执行工作流 '{}' (run={})", wf.name, &run_id[..8]));

    // 3. Sort stages by order
    let mut stages = wf.stages.clone();
    stages.sort_by_key(|s| s.stage_order);

    // 4. Stage outputs indexed by output_key
    let mut outputs: HashMap<String, String> = HashMap::new();
    let mut summaries: HashMap<String, String> = HashMap::new();
    let mut overall_status = "success".to_string();

    for stage in &stages {
        // 4a. Condition evaluation
        if !evaluate_condition(&stage.condition, &overall_status) {
            output.on_warning(&format!(
                "⏭ 跳过阶段 #{} '{}' (条件: {}, 当前状态: {})",
                stage.stage_order + 1, stage.stage_group, stage.condition, overall_status,
            ));
            let sr = skipped_stage_result(&run_id, stage);
            db.save_stage_result(&sr)?;
            run.stage_results.push(sr);
            continue;
        }

        // 4b. Resolve input template
        let prompt = resolve_template(&stage.input_template, task, &outputs, &summaries);
        output.on_warning(&format!(
            "▶ 阶段 #{} '{}' 开始执行…",
            stage.stage_order + 1, stage.stage_group,
        ));

        // 4c. Resolve connection target from embedded fields
        //     New stages carry server_url directly; fall back to preset_id for
        //     backward compatibility with workflows saved before migration 005.
        let (server_url, workdir, model, agent_mode) = resolve_stage_target(db, stage)?;

        if server_url.is_empty() {
            output.on_warning("⚠️ 阶段没有指定目标服务器，跳过");
            let sr = skipped_stage_result(&run_id, stage);
            db.save_stage_result(&sr)?;
            run.stage_results.push(sr);
            continue;
        }

        // 4d. Execute on remote agent (with retries)
        let timeout = if stage.timeout_secs > 0 { stage.timeout_secs as u64 } else { 300 };
        let max_retries = stage.retry_count.max(0) as u32;
        let mut last_error: Option<String> = None;

        let mut stage_result = None;
        for retry_attempt in 0..=max_retries {
            if retry_attempt > 0 {
                output.on_warning(&format!(
                    "🔄 阶段 #{} 重试 {}/{}",
                    stage.stage_order + 1, retry_attempt, max_retries,
                ));
            }

            match execute_single_stage(
                &server_url,
                &workdir,
                &model,
                &agent_mode,
                &prompt,
                stage,
                timeout,
                output,
            ).await {
                Ok(mut sr) => {
                    sr.retry_attempt = retry_attempt as i32;
                    stage_result = Some(sr);
                    break;
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    if retry_attempt < max_retries {
                        output.on_warning(&format!("⚠️ 阶段失败: {}，准备重试…", e));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        match stage_result {
            Some(mut sr) => {
                // 4e. Save output for downstream stages
                if let Some(ref out_key) = stage.output_key {
                    if !out_key.is_empty() {
                        if let Some(ref text) = sr.output_text {
                            outputs.insert(out_key.clone(), text.clone());
                        }
                        if let Some(ref summary) = sr.output_summary {
                            summaries.insert(out_key.clone(), summary.clone());
                        } else if let Some(ref text) = sr.output_text {
                            // Use first 300 chars as summary
                            let s = text.chars().take(300).collect::<String>();
                            summaries.insert(out_key.clone(), s);
                        }
                    }
                }

                if sr.status == "failed" {
                    overall_status = "failed".to_string();
                }

                db.save_stage_result(&sr)?;
                run.stage_results.push(sr);
            }
            None => {
                let err = format!("阶段 #{} 所有重试均失败: {}",
                    stage.stage_order + 1,
                    last_error.as_deref().unwrap_or("unknown"));
                output.on_warning(&format!("❌ {}", err));
                overall_status = "failed".to_string();
                let sr = failed_stage_result(&run_id, stage, &err);
                db.save_stage_result(&sr)?;
                db.update_run_status(&run_id, "failed", Some(&err))?;
                run.stage_results.push(sr);
                run.status = "failed".into();
                run.error_message = Some(err);
                return Ok(run);
            }
        }
    }

    // 5. Finalize
    db.update_run_status(&run_id, &overall_status, None)?;
    run.status = overall_status;
    run.finished_at = Some(chrono::Utc::now().to_rfc3339());

    output.on_warning(&format!(
        "✅ 工作流 '{}' 完成 — 状态: {} ({} 个阶段)",
        wf.name, run.status, run.stage_results.len(),
    ));

    Ok(run)
}

// ── Single stage execution ─────────────────────────────────────────────────

async fn execute_single_stage(
    server_url: &str,
    workdir: &Option<String>,
    model: &Option<String>,
    agent_mode: &str,
    prompt: &str,
    stage: &WorkflowStage,
    timeout_secs: u64,
    output: &Arc<dyn AgentOutput>,
) -> Result<StageResult> {
    let display_name = stage.stage_group.clone();
    let server_url = server_url.to_string();

    let mut sr = StageResult {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: String::new(), // filled in by caller
        stage_id: stage.id.clone(),
        stage_order: stage.stage_order,
        preset_name: Some(display_name.clone()),
        status: "running".into(),
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };

    output.on_warning(&format!("  🔗 连接到 {} ({})", display_name, server_url));

    // Use the server URL directly — the user_message + ready protocol
    let ws_url = if server_url.ends_with("/agent") {
        server_url.clone()
    } else {
        format!("{}/agent", server_url.trim_end_matches('/'))
    };

    let (ws_stream, _) = connect_async(&ws_url).await
        .context(format!("连接失败: {}", ws_url))?;

    let (mut write, mut read) = ws_stream.split();

    // Wait for ready
    let mut _connected = false;

    // Send set_mode if specified
    if !agent_mode.is_empty() && agent_mode != "auto" {
        let mode_msg = json!({ "type": "set_mode", "data": { "mode": agent_mode } });
        let _ = write.send(Message::Text(mode_msg.to_string().into())).await;
    }

    // Send user_message (with optional workdir/model from stage config)
    let mut msg_data = serde_json::json!({ "text": prompt });
    if let Some(ref wd) = workdir {
        msg_data["workdir"] = serde_json::json!(wd);
    }
    if let Some(ref m) = model {
        msg_data["model"] = serde_json::json!(m);
    }
    let initial_msg = json!({
        "type": "user_message",
        "data": msg_data
    });
    write.send(Message::Text(initial_msg.to_string().into())).await
        .context("发送任务失败")?;

    let mut streaming_answer = String::new();
    let start_time = Instant::now();
    let mut last_msg_at = Instant::now();
    let total_timeout = Duration::from_secs(timeout_secs);
    let mut token_count: i64 = 0;
    let auto_approve = stage.auto_approve;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_msg_at = Instant::now();
                        let event: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        match event["type"].as_str() {
                            Some("ready") => {
                                _connected = true;
                                let wd = event["data"]["workdir"].as_str().unwrap_or("?");
                                output.on_warning(&format!(
                                    "  ✅ 已连接 — workdir={}", wd
                                ));
                            }
                            Some("stream_start") => {
                                output.on_stream_start();
                            }
                            Some("stream_end") => {
                                output.on_stream_end();
                            }
                            Some("streaming_token") => {
                                let token = event["data"]["token"].as_str()
                                    .or_else(|| event["token"].as_str())
                                    .unwrap_or("");
                                if !token.is_empty() {
                                    output.on_streaming_text(token);
                                    streaming_answer.push_str(token);
                                    token_count += 1;
                                }
                            }
                            Some("assistant_text") => {
                                let t = event["data"]["text"].as_str()
                                    .or_else(|| event["content"].as_str())
                                    .unwrap_or("");
                                if !t.is_empty() {
                                    output.on_assistant_text(t);
                                    streaming_answer = t.to_string();
                                }
                            }
                            Some("thinking") | Some("thought") => {
                                let thought = event["data"]["thought"].as_str()
                                    .or_else(|| event["data"]["content"].as_str())
                                    .or_else(|| event["content"].as_str())
                                    .unwrap_or("");
                                if !thought.is_empty() {
                                    output.on_warning(&format!("[思考] {}", &thought[..thought.len().min(120)]));
                                }
                            }
                            Some("tool_use") => {
                                let name = event["data"]["name"].as_str()
                                    .or_else(|| event["data"]["tool"].as_str())
                                    .unwrap_or("unknown");
                                let input = &event["data"]["input"];
                                output.on_tool_use(
                                    &format!("↳ {}", name),
                                    input,
                                    "",
                                );
                                sr.tool_calls.push(name.to_string());
                            }
                            Some("tool_result") => {
                                let name = event["data"]["name"].as_str()
                                    .or_else(|| event["data"]["tool"].as_str())
                                    .unwrap_or("unknown");
                                let tool_out = event["data"]["output"].as_str()
                                    .or_else(|| event["data"]["result"].as_str())
                                    .unwrap_or("");
                                let summary: String = tool_out.chars().take(200).collect();
                                output.on_warning(&format!("  ↳ {} → {}", name, summary));
                            }
                            Some("confirm_request") => {
                                let action = event["data"]["action"].as_str().unwrap_or("");
                                let details = event["data"]["details"].as_str().map(|s| s.to_string());
                                let approved = if auto_approve {
                                    output.on_warning(&format!(
                                        "  ⚡ 自动批准: {}", action
                                    ));
                                    true
                                } else {
                                    use crate::confirm::{ConfirmAction, confirm as do_confirm};
                                    let ca = match action {
                                        "write_file" => ConfirmAction::WriteFile {
                                            path: details.unwrap_or_default(), lines: 0,
                                        },
                                        "edit_file" => ConfirmAction::EditFile {
                                            path: details.unwrap_or_default(),
                                        },
                                        "delete_file" => ConfirmAction::DeleteFile {
                                            path: details.unwrap_or_default(),
                                        },
                                        "run_command" => ConfirmAction::RunCommand {
                                            command: details.unwrap_or_default(),
                                        },
                                        _ => ConfirmAction::RunCommand {
                                            command: format!("{} {}", action, details.as_deref().unwrap_or("")),
                                        },
                                    };
                                    matches!(do_confirm(&ca), crate::confirm::ConfirmResult::Yes | crate::confirm::ConfirmResult::AlwaysYes)
                                };
                                let response = json!({
                                    "type": "confirm_response",
                                    "data": { "approved": approved }
                                });
                                let _ = write.send(Message::Text(response.to_string().into())).await;
                            }
                            Some("ask_user") => {
                                let question = event["data"]["question"].as_str().unwrap_or("").to_string();
                                let out = output.clone();
                                let answer = tokio::task::spawn_blocking(move || {
                                    out.ask_user(&question)
                                }).await.unwrap_or_default();
                                let response = json!({
                                    "type": "ask_user_response",
                                    "data": { "answer": answer }
                                });
                                let _ = write.send(Message::Text(response.to_string().into())).await;
                            }
                            Some("done") => {
                                let text = event["data"]["text"].as_str().unwrap_or("");
                                let final_text = if !text.is_empty() { text.to_string() } else { streaming_answer };
                                let _ = write.send(Message::Close(None)).await;

                                sr.status = "success".into();
                                sr.output_text = Some(final_text.clone());
                                sr.output_summary = Some(final_text.chars().take(300).collect());
                                sr.tokens_used = token_count;
                                sr.finished_at = Some(chrono::Utc::now().to_rfc3339());
                                output.on_warning(&format!("  ✅ 阶段完成 — {} tokens", token_count));
                                return Ok(sr);
                            }
                            Some("error") => {
                                let msg = event["data"]["message"].as_str().unwrap_or("unknown error");
                                let _ = write.send(Message::Close(None)).await;
                                sr.status = "failed".into();
                                sr.error_message = Some(msg.to_string());
                                sr.finished_at = Some(chrono::Utc::now().to_rfc3339());
                                return Err(anyhow::anyhow!("远程错误: {}", msg));
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        // Connection dropped
                        if !streaming_answer.is_empty() {
                            let summary: String = streaming_answer.chars().take(300).collect();
                            sr.status = "success".into();
                            sr.output_text = Some(streaming_answer);
                            sr.output_summary = Some(summary);
                            sr.finished_at = Some(chrono::Utc::now().to_rfc3339());
                            return Ok(sr);
                        }
                        return Err(anyhow::anyhow!("连接意外关闭"));
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("WebSocket 错误: {}", e));
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(PING_INTERVAL) => {
                let elapsed = start_time.elapsed();
                let idle = last_msg_at.elapsed();

                if idle >= total_timeout {
                    let _ = write.send(Message::Close(None)).await;
                    return Err(anyhow::anyhow!(
                        "阶段超时: {}秒无响应 (总耗时 {}s)",
                        idle.as_secs(), elapsed.as_secs()
                    ));
                }

                if idle >= INACTIVITY_ABORT {
                    let _ = write.send(Message::Close(None)).await;
                    return Err(anyhow::anyhow!(
                        "阶段无活动: {}秒无消息",
                        idle.as_secs()
                    ));
                }
            }
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve a stage's connection target.
///
/// New stages carry `server_url` (and optionally `workdir`, `model`, `agent_mode`)
/// directly.  For backward compatibility with workflows saved before migration 005,
/// if `server_url` is empty we fall back to the `preset_id` → `presets` table lookup.
fn resolve_stage_target(
    db: &GlobalDb,
    stage: &WorkflowStage,
) -> Result<(String, Option<String>, Option<String>, String)> {
    // New path: stage carries its own connection info
    if !stage.server_url.is_empty() {
        return Ok((
            stage.server_url.clone(),
            stage.workdir.clone(),
            stage.model.clone(),
            stage.agent_mode.clone(),
        ));
    }

    // Backward compat: look up preset
    if let Some(ref pid) = stage.preset_id {
        if let Some(p) = db.get_preset(pid)? {
            return Ok((
                p.server_url.clone(),
                p.workdir.clone(),
                p.model.clone(),
                p.agent_mode.clone(),
            ));
        }
        // Preset not found — return empty so caller can skip
        return Ok((String::new(), None, None, "auto".into()));
    }

    // No target at all
    Ok((String::new(), None, None, "auto".into()))
}

/// Evaluate a stage condition against the current pipeline status.
fn evaluate_condition(condition: &str, current_status: &str) -> bool {
    match condition.trim().to_lowercase().as_str() {
        "always" => true,
        "on_success" => current_status == "success",
        "on_failure" => current_status == "failed",
        "" | "true" => true,
        "false" => false,
        _ => true, // unknown → proceed
    }
}

/// Replace template variables in the input string.
///
/// Supported: `{{task}}`, `{{stage.<key>.output}}`, `{{stage.<key>.summary}}`
fn resolve_template(
    template: &str,
    task: &str,
    outputs: &HashMap<String, String>,
    summaries: &HashMap<String, String>,
) -> String {
    let mut result = template.to_string();

    // {{task}}
    result = result.replace("{{task}}", task);

    // {{stage.<key>.output}} and {{stage.<key>.summary}}
    // Simple regex-free approach: find {{stage.XXX.output}} patterns
    for (key, value) in outputs {
        let pattern_output = format!("{{{{stage.{}.output}}}}", key);
        result = result.replace(&pattern_output, value);

        let pattern_output_alt = format!("{{{{stage.{}.response}}}}", key);
        result = result.replace(&pattern_output_alt, value);
    }

    for (key, value) in summaries {
        let pattern_summary = format!("{{{{stage.{}.summary}}}}", key);
        result = result.replace(&pattern_summary, value);
    }

    result
}

fn skipped_stage_result(run_id: &str, stage: &WorkflowStage) -> StageResult {
    StageResult {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        stage_id: stage.id.clone(),
        stage_order: stage.stage_order,
        preset_name: None,
        status: "skipped".into(),
        ..Default::default()
    }
}

fn failed_stage_result(run_id: &str, stage: &WorkflowStage, error: &str) -> StageResult {
    StageResult {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        stage_id: stage.id.clone(),
        stage_order: stage.stage_order,
        preset_name: None,
        status: "failed".into(),
        error_message: Some(error.to_string()),
        finished_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    }
}
