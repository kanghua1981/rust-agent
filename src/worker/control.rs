use super::*;

pub(super) async fn handle_control_cmd(
    ctrl: ControlCmd,
    agent: &mut Agent,
    ws_output: &Arc<WsOutput>,
) {
    match ctrl {
        ControlCmd::SetModel(alias) => {
            if let Some(resolved) = agent.models_cfg.resolve(&alias) {
                let endpoint_name = agent.models_cfg.models.get(&alias)
                    .and_then(|e| e.endpoint.clone());
                agent.switch_model(&resolved);
                ws_output.emit_public("model_changed", serde_json::json!({
                    "alias": alias,
                    "model": resolved.model,
                    "provider": resolved.provider.to_string(),
                    "endpoint": endpoint_name,
                }));
            } else {
                ws_output.emit_public("warning", serde_json::json!({
                    "message": format!("Unknown model alias: '{}'", alias)
                }));
            }
        }

        ControlCmd::LoadSession => {
            let name = agent.session_id().map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
            match crate::persistence::load_local_named_session(&name, &agent.project_dir) {
                Ok(Some(session)) => {
                    let history = messages_to_json(&session.messages);
                    agent.conversation = crate::persistence::restore_conversation(&session);
                    ws_output.emit_public("session_restored", serde_json::json!({
                        "message_count": history.len(),
                        "messages": history,
                        "session_name": name,
                    }));
                }
                Ok(None) => {
                    ws_output.emit_public("warning", serde_json::json!({
                        "message": format!("Session '{}' has no saved data", name),
                    }));
                }
                Err(e) => {
                    ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to load session: {:#}", e),
                    }));
                }
            }
        }

        ControlCmd::NewSession => {
            let name = agent.session_id().map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
            agent.reset_conversation();
            if let Err(e) = crate::persistence::save_local_named_session(
                &name, &agent.conversation, &agent.project_dir,
            ) { tracing::warn!("save_local_named_session on new_session: {}", e); }
            if let Err(e) = crate::persistence::save_session_for_workdir(
                &agent.conversation, &agent.project_dir,
            ) { tracing::warn!("save_session_for_workdir on new_session: {}", e); }
            ws_output.emit_public("session_cleared", serde_json::json!({ "message": "New session started", "session_name": &name }));
            ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
        }

        ControlCmd::LoadSessionById(id) => {
            match crate::persistence::load_session(&id) {
                Ok(session) => {
                    let new_dir = std::path::PathBuf::from(&session.meta.working_dir);
                    if new_dir.is_dir() {
                        agent.set_project_dir(new_dir.clone());
                        if !agent.sandbox.is_disabled {
                            agent.set_allowed_dir(Some(new_dir));
                        }
                        ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                    }
                    let history = messages_to_json(&session.messages);
                    agent.conversation = crate::persistence::restore_conversation(&session);
                    ws_output.emit_public("session_restored", serde_json::json!({
                        "message_count": history.len(),
                        "messages": history,
                    }));
                }
                Err(e) => {
                    ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to load session: {:#}", e),
                    }));
                }
            }
        }

        // ── Local named session commands ──────────────────────────────

        ControlCmd::ListLocalSessions => {
            match crate::persistence::list_local_sessions(&agent.project_dir) {
                Ok(sessions) => {
                    let active = agent.session_id().unwrap_or("default");
                    let list: Vec<_> = sessions.iter().map(|s| serde_json::json!({
                        "id": s.id,
                        "session_name": s.session_name,
                        "summary": s.summary,
                        "updated_at": s.updated_at,
                        "message_count": s.message_count,
                        "working_dir": s.working_dir,
                    })).collect();
                    ws_output.emit_public("local_sessions_list", serde_json::json!({
                        "sessions": list,
                        "active": active,
                    }));
                }
                Err(e) => ws_output.emit_public("error", serde_json::json!({
                    "message": format!("list_local_sessions failed: {:#}", e)
                })),
            }
        }

        ControlCmd::SwitchLocalSession(name) => {
            // Save current session first
            let old_name = agent.session_id().map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
            if let Err(e) = crate::persistence::save_local_named_session(
                &old_name, &agent.conversation, &agent.project_dir,
            ) { tracing::warn!("save before switch failed: {}", e); }

            match crate::persistence::load_local_named_session(&name, &agent.project_dir) {
                Ok(Some(session)) => {
                    let history = messages_to_json(&session.messages);
                    agent.conversation = crate::persistence::restore_conversation(&session);
                    agent.set_session_id(name.clone());
                    let _ = crate::persistence::write_active_session_name(&agent.project_dir, &name);
                    ws_output.emit_public("session_switched", serde_json::json!({
                        "name": name,
                        "message_count": history.len(),
                        "messages": history,
                    }));
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }
                Ok(None) => {
                    // Create new session
                    agent.reset_conversation();
                    agent.set_session_id(name.clone());
                    let _ = crate::persistence::save_local_named_session(
                        &name, &agent.conversation, &agent.project_dir,
                    );
                    let _ = crate::persistence::write_active_session_name(&agent.project_dir, &name);
                    ws_output.emit_public("session_switched", serde_json::json!({
                        "name": name,
                        "message_count": 0,
                        "messages": [],
                    }));
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }
                Err(e) => ws_output.emit_public("error", serde_json::json!({
                    "message": format!("switch_local_session failed: {:#}", e)
                })),
            }
        }

        ControlCmd::NewLocalNamedSession(name) => {
            // Save current first
            let old_name = agent.session_id().map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
            let _ = crate::persistence::save_local_named_session(
                &old_name, &agent.conversation, &agent.project_dir,
            );
            agent.reset_conversation();
            agent.set_session_id(name.clone());
            if let Err(e) = crate::persistence::save_local_named_session(
                &name, &agent.conversation, &agent.project_dir,
            ) {
                ws_output.emit_public("error", serde_json::json!({
                    "message": format!("new_local_session failed: {:#}", e)
                }));
            } else {
                let _ = crate::persistence::write_active_session_name(&agent.project_dir, &name);
                ws_output.emit_public("session_switched", serde_json::json!({
                    "name": name,
                    "message_count": 0,
                    "messages": [],
                }));
                ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
            }
        }

        ControlCmd::DeleteLocalNamedSession(name) => {
            let active = agent.session_id().map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
            match crate::persistence::delete_local_named_session(&name, &agent.project_dir) {
                Ok(()) => {
                    // If deleted the active session, reset to default
                    if name == active {
                        agent.reset_conversation();
                        agent.set_session_id("default".to_string());
                        let _ = crate::persistence::write_active_session_name(&agent.project_dir, "default");
                    }
                    ws_output.emit_public("session_deleted", serde_json::json!({
                        "name": name,
                    }));
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }
                Err(e) => ws_output.emit_public("error", serde_json::json!({
                    "message": format!("delete_local_session failed: {:#}", e)
                })),
            }
        }

        ControlCmd::RenameLocalSession { old, new } => {
            match crate::persistence::rename_local_named_session(&old, &new, &agent.project_dir) {
                Ok(()) => {
                    if agent.session_id() == Some(&old) {
                        agent.set_session_id(new.clone());
                        let _ = crate::persistence::write_active_session_name(&agent.project_dir, &new);
                    }
                    ws_output.emit_public("session_renamed", serde_json::json!({
                        "old_name": old,
                        "new_name": new,
                    }));
                    ws_output.emit_public("session_info", session_info_json(&agent.project_dir));
                }
                Err(e) => ws_output.emit_public("error", serde_json::json!({
                    "message": format!("rename_local_session failed: {:#}", e)
                })),
            }
        }

        ControlCmd::SetSandbox(enabled) => {
            // In container mode, sandbox is wired at startup via kernel overlay.
            // Dynamic toggle via set_sandbox message is only effective in CLI mode.
            // In container mode, if we already have the correct state, just report it.
            let already_correct = (enabled && !agent.sandbox.is_disabled)
                || (!enabled && agent.sandbox.is_disabled);
            if !already_correct {
                agent.set_sandbox_enabled(enabled);
            }
            let actual_enabled = !agent.sandbox.is_disabled;
            if enabled && !actual_enabled {
                ws_output.emit_public("warning", serde_json::json!({
                    "message": "沙盒模式需要在连接时通过 URL 参数 sandbox=1 启用（容器需要在启动前挂载 overlay）。请断开重连并在连接面板中勾选沙盒选项。"
                }));
            }
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": actual_enabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": agent.sandbox.ops_count().await,
            }));
        }

        ControlCmd::SetPlanMode(on) => {
            agent.set_plan_mode(on);
        }

        ControlCmd::SandboxListChanges => {
            let changes = agent.sandbox.changed_files().await;
            let files: Vec<serde_json::Value> = changes.iter().map(|c| c.to_json()).collect();
            ws_output.emit_public("sandbox_changes_result", serde_json::json!({
                "files": files,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": changes.len(),
            }));
        }

        ControlCmd::SandboxCommit => {
            let result = agent.sandbox.commit().await;
            ws_output.emit_public("sandbox_commit_result", serde_json::json!({
                "modified": result.modified,
                "created": result.created,
            }));
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": 0,
            }));
        }
        
        ControlCmd::SandboxCommitFile(file_path) => {
            let result = agent.sandbox.commit_file(&file_path).await;
            ws_output.emit_public("sandbox_commit_file_result", serde_json::json!({
                "file_path": file_path,
                "modified": result.modified,
                "created": result.created,
            }));
            // Update pending changes count
            let changes = agent.sandbox.changed_files().await;
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": changes.len(),
            }));
        }

        ControlCmd::SandboxRollback => {
            let result = agent.sandbox.rollback().await;
            ws_output.emit_public("sandbox_rollback_result", serde_json::json!({
                "restored": result.restored,
                "deleted": result.deleted,
                "errors": result.errors,
            }));
            ws_output.emit_public("sandbox_status", serde_json::json!({
                "enabled": !agent.sandbox.is_disabled,
                "backend": agent.sandbox.backend_label_sync(),
                "pending_changes": 0,
            }));
        }

        ControlCmd::LoadMcp(entries) => {
            let (loaded, errors) = agent.load_mcp_from_entries(&entries).await;
            ws_output.emit_public("mcp_loaded", serde_json::json!({
                "tools": loaded,
                "errors": errors,
            }));
        }

        ControlCmd::UnloadMcp(prefix) => {
            let removed = agent.unload_mcp(&prefix);
            ws_output.emit_public("mcp_unloaded", serde_json::json!({
                "prefix": prefix,
                "removed": removed,
            }));
        }

        ControlCmd::ListMcpTools => {
            let tools: Vec<serde_json::Value> = agent
                .list_mcp_tools()
                .into_iter()
                .map(|(name, description)| serde_json::json!({ "name": name, "description": description }))
                .collect();
            ws_output.emit_public("mcp_tools_list", serde_json::json!({
                "tools": tools,
            }));
        }

        ControlCmd::ListPlugins => {
            if let Some(pm) = &agent.plugin_manager {
                let lock = pm.lock().await;
                let plugins = lock.list_plugins();
                let list: Vec<_> = plugins.iter().map(|p| serde_json::json!({
                    "id":          p.id,
                    "name":        p.name,
                    "version":     p.version,
                    "description": p.description,
                    "enabled":     p.enabled,
                    "tools":       p.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
                })).collect();
                ws_output.emit_public("plugins_list", serde_json::json!({ "plugins": list }));
            } else {
                ws_output.emit_public("plugins_list", serde_json::json!({ "plugins": [] }));
            }
        }

        ControlCmd::EnablePlugin(id) => {
            if let Some(pm) = &agent.plugin_manager {
                let result = { pm.lock().await.enable_plugin(&id) };
                match result {
                    Ok(()) => {
                        // 刷新 LLM 可见工具列表（仅内存，不回写磁盘）
                        let _ = agent.load_plugin_tools().await;
                        ws_output.emit_public("plugin_status_changed", serde_json::json!({
                            "id":     id,
                            "action": "enabled",
                            "note":   "session-only, not persisted to disk",
                        }));
                    }
                    Err(e) => {
                        ws_output.emit_public("error", serde_json::json!({
                            "message": format!("enable_plugin '{}' failed: {}", id, e)
                        }));
                    }
                }
            } else {
                ws_output.emit_public("error", serde_json::json!({ "message": "Plugin system not available" }));
            }
        }

        ControlCmd::DisablePlugin(id) => {
            if let Some(pm) = &agent.plugin_manager {
                let result = { pm.lock().await.disable_plugin(&id) };
                match result {
                    Ok(()) => {
                        // 已禁用插件的工具将从 LLM 可见列表消失
                        let _ = agent.load_plugin_tools().await;
                        ws_output.emit_public("plugin_status_changed", serde_json::json!({
                            "id":     id,
                            "action": "disabled",
                            "note":   "session-only, not persisted to disk",
                        }));
                    }
                    Err(e) => {
                        ws_output.emit_public("error", serde_json::json!({
                            "message": format!("disable_plugin '{}' failed: {}", id, e)
                        }));
                    }
                }
            } else {
                ws_output.emit_public("error", serde_json::json!({ "message": "Plugin system not available" }));
            }
        }

        // ── Model & endpoint management ────────────────────────────────

        ControlCmd::ListEndpoints => {
            let cfg = crate::model_manager::load();
            let eps: Vec<serde_json::Value> = cfg.endpoints.iter().map(|(name, ep)| {
                serde_json::json!({
                    "name": name,
                    "provider": ep.provider,
                    "base_url": ep.base_url,
                    "has_api_key": ep.api_key.is_some(),
                })
            }).collect();
            ws_output.emit_public("endpoints_list", serde_json::json!({ "endpoints": eps }));
        }

        ControlCmd::FetchModels { url, api_key } => {
            match crate::model_manager::fetch_models(&url, api_key.as_deref()).await {
                Ok(fetched) => {
                    ws_output.emit_public("models_fetched", serde_json::json!({
                        "models": fetched.models,
                        "source": fetched.source,
                        "url": url,
                    }));
                }
                Err(e) => {
                    ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to fetch models: {:#}", e),
                    }));
                }
            }
        }

        ControlCmd::AddModel { alias, model, endpoint_name } => {
            let mut cfg = crate::model_manager::load();
            if !cfg.endpoints.contains_key(&endpoint_name) {
                ws_output.emit_public("error", serde_json::json!({
                    "message": format!("Endpoint '{}' not found", endpoint_name),
                }));
            } else {
                cfg.models.insert(alias.clone(), crate::model_manager::ModelEntry {
                    provider: String::new(),
                    model: model.clone(),
                    endpoint: Some(endpoint_name.clone()),
                    base_url: None,
                    api_key: None,
                    max_tokens: None,
                    thinking_enabled: None,
                    reasoning_effort: None,
                    temperature: None,
                });
                match crate::model_manager::save(&cfg) {
                    Ok(()) => {
                        agent.models_cfg = cfg;
                        emit_model_state(ws_output, &agent.models_cfg);
                        ws_output.emit_public("model_added", serde_json::json!({
                            "alias": alias, "model": model, "endpoint": endpoint_name,
                        }));
                    }
                    Err(e) => ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to save: {:#}", e),
                    })),
                }
            }
        }

        ControlCmd::DeleteModel(alias) => {
            let mut cfg = crate::model_manager::load();
            if cfg.remove(&alias) {
                // If the deleted model was active, clear the alias
                if agent.config.model_alias.as_deref() == Some(&alias) {
                    agent.config.model_alias = None;
                }
                match crate::model_manager::save(&cfg) {
                    Ok(()) => {
                        agent.models_cfg = cfg;
                        emit_model_state(ws_output, &agent.models_cfg);
                        ws_output.emit_public("model_deleted", serde_json::json!({ "alias": alias }));
                    }
                    Err(e) => ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to save: {:#}", e),
                    })),
                }
            } else {
                ws_output.emit_public("warning", serde_json::json!({
                    "message": format!("Model alias '{}' not found", alias),
                }));
            }
        }

        ControlCmd::AddEndpoint { name, provider, base_url, api_key } => {
            let mut cfg = crate::model_manager::load();
            cfg.endpoints.insert(name.clone(), crate::model_manager::EndpointEntry {
                provider,
                base_url: base_url.clone(),
                api_key,
            });
            match crate::model_manager::save(&cfg) {
                Ok(()) => {
                    agent.models_cfg = cfg;
                    emit_model_state(ws_output, &agent.models_cfg);
                    ws_output.emit_public("endpoint_added", serde_json::json!({
                        "name": name, "base_url": base_url,
                    }));
                }
                Err(e) => ws_output.emit_public("error", serde_json::json!({
                    "message": format!("Failed to save: {:#}", e),
                })),
            }
        }

        ControlCmd::DeleteEndpoint(name) => {
            let mut cfg = crate::model_manager::load();
            // Check for models referencing this endpoint
            let using: Vec<String> = cfg.models.iter()
                .filter(|(_, m)| m.endpoint.as_deref() == Some(&name))
                .map(|(a, _)| a.clone())
                .collect();
            if !using.is_empty() {
                ws_output.emit_public("error", serde_json::json!({
                    "message": format!("Endpoint '{}' is referenced by: {}. Delete those models first.", name, using.join(", ")),
                }));
            } else if cfg.endpoints.remove(&name).is_some() {
                match crate::model_manager::save(&cfg) {
                    Ok(()) => {
                        agent.models_cfg = cfg;
                        emit_model_state(ws_output, &agent.models_cfg);
                        ws_output.emit_public("endpoint_deleted", serde_json::json!({ "name": name }));
                    }
                    Err(e) => ws_output.emit_public("error", serde_json::json!({
                        "message": format!("Failed to save: {:#}", e),
                    })),
                }
            } else {
                ws_output.emit_public("warning", serde_json::json!({
                    "message": format!("Endpoint '{}' not found", name),
                }));
            }
        }

        ControlCmd::UploadFile(name, data, mime_type) => {
            match save_uploaded_file(&agent.project_dir, &name, &data, mime_type.as_deref()) {
                Ok((rel_path, size)) => {
                    ws_output.emit_public("upload_file_result", serde_json::json!({
                        "success": true,
                        "name": name,
                        "path": rel_path,
                        "size": size,
                    }));
                }
                Err(e) => {
                    ws_output.emit_public("upload_file_result", serde_json::json!({
                        "success": false,
                        "name": name,
                        "error": e,
                    }));
                }
            }
        }
    }
}

