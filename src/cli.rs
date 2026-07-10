//! CLI interaction loop with session management.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::agent::Agent;
use crate::config::Config;
use crate::confirm;
use crate::output::AgentOutput;
use crate::persistence;
use crate::ui;

/// Handle `/upload <source_path> [target_name]` command.
/// Reads a local file and copies it to the project's uploads/ directory.
async fn handle_upload_command(args: &str, agent: &Agent) {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() || parts[0].is_empty() {
        println!("\n📎  Usage: /upload <source_path> [target_name]");
        println!("   Copies a local file into the agent's uploads/ directory.");
        println!("   The agent can then access it via read_file and other tools.");
        return;
    }

    let source = std::path::PathBuf::from(parts[0]);
    let target_name = if parts.len() > 1 {
        parts[1].to_string()
    } else {
        source.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("uploaded_file")
            .to_string()
    };

    // ── Security checks ──────────────────────────────────────────────
    // Reject path traversal in target name
    if target_name.contains('/') || target_name.contains('\\') || target_name.contains("..") {
        println!("\n❌  Invalid target name '{}': must be a plain file name (no path separators).", target_name);
        return;
    }

    // Read source file
    let data = match std::fs::read(&source) {
        Ok(d) => d,
        Err(e) => {
            println!("\n❌  Failed to read '{}': {}", source.display(), e);
            return;
        }
    };

    // Size check (50 MB)
    if data.len() > 50 * 1024 * 1024 {
        println!(
            "\n❌  File too large: {} bytes (max 50 MB)",
            data.len()
        );
        return;
    }

    // Write to uploads/ directory
    let uploads_dir = agent.project_dir.join("uploads");
    if let Err(e) = std::fs::create_dir_all(&uploads_dir) {
        println!("\n❌  Failed to create uploads directory: {}", e);
        return;
    }

    let dest_path = if uploads_dir.join(&target_name).exists() {
        // Add timestamp suffix for dedup
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let (stem, ext) = target_name
            .rfind('.')
            .map(|i| (&target_name[..i], &target_name[i..]))
            .unwrap_or((target_name.as_str(), ""));
        uploads_dir.join(format!("{}_{}{}", stem, ts, ext))
    } else {
        uploads_dir.join(&target_name)
    };

    match std::fs::write(&dest_path, &data) {
        Ok(()) => {
            let rel_path = dest_path
                .strip_prefix(&agent.project_dir)
                .unwrap_or(&dest_path)
                .to_string_lossy();
            let size_str = if data.len() < 1024 {
                format!("{} B", data.len())
            } else if data.len() < 1024 * 1024 {
                format!("{:.1} KB", data.len() as f64 / 1024.0)
            } else {
                format!("{:.1} MB", data.len() as f64 / (1024.0 * 1024.0))
            };
            println!("\n✅  Uploaded: {} → {} ({})", source.display(), rel_path, size_str);
            println!("   The agent can now access this file with read_file and other tools.");
        }
        Err(e) => {
            println!("\n❌  Failed to write '{}': {}", dest_path.display(), e);
        }
    }
}

/// Handle `/plugin` command — list, enable, disable, info, tools.
///
/// - `/plugin`                — list all plugins
/// - `/plugin list`           — list all plugins
/// - `/plugin enable <name>`  — enable a plugin
/// - `/plugin disable <name>` — disable a plugin
/// - `/plugin info <name>`    — show plugin information
/// - `/plugin tools`          — list plugin tools
pub async fn handle_plugin_command(subcommand: &str, agent: &mut Agent) {
    let parts: Vec<&str> = subcommand.split_whitespace().collect();
    
    match parts.as_slice() {
        [] | ["list"] => {
            if let Some(pm) = &agent.plugin_manager {
                let pm_lock = pm.lock().await;
                let plugins = pm_lock.list_plugins();
                if plugins.is_empty() {
                    println!("\n🔌  No plugins loaded.");
                } else {
                    println!("\n🔌  {} plugin(s) loaded:", plugins.len());
                    for plugin in plugins {
                        let status = if plugin.enabled { "enabled" } else { "disabled" };
                        println!("  • {} ({}) [{}]", plugin.name, plugin.id, status);
                    }
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
                println!("  Start the agent with --enable-plugins to enable plugins.");
            }
        }
        ["enable", name] => {
            if let Some(pm) = &agent.plugin_manager {
                let mut pm_lock = pm.lock().await;
                match pm_lock.enable_plugin(name) {
                    Ok(()) => println!("\n✅  Plugin '{}' enabled.", name),
                    Err(e) => println!("\n❌  Failed to enable plugin '{}': {}", name, e),
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        ["disable", name] => {
            if let Some(pm) = &agent.plugin_manager {
                let mut pm_lock = pm.lock().await;
                match pm_lock.disable_plugin(name) {
                    Ok(()) => println!("\n✅  Plugin '{}' disabled.", name),
                    Err(e) => println!("\n❌  Failed to disable plugin '{}': {}", name, e),
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        ["info", name] => {
            if let Some(pm) = &agent.plugin_manager {
                let pm_lock = pm.lock().await;
                match pm_lock.get_plugin_info(name) {
                    Some(info) => {
                        println!("\n🔌  Plugin: {}", info.name);
                        println!("  ID: {}", info.id);
                        println!("  Version: {}", info.version);
                        println!("  Description: {}", info.description);
                        println!("  Author: {}", info.author);
                        println!("  Status: {}", if info.enabled { "enabled" } else { "disabled" });
                        println!("  Tools: {}", info.tools.len());
                        for tool in &info.tools {
                            println!("    • {} - {}", tool.name, tool.description);
                        }
                    }
                    None => println!("\n❌  Plugin '{}' not found.", name),
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        ["tools"] => {
            if let Some(pm) = &agent.plugin_manager {
                let pm_lock = pm.lock().await;
                let tools = pm_lock.get_all_tools();
                if tools.is_empty() {
                    println!("\n🔧  No plugin tools available.");
                } else {
                    println!("\n🔧  {} plugin tool(s) available:", tools.len());
                    for tool in tools {
                        println!("  • {} ({}) - {}", tool.name, tool.plugin_id, tool.description);
                    }
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        ["skills"] | ["skills", ""] => {
            if let Some(pm) = &agent.plugin_manager {
                let pm_lock = pm.lock().await;
                let skills = pm_lock.get_all_skills();
                if skills.is_empty() {
                    println!("\n📚  No plugin skills available.");
                } else {
                    println!("\n📚  {} plugin skill(s):", skills.len());
                    for skill in &skills {
                        let tags = if skill.tags.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", skill.tags.join(", "))
                        };
                        println!("  • {} (plugin: {}){}", skill.name, skill.plugin_id, tags);
                        if !skill.description.is_empty() {
                            println!("    {}", skill.description);
                        }
                    }
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        ["skills", query] => {
            if let Some(pm) = &agent.plugin_manager {
                let pm_lock = pm.lock().await;
                let skills = pm_lock.search_skills(query);
                if skills.is_empty() {
                    println!("\n📚  No plugin skills matching '{}'.", query);
                } else {
                    println!("\n📚  {} skill(s) matching '{}':", skills.len(), query);
                    for skill in &skills {
                        println!("  • {} (plugin: {}) — {}", skill.name, skill.plugin_id, skill.description);
                    }
                }
            } else {
                println!("\n🔌  Plugin system is not enabled.");
            }
        }
        _ => {
            println!("\n🔌  Plugin command usage:");
            println!("  /plugin list            - list all plugins");
            println!("  /plugin enable <name>   - enable a plugin");
            println!("  /plugin disable <name>  - disable a plugin");
            println!("  /plugin info <name>     - show plugin information");
            println!("  /plugin tools           - list plugin tools");
            println!("  /plugin skills          - list all plugin skills");
            println!("  /plugin skills <query>  - search plugin skills");
        }
    }
}

/// List saved sessions and exit
pub fn list_sessions_and_exit() -> Result<()> {
    let sessions = persistence::list_sessions()?;
    if sessions.is_empty() {
        println!("No saved sessions found.");
    } else {
        println!("\n{}", "📜 Saved Sessions:".bright_cyan().bold());
        println!(
            "  {:<10} {:<24} {:<6} {}",
            "ID".bright_white().bold(),
            "Updated".bright_white().bold(),
            "Msgs".bright_white().bold(),
            "Summary".bright_white().bold()
        );
        println!("  {}", "─".repeat(70).dimmed());
        for s in &sessions {
            println!(
                "  {:<10} {:<24} {:<6} {}",
                s.id.bright_yellow(),
                s.updated_at.dimmed(),
                s.message_count.to_string().bright_white(),
                s.summary
            );
        }
        println!();
        println!(
            "  Resume with: {} {}",
            "agent --resume".bright_green(),
            "<ID>".dimmed()
        );
    }
    Ok(())
}

enum SlashResult {
    Continue,
    Quit,
    NotACommand,
}

fn handle_slash_command(input: &str, agent: &mut Agent) -> SlashResult {
    match input {
        "/quit" | "/exit" | "/q" => {
            // Sandbox cleanup is handled by the caller after the REPL exits
            auto_save_session(agent);
            println!("\n{}", "👋 Goodbye! Happy coding!".bright_green());
            SlashResult::Quit
        }
        "/help" | "/h" => {
            ui::print_help();
            SlashResult::Continue
        }
        "/clear" => {
            agent.reset();
            println!("\n{}", "🔄 Conversation cleared.".bright_cyan());
            SlashResult::Continue
        }
        "/usage" => {
            let (input_tokens, output_tokens) = agent.token_usage();
            ui::print_usage(input_tokens, output_tokens, agent.role_token_usage());
            SlashResult::Continue
        }
        "/save" => {
            if agent.global_session {
                match persistence::save_session(&agent.conversation, agent.session_id(), &agent.project_dir) {
                    Ok(id) => {
                        agent.set_session_id(id.clone());
                        println!("\n{}  Session saved (global): {}", "💾", id.bright_yellow());
                    }
                    Err(e) => ui::print_error(&format!("Failed to save session: {}", e)),
                }
            } else {
                let name = agent.session_id().unwrap_or("default");
                match persistence::save_local_named_session(name, &agent.conversation, &agent.project_dir) {
                    Ok(()) => {
                        let _ = persistence::write_active_session_name(&agent.project_dir, name);
                        println!("\n{}  Session '{}' saved to {}", "💾", name.bright_yellow(), ".agent/sessions/".bright_yellow());
                    }
                    Err(e) => ui::print_error(&format!("Failed to save session: {}", e)),
                }
            }
            SlashResult::Continue
        }
        "/sessions" => {
            if let Err(e) = list_sessions_and_exit() {
                ui::print_error(&format!("Failed to list sessions: {}", e));
            }
            SlashResult::Continue
        }
        "/yesall" => {
            confirm::set_auto_approve(true);
            println!(
                "\n{}  {}",
                "✅",
                "Auto-approve enabled. All operations will proceed without confirmation."
                    .bright_green()
            );
            SlashResult::Continue
        }
        "/confirm" => {
            confirm::set_auto_approve(false);
            println!(
                "\n{}  {}",
                "🔒",
                "Confirmations re-enabled. Dangerous operations will require approval."
                    .bright_cyan()
            );
            SlashResult::Continue
        }
        "/context" => {
            let status =
                crate::context::check_context(&agent.conversation, &agent.config.model);
            ui::print_context_status(
                status.estimated_tokens,
                status.max_tokens,
                status.usage_percent,
                agent.conversation.messages.len(),
            );
            SlashResult::Continue
        }
        _ if input == "/model" || input.starts_with("/model ") => {
            handle_model_command(input, agent);
            SlashResult::Continue
        }
        "/skills" => {
            {
                let loaded = crate::skills::load_skills(&agent.project_dir);
                if loaded.is_empty() {
                    println!(
                        "\n{}  No skills found. Create {} or add Markdown files to {}",
                        "📋",
                        "AGENT.md".bright_yellow(),
                        ".agent/skills/".bright_yellow()
                    );
                } else {
                    println!("\n{}  {} skill(s) loaded:", "📋", loaded.len());
                    for skill in &loaded.skills {
                        println!(
                            "  {} {} {} {}",
                            "•".bright_cyan(),
                            skill.name.bright_white(),
                            format!("({})", skill.source).dimmed(),
                            "[embedded]".green()
                        );
                    }
                    for entry in &loaded.index {
                        println!(
                            "  {} {} {} {}",
                            "•".bright_cyan(),
                            entry.name.bright_white(),
                            format!("({})", entry.source).dimmed(),
                            "[on-demand]".yellow()
                        );
                    }
                }
            }
            SlashResult::Continue
        }
        "/memory" => {
            let mem = agent.memory.as_ref();
            if mem.is_empty() {
                println!(
                    "\n{}  Memory is empty. It will grow as you use the agent.",
                    "🧠"
                );
            } else {
                println!("\n{}  Agent Memory ({} entries):", "🧠", mem.entry_count());
                let knowledge = mem.knowledge();
                if !knowledge.is_empty() {
                    println!("  {} {}:", "📖", "Project Knowledge".bright_cyan());
                    for fact in &knowledge {
                        println!("    {} {}", "•".dimmed(), fact);
                    }
                }
                let file_map = mem.file_map();
                if !file_map.is_empty() {
                    println!("  {} {}:", "📁", "Key Files".bright_cyan());
                    for (path, desc) in &file_map {
                        if desc.is_empty() {
                            println!("    {} {}", "•".dimmed(), path.bright_white());
                        } else {
                            println!(
                                "    {} {} {}",
                                "•".dimmed(),
                                path.bright_white(),
                                format!("({})", desc).dimmed()
                            );
                        }
                    }
                }
                let session_log = mem.session_log();
                if !session_log.is_empty() {
                    println!("  {} {}:", "📝", "Session Log".bright_cyan());
                    for entry in &session_log {
                        println!("    {} {}", "•".dimmed(), entry.dimmed());
                    }
                }
            }
            SlashResult::Continue
        }
        _ if input == "/mode" || input.starts_with("/mode ") => {
            handle_mode_command(input, agent);
            SlashResult::Continue
        }
        _ if input == "/endpoint" || input.starts_with("/endpoint ") => {
            handle_endpoint_command(input);
            SlashResult::Continue
        }
        _ if input == "/list" => {
            handle_list_sessions_command(agent);
            SlashResult::Continue
        }
        _ if input.starts_with("/new ") => {
            let name = input.strip_prefix("/new ").unwrap_or("").trim();
            handle_new_session_command(name, agent);
            SlashResult::Continue
        }
        _ if input.starts_with("/switch ") => {
            let name = input.strip_prefix("/switch ").unwrap_or("").trim();
            handle_switch_session_command(name, agent);
            SlashResult::Continue
        }
        _ if input.starts_with("/rename ") => {
            let args: Vec<&str> = input.strip_prefix("/rename ").unwrap_or("").split_whitespace().collect();
            if args.len() >= 2 {
                handle_rename_session_command(args[0], args[1], agent);
            } else {
                println!("\n{}  Usage: /rename <old_name> <new_name>", "❓");
            }
            SlashResult::Continue
        }
        _ => SlashResult::NotACommand,
    }
}

/// Auto-save the session (silent, won't error to user)
pub fn auto_save_session(agent: &mut Agent) {
    if agent.conversation.messages.is_empty() {
        return;
    }
    if agent.global_session {
        match persistence::save_session(&agent.conversation, agent.session_id(), &agent.project_dir) {
            Ok(id) => {
                agent.set_session_id(id);
            }
            Err(e) => {
                tracing::warn!("Auto-save (global) failed: {}", e);
            }
        }
    } else {
        let name = agent.session_id().unwrap_or("default");
        if let Err(e) = persistence::save_local_named_session(name, &agent.conversation, &agent.project_dir) {
            tracing::warn!("Auto-save (local '{}') failed: {}", name, e);
        }
        let _ = persistence::write_active_session_name(&agent.project_dir, name);
    }
}

// ── Multi-session slash command handlers ────────────────────────────────

/// `/list` — list local named sessions
fn handle_list_sessions_command(agent: &Agent) {
    match persistence::list_local_sessions(&agent.project_dir) {
        Ok(sessions) => {
            if sessions.is_empty() {
                println!("\n{}  No local sessions found.", "📋");
                return;
            }
            let active = agent.session_id().unwrap_or("default");
            println!("\n{}  Local sessions ({}):", "📋", sessions.len());
            for s in &sessions {
                let marker = if s.id == active { " *" } else { "" };
                let name = s.session_name.as_deref().unwrap_or(&s.id);
                println!(
                    "  {}{} {} {}  {} messages  {}",
                    "•".bright_cyan(),
                    marker.bright_green(),
                    name.bright_white(),
                    format!("({})", s.working_dir).dimmed(),
                    s.message_count.to_string().bright_white(),
                    s.updated_at.dimmed(),
                );
            }
            println!("  {} = active session", "*".bright_green());
        }
        Err(e) => ui::print_error(&format!("Failed to list sessions: {}", e)),
    }
}

/// `/new <name>` — create a new named session
fn handle_new_session_command(name: &str, agent: &mut Agent) {
    if name.is_empty() {
        println!("\n{}  Usage: /new <name>", "❓");
        return;
    }
    if name == "_active" {
        println!("\n{}  '_active' is a reserved name.", "❌");
        return;
    }

    // Save current session first
    auto_save_session(agent);

    // Create a fresh conversation (keeping system prompt)
    agent.conversation = crate::conversation::Conversation::new(&agent.project_dir);
    agent.set_session_id(name.to_string());

    // Persist empty session immediately
    if let Err(e) = persistence::save_local_named_session(name, &agent.conversation, &agent.project_dir) {
        ui::print_error(&format!("Failed to create session: {}", e));
        return;
    }
    let _ = persistence::write_active_session_name(&agent.project_dir, name);
    println!("\n{}  Created and switched to session '{}'", "✅", name.bright_yellow());
}

/// `/switch <name>` — switch to a named session
fn handle_switch_session_command(name: &str, agent: &mut Agent) {
    if name.is_empty() {
        println!("\n{}  Usage: /switch <name>", "❓");
        return;
    }

    // Save current session first
    auto_save_session(agent);

    match persistence::load_local_named_session(name, &agent.project_dir) {
        Ok(Some(session)) => {
            let msg_count = session.messages.len();
            agent.conversation = persistence::restore_conversation(&session);
            agent.set_session_id(name.to_string());
            let _ = persistence::write_active_session_name(&agent.project_dir, name);
            println!(
                "\n{}  Switched to session '{}' ({} messages)",
                "✅",
                name.bright_yellow(),
                msg_count.to_string().bright_white()
            );
        }
        Ok(None) => {
            // Session doesn't exist — create it
            agent.conversation = crate::conversation::Conversation::new(&agent.project_dir);
            agent.set_session_id(name.to_string());
            let _ = persistence::save_local_named_session(name, &agent.conversation, &agent.project_dir);
            let _ = persistence::write_active_session_name(&agent.project_dir, name);
            println!(
                "\n{}  Created and switched to new session '{}'",
                "✅",
                name.bright_yellow()
            );
        }
        Err(e) => ui::print_error(&format!("Failed to switch session: {}", e)),
    }
}

/// `/rename <old> <new>` — rename a local session
fn handle_rename_session_command(old_name: &str, new_name: &str, agent: &mut Agent) {
    match persistence::rename_local_named_session(old_name, new_name, &agent.project_dir) {
        Ok(()) => {
            // Update agent's session_id if we renamed the active session
            if agent.session_id() == Some(old_name) {
                agent.set_session_id(new_name.to_string());
                let _ = persistence::write_active_session_name(&agent.project_dir, new_name);
            }
            println!(
                "\n{}  Renamed session '{}' → '{}'",
                "✅",
                old_name.bright_yellow(),
                new_name.bright_yellow()
            );
        }
        Err(e) => ui::print_error(&format!("Failed to rename session: {}", e)),
    }
}



/// Handle `/mode [simple|plan|pipeline|auto]` command.
///
/// - `/mode`              — show current override (or "auto")
/// - `/mode simple`       — force BasicLoop for every message
/// - `/mode plan`         — force PlanAndExecute for every message
/// - `/mode pipeline`     — force FullPipeline for every message
/// - `/mode auto`         — clear override, let the router decide
fn handle_mode_command(input: &str, agent: &mut Agent) {
    use crate::router::ExecutionMode;

    let sub = input.strip_prefix("/mode").unwrap_or("").trim();

    match sub {
        "" => {
            let current = match agent.force_mode {
                Some(ExecutionMode::BasicLoop)     => "simple (forced)".to_string(),
                Some(ExecutionMode::PlanAndExecute) => "plan (forced)".to_string(),
                Some(ExecutionMode::FullPipeline)  => "pipeline (forced)".to_string(),
                None => "auto (router decides)".to_string(),
            };
            println!("\n{}  Current execution mode: {}", "🔀", current.bright_white());
            println!("  Use {} to change:", "/mode <option>".bright_cyan());
            println!("    {}      — single-model loop, fast & cheap", "simple".bright_yellow());
            println!("    {}        — planner + executor, no checker", "plan".bright_yellow());
            println!("    {}    — full planner → executor → checker", "pipeline".bright_yellow());
            println!("    {}        — let the router decide (default)", "auto".bright_yellow());
            println!();
        }
        "simple" => {
            agent.set_force_mode(Some(ExecutionMode::BasicLoop));
            println!("\n{}  Mode locked to {}: single-model loop for all messages.", "🔀", "simple".bright_green());
        }
        "plan" => {
            agent.set_force_mode(Some(ExecutionMode::PlanAndExecute));
            println!("\n{}  Mode locked to {}: planner + executor for all messages.", "🔀", "plan".bright_green());
        }
        "pipeline" => {
            agent.set_force_mode(Some(ExecutionMode::FullPipeline));
            println!("\n{}  Mode locked to {}: full pipeline for all messages.", "🔀", "pipeline".bright_green());
        }
        "auto" => {
            agent.set_force_mode(None);
            println!("\n{}  Mode reset to {}: adaptive router will classify each task.", "🔀", "auto".bright_green());
        }
        other => {
            println!(
                "\n{}  Unknown mode: {}. Valid options: simple, plan, pipeline, auto",
                "❓",
                other.bright_red()
            );
        }
    }
}

/// Handle `/model [subcommand]` — list, switch, remove, set default.
///
/// - `/model`              — show current model + list all configured aliases
/// - `/model <alias>`      — switch to the named alias
/// - `/model rm <alias>`   — remove an alias from models.toml
/// - `/model default <alias>` — set the default model alias
fn handle_model_command(input: &str, agent: &mut Agent) {
    let sub = input.strip_prefix("/model").unwrap_or("").trim();

    match sub {
        "" => {
            // Show current model
            let current_alias = agent.config.model_alias.as_deref().unwrap_or("<none>");
            let current_model = &agent.config.model;
            println!("\n{}  Current model: {} ({})",
                "🤖",
                current_alias.bright_green(),
                current_model.bright_white()
            );

            // List all configured aliases
            let cfg = crate::model_manager::load();
            if cfg.models.is_empty() {
                println!("  No extra models configured. Use {} to add one.",
                    "/model fetch <url>".bright_cyan());
                println!("  Or edit {}  manually.",
                    crate::model_manager::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "~/.config/rust_agent/models.toml".into())
                        .bright_yellow()
                );
            } else {
                let default = cfg.default.as_deref();
                println!("  {}", "Configured models:".bright_cyan());
                for alias in cfg.models.keys() {
                    let marker = if Some(alias.as_str()) == default {
                        " [default]".bright_yellow().to_string()
                    } else if Some(alias.as_str()) == agent.config.model_alias.as_deref() {
                        " [active]".bright_green().to_string()
                    } else {
                        String::new()
                    };
                    let entry = &cfg.models[alias];
                    println!("    {} — {}{}",
                        alias.bright_white(),
                        entry.model.dimmed(),
                        marker
                    );
                }
                println!();
                println!("  Switch:  {} <alias>", "/model".bright_cyan());
                println!("  Default: {} <alias>", "/model default".bright_cyan());
                println!("  Remove:  {} <alias>", "/model rm".bright_cyan());
                println!("  Fetch:   {} <url>", "/model fetch".bright_cyan());
            }
        }

        // /model rm <alias>
        sub if sub.starts_with("rm ") => {
            let alias = sub.strip_prefix("rm ").unwrap_or("").trim();
            if alias.is_empty() {
                println!("\n{}  Usage: /model rm <alias>", "⚠️");
                return;
            }
            let mut cfg = crate::model_manager::load();
            if cfg.remove(alias) {
                match crate::model_manager::save(&cfg) {
                    Ok(()) => println!("\n{}  Removed model alias '{}'.", "🗑️", alias.bright_yellow()),
                    Err(e) => println!("\n{}  Failed to save models.toml: {}", "❌", e),
                }
            } else {
                println!("\n{}  Alias '{}' not found.", "⚠️", alias);
            }
        }

        // /model default <alias>
        sub if sub.starts_with("default ") => {
            let alias = sub.strip_prefix("default ").unwrap_or("").trim();
            if alias.is_empty() {
                println!("\n{}  Usage: /model default <alias>", "⚠️");
                return;
            }
            let mut cfg = crate::model_manager::load();
            if cfg.models.contains_key(alias) {
                cfg.set_default(alias.to_string());
                match crate::model_manager::save(&cfg) {
                    Ok(()) => println!("\n{}  Default model set to '{}'.", "⭐", alias.bright_green()),
                    Err(e) => println!("\n{}  Failed to save models.toml: {}", "❌", e),
                }
            } else {
                println!("\n{}  Alias '{}' not found.", "⚠️", alias);
            }
        }

        // /model fetch <url> — handled as async in the REPL loop
        sub if sub.starts_with("fetch ") || sub == "fetch" => {
            println!("\n{}  Use /model fetch <url> from the main prompt (handled externally).", "💡");
        }

        // /model <alias> — switch to alias
        _ => {
            let alias = sub;
            let cfg = crate::model_manager::load();
            if let Some(resolved) = cfg.resolve(alias) {
                agent.switch_model(&resolved);
                println!("\n{}  Switched to model: {} ({})",
                    "✅",
                    alias.bright_green(),
                    resolved.model.bright_white()
                );
            } else {
                println!("\n{}  Unknown alias '{}'. Use {} to see available models.",
                    "❌",
                    alias.bright_red(),
                    "/model".bright_cyan()
                );
            }
        }
    }
}

/// Handle `/endpoint [subcommand]` — manage endpoint definitions in models.toml.
///
/// - `/endpoint`                   — list all endpoints
/// - `/endpoint rm <name>`         — remove an endpoint
/// - `/endpoint add <name> <url> [--key <key>] [--provider <p>]` — add manually
fn handle_endpoint_command(input: &str) {
    let sub = input.strip_prefix("/endpoint").unwrap_or("").trim();

    match sub {
        "" => {
            let cfg = crate::model_manager::load();
            if cfg.endpoints.is_empty() {
                println!("\n🔗  No endpoints configured.");
                println!("  Endpoints are created automatically by {} or manually with",
                    "/model fetch".bright_cyan());
                println!("  {} add <name> <url> [--key <key>] [--provider <p>]",
                    "/endpoint".bright_cyan());
            } else {
                println!("\n🔗  {} endpoint(s) configured:\n", cfg.endpoints.len());
                for (name, ep) in &cfg.endpoints {
                    let models_using: Vec<&str> = cfg.models.iter()
                        .filter(|(_, m)| m.endpoint.as_deref() == Some(name.as_str()))
                        .map(|(a, _)| a.as_str())
                        .collect();
                    println!("  {} {} — {}", 
                        "•".bright_cyan(),
                        name.bright_white(),
                        ep.base_url.dimmed());
                    println!("    provider: {}, key: {}", 
                        ep.provider.bright_white(),
                        if ep.api_key.is_some() { "set".green() } else { "not set".yellow() }
                    );
                    if !models_using.is_empty() {
                        println!("    models: {}", models_using.join(", ").bright_white());
                    } else {
                        println!("    models: {}", "(none)".dimmed());
                    }
                }
                println!();
                println!("  Remove: {} rm <name>", "/endpoint".bright_cyan());
                println!("  Add:    {} add <name> <url> [--key <k>] [--provider <p>]", "/endpoint".bright_cyan());
            }
        }

        // /endpoint rm <name>
        sub if sub.starts_with("rm ") => {
            let name = sub.strip_prefix("rm ").unwrap_or("").trim();
            if name.is_empty() {
                println!("\n{}  Usage: /endpoint rm <name>", "⚠️");
                return;
            }
            let mut cfg = crate::model_manager::load();
            if !cfg.endpoints.contains_key(name) {
                println!("\n{}  Endpoint '{}' not found.", "⚠️", name);
                return;
            }

            // Check for models referencing this endpoint
            let using: Vec<String> = cfg.models.iter()
                .filter(|(_, m)| m.endpoint.as_deref() == Some(name))
                .map(|(a, _)| a.clone())
                .collect();
            if !using.is_empty() {
                println!(
                    "\n{}  Endpoint '{}' is referenced by model(s): {}",
                    "⚠️",
                    name.bright_yellow(),
                    using.join(", ").bright_white()
                );
                println!("  Remove these models first with {} or they will become unresolvable.",
                    "/model rm <alias>".bright_cyan());
                return;
            }

            cfg.endpoints.remove(name);
            match crate::model_manager::save(&cfg) {
                Ok(()) => println!("\n{}  Removed endpoint '{}'.", "🗑️", name.bright_yellow()),
                Err(e) => println!("\n{}  Failed to save models.toml: {}", "❌", e),
            }
        }

        // /endpoint add <name> <url> [--key <key>] [--provider <p>]
        sub if sub.starts_with("add ") => {
            let args = sub.strip_prefix("add ").unwrap_or("").trim();
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.len() < 2 {
                println!("\n{}  Usage: /endpoint add <name> <url> [--key <k>] [--provider <p>]", "⚠️");
                println!("  Example: /endpoint add deepseek https://api.deepseek.com/anthropic --provider anthropic");
                return;
            }
            let name = parts[0];
            let url = parts[1];

            let api_key = parts.iter().position(|&p| p == "--key")
                .and_then(|i| parts.get(i + 1).map(|k| k.to_string()));
            let provider = parts.iter().position(|&p| p == "--provider")
                .and_then(|i| parts.get(i + 1).map(|p| p.to_string()))
                .unwrap_or_else(|| "openai".to_string());

            let mut cfg = crate::model_manager::load();
            if cfg.endpoints.contains_key(name) {
                println!(
                    "\n{}  Endpoint '{}' already exists. Use {} first.",
                    "⚠️", name,
                    "/endpoint rm".bright_cyan()
                );
                return;
            }

            cfg.endpoints.insert(name.to_string(), crate::model_manager::EndpointEntry {
                provider,
                base_url: url.to_string(),
                api_key,
            });

            match crate::model_manager::save(&cfg) {
                Ok(()) => println!("\n{}  Endpoint '{}' added ({})", "✅", name.bright_green(), url.dimmed()),
                Err(e) => println!("\n{}  Failed to save models.toml: {}", "❌", e),
            }
        }

        // /endpoint fetch <url> — just a hint
        sub if sub.starts_with("fetch") => {
            println!("\n{}  Use {} to fetch models and auto-create an endpoint.",
                "💡", "/model fetch <url>".bright_cyan());
        }

        _ => {
            println!("\n{}  Unknown subcommand. Valid: list, rm, add", "⚠️");
        }
    }
}

/// Handle `/model fetch <url> [--key <api_key>]` — fetch model list from a remote API.
async fn handle_model_fetch_command(args: &str) {
    let parts: Vec<&str> = args.split_whitespace().collect();

    if parts.is_empty() {
        println!("\n📡  Usage: /model fetch <url> [--key <api_key>]");
        println!("   Fetches available models from an OpenAI-compatible or Ollama endpoint.");
        println!("   Examples:");
        println!("     /model fetch http://localhost:11434");
        println!("     /model fetch https://api.openai.com/v1 --key sk-...");
        println!("     /model fetch https://dashscope.aliyuncs.com/compatible-mode/v1 --key sk-...");
        return;
    }

    let url = parts[0];

    // Parse optional --key flag
    let api_key = if let Some(pos) = parts.iter().position(|&p| p == "--key") {
        parts.get(pos + 1).map(|k| k.to_string())
    } else {
        // Fall back to LLM_API_KEY env
        std::env::var("LLM_API_KEY").ok()
    };

    println!("\n📡  Fetching models from {} ...", url.bright_cyan());

    match crate::model_manager::fetch_models(url, api_key.as_deref()).await {
        Ok(fetched) => {
            let source_label = match fetched.source.as_str() {
                "ollama" => "Ollama",
                _ => "OpenAI-compatible",
            };
            println!("\n{}  Found {} model(s) via {}:\n",
                "✅",
                fetched.models.len().to_string().bright_white(),
                source_label.bright_cyan()
            );

            // Print numbered list
            for (i, model) in fetched.models.iter().enumerate() {
                println!("  {:>3}. {}", (i + 1).to_string().bright_yellow(), model);
            }

            // Interactive selection
            println!();
            println!("  Enter a number to select (1-{}), or press Enter to cancel:",
                fetched.models.len()
            );

            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_err() {
                return;
            }
            let line = line.trim().to_string();
            if line.is_empty() {
                println!("  Cancelled.");
                return;
            }

            let idx: usize = match line.parse::<usize>() {
                Ok(n) if n >= 1 && n <= fetched.models.len() => n - 1,
                _ => {
                    println!("\n{}  Invalid selection.", "❌");
                    return;
                }
            };

            let model_name = &fetched.models[idx];

            // Auto-suggest an alias from the model name
            let suggested_alias = crate::model_manager::sanitize_alias(model_name);
            println!("\n  Selected: {}", model_name.bright_green());

            // Determine provider
            let provider = match fetched.source.as_str() {
                "ollama" => "openai",
                _ => "openai",
            };

            // Determine base_url: strip trailing /v1 etc.
            let base_url = if fetched.source == "ollama" {
                url.to_string()
            } else {
                url.trim_end_matches('/')
                    .trim_end_matches("/v1/models")
                    .trim_end_matches("/v1")
                    .to_string()
            };

            // ── Auto-create or reuse endpoint ─────────────────────
            let cfg = crate::model_manager::load();
            let endpoint_name = auto_endpoint_name(&base_url, &fetched.source);

            // Check if an endpoint with this name already exists
            let endpoint_name = if cfg.endpoints.contains_key(&endpoint_name) {
                let existing = &cfg.endpoints[&endpoint_name];
                if existing.base_url == base_url {
                    // Same name + same URL → reuse
                    println!(
                        "  Reusing existing endpoint '{}' ({})",
                        endpoint_name.bright_cyan(),
                        base_url.dimmed()
                    );
                    endpoint_name
                } else {
                    // Name collision → generate a unique one
                    let alt = format!("{}_{}", endpoint_name, cfg.endpoints.len() + 1);
                    println!(
                        "  Endpoint name '{}' taken, using '{}' instead",
                        endpoint_name.dimmed(),
                        alt.bright_cyan()
                    );
                    alt
                }
            } else {
                endpoint_name
            };

            // ── Conflict check: same model+base_url already configured? ──
            let dup_aliases = cfg.find_duplicates(model_name, &base_url);
            if !dup_aliases.is_empty() {
                println!(
                    "\n{}  This model+endpoint is already configured under: {}",
                    "⚠️",
                    dup_aliases.join(", ").bright_yellow()
                );
                println!("  Adding a duplicate alias is allowed but usually unnecessary.");
            }

            // ── Interactive alias prompt with conflict detection ────
            let alias = loop {
                println!(
                    "  Enter an alias name (suggested: {}), or press Enter to cancel:",
                    suggested_alias.bright_cyan()
                );
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err() {
                    return;
                }
                let input = input.trim().to_string();

                // Empty → cancel
                if input.is_empty() {
                    println!("  Cancelled.");
                    return;
                }

                // Check if alias already exists
                let cfg = crate::model_manager::load();
                if cfg.has_alias(&input) {
                    let existing = &cfg.models[&input];
                    println!(
                        "\n{}  Alias '{}' already exists (model: {}, provider: {}).",
                        "⚠️",
                        input.bright_red(),
                        existing.model.bright_white(),
                        existing.provider.dimmed()
                    );
                    println!("  Overwrite? [y/N] ");
                    let mut confirm = String::new();
                    if std::io::stdin().read_line(&mut confirm).is_err() {
                        return;
                    }
                    let confirm = confirm.trim().to_lowercase();
                    if confirm == "y" || confirm == "yes" {
                        break input;
                    }
                    println!("  Enter a different alias name:");
                    continue;
                }

                break input;
            };

            // ── Build model entry (endpoint-referenced style) ─────
            let entry = crate::model_manager::ModelEntry {
                provider: String::new(),   // inherited from endpoint
                model: model_name.clone(),
                endpoint: Some(endpoint_name.clone()),
                base_url: None,            // inherited from endpoint
                api_key: None,             // inherited from endpoint
                max_tokens: None,
                thinking_enabled: None,
                reasoning_effort: None,
                temperature: None,
            };

            let mut cfg = crate::model_manager::load();
            let overwriting = cfg.has_alias(&alias);

            // Ensure endpoint exists
            if !cfg.endpoints.contains_key(&endpoint_name) {
                cfg.endpoints.insert(endpoint_name.clone(), crate::model_manager::EndpointEntry {
                    provider: provider.to_string(),
                    base_url: base_url.clone(),
                    api_key: api_key.clone(),
                });
            }

            cfg.add(alias.clone(), entry);

            match crate::model_manager::save(&cfg) {
                Ok(()) => {
                    let verb = if overwriting { "Updated" } else { "Saved" };
                    println!("\n{}  {} model '{}' as alias '{}' in models.toml",
                        "✅",
                        verb,
                        model_name.bright_white(),
                        alias.bright_green()
                    );
                    println!("  Switch to it with: {} {}", "/model".bright_cyan(), alias.bright_white());
                }
                Err(e) => {
                    println!("\n{}  Failed to save models.toml: {}", "❌", e);
                }
            }
        }
        Err(e) => {
            println!("\n{}  Failed to fetch models: {}", "❌", e);
        }
    }
}

/// Generate a readable endpoint name from a URL.
/// e.g. "https://api.deepseek.com/anthropic" → "deepseek"
///      "http://localhost:11434" → "local"
///      "https://dashscope.aliyuncs.com/compatible-mode/v1" → "dashscope"
fn auto_endpoint_name(base_url: &str, source: &str) -> String {
    if source == "ollama" {
        return "local_ollama".to_string();
    }
    // Extract hostname (strip scheme, port, and path)
    let host = base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("unknown")
        .split(':')          // strip port
        .next()
        .unwrap_or("unknown");

    // Special-case localhost
    if host == "localhost" || host == "127.0.0.1" || host == "0.0.0.0" {
        return "local".to_string();
    }

    // e.g. "api.deepseek.com" → "deepseek"
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 2 {
        let name = parts[parts.len() - 2]; // second-level domain
        if name.len() >= 3 && name != "api" && name != "com" && name != "org" && name != "net" {
            return name.to_string();
        }
        // Fallback: first meaningful subdomain
        for part in &parts[..parts.len() - 1] {
            if *part != "api" && *part != "www" && part.len() >= 3 {
                return part.to_string();
            }
        }
    }
    host.replace('.', "_")
}

/// Save the current terminal (termios) state so it can be restored later.
///
/// Child processes spawned by `run_command` can accidentally corrupt
/// terminal settings (ECHO, ICANON, VMIN, etc.) even though we set
/// their stdin to null.  Some tools or signal handlers might also
/// leave the terminal in a bad state.  Saving before `process_message`
/// and restoring after guarantees the readline prompt always works.
#[cfg(unix)]
fn save_terminal_state() -> Option<libc::termios> {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
            Some(termios)
        } else {
            None
        }
    }
}

#[cfg(not(unix))]
fn save_terminal_state() -> Option<()> {
    None
}

/// Restore terminal settings saved by `save_terminal_state`.
#[cfg(unix)]
fn restore_terminal_state(termios: &libc::termios) {
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, termios);
    }
}

#[cfg(not(unix))]
fn restore_terminal_state(_: &()) {}

/// Run `process_message` with Ctrl-C interrupt support.
///
/// A background task listens for SIGINT and sets the global interrupt flag.
/// `process_message` checks this flag at every tool-call boundary and exits
/// cleanly, leaving the conversation in a consistent state.
///
/// This is safer than `tokio::select!` which would cancel the future at an
/// arbitrary `.await` point (e.g. mid-stream LLM response), potentially
/// leaving a `ToolUse` block without a matching `ToolResult` in the history.
async fn run_interruptible(agent: &mut Agent, input: &str) -> Result<String> {
    crate::agent::clear_interrupt();
    crate::agent::clear_guidance();
    // Ctrl-C → interrupt flag
    let interrupt_guard = tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            crate::agent::request_interrupt();
        }
    });
    // Ctrl-\ (SIGQUIT) → guidance flag (pipeline executor picks it up between iterations)
    #[cfg(unix)]
    let guidance_guard = tokio::spawn(async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sigquit) = signal(SignalKind::quit()) {
            loop {
                if sigquit.recv().await.is_none() { break; }
                crate::agent::request_guidance();
            }
        }
    });
    let result = agent.process_message(input).await;
    interrupt_guard.abort();
    #[cfg(unix)]
    guidance_guard.abort();
    result
}

/// Main entry point for the CLI interaction loop
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    initial_prompt: Option<String>,
    resume_id: Option<String>,
    session_name: Option<String>,
    output: Arc<dyn AgentOutput>,
    isolation: crate::container::IsolationMode,
    global_session: bool,
    plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>,
) -> Result<()> {
    ui::print_banner();
    ui::print_workdir();

    // ── Resolve session name ─────────────────────────────────────────
    let active_session = persistence::resolve_session_name(&project_dir, session_name.as_deref());
    if session_name.is_some() {
        println!("{}  Session: {}", "📋", active_session.bright_yellow());
    }
    // Ensure migration happens before any load/save
    let _ = persistence::migrate_old_local_session(&project_dir);

    // Build sandbox: only Sandbox mode tries fuse-overlayfs.
    // Normal and Container both run without overlay protection in the CLI.
    let sandbox = if isolation == crate::container::IsolationMode::Sandbox {
        crate::sandbox::Sandbox::new(&project_dir)
    } else {
        crate::sandbox::Sandbox::disabled(&project_dir)
    };
    let sandbox_enabled = isolation == crate::container::IsolationMode::Sandbox;

    // Create or restore agent
    let mut agent = if let Some(ref session_id) = resume_id {
        // Explicit --resume: load from global session store
        match persistence::load_session(session_id) {
            Ok(session) => {
                let conversation = persistence::restore_conversation(&session);
                let msg_count = conversation.messages.len();
                println!(
                    "{}  Resumed session {} ({} messages)\n",
                    "🔄",
                    session.meta.id.bright_yellow(),
                    msg_count.to_string().bright_white()
                );
                Agent::with_conversation(config, project_dir.clone(), conversation, session.meta.id, output.clone(), sandbox, plugin_manager.clone())
            }
            Err(e) => {
                ui::print_error(&format!("Failed to resume session: {}", e));
                println!("Starting a new session instead.\n");
                Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone())
            }
        }
    } else if !global_session {
        // Default: auto-load local named session
        match persistence::load_local_named_session(&active_session, &project_dir) {
            Ok(Some(session)) => {
                let msg_count = session.messages.len();
                let conversation = persistence::restore_conversation(&session);
                println!(
                    "{}  Resumed session '{}' ({} messages)\n",
                    "🔄",
                    active_session.bright_yellow(),
                    msg_count.to_string().bright_white()
                );
                // Pass session name as session_id so auto-save picks it up
                Agent::with_conversation(config, project_dir.clone(), conversation, active_session.clone(), output.clone(), sandbox, plugin_manager.clone())
            }
            Ok(None) => Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone()),
            Err(e) => {
                tracing::warn!("Failed to load local session: {}", e);
                Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone())
            }
        }
    } else {
        Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone())
    };
    agent.global_session = global_session;

    // 检查旧格式 .agent/mcp.toml 是否存在，提示迁移
    let legacy_mcp = project_dir.join(".agent").join("mcp.toml");
    if legacy_mcp.exists() {
        output.on_warning(
            ".agent/mcp.toml 已废弃：MCP 服务配置请移至插件目录。\
            \n  创建插件目录 .agent/plugins/<名称>/，并在其中新建 mcp/<服务名>.toml。\
            \n  详见 docs/plugin_design.md。"
        );
    }

    // Load plugin tools
    if let Some(pm) = &plugin_manager {
        let mut pm_lock = pm.lock().await;
        if let Err(e) = pm_lock.load_all_plugins() {
            output.on_warning(&format!("Failed to load plugins: {}", e));
        }
        // 将项目内技能（AGENT.md / .agent/skills）注册为 @system 插件，
        // 使得 load_skill 工具可以统一查询项目技能和插件技能。
        pm_lock.load_system_skills(&project_dir);

        // Hook 总线：将 PluginManager 的 hook_bus 共享给 Agent（含 ToolExecutor）
        let hook_bus = pm_lock.get_hook_bus();
        drop(pm_lock);
        agent.set_hook_bus(Some(hook_bus.clone()));
        // ── agent.start hook（fire-and-forget）─────────────────────────────────
        {
            use crate::plugin::hook_bus::HookEvent;
            let session_id = agent.session_id().unwrap_or("none").to_string();
            hook_bus.emit(HookEvent::new(
                "agent.start",
                session_id,
                serde_json::json!({
                    "project_dir": project_dir.display().to_string(),
                    "mode": "cli",
                }),
            ));
        }
    }
    
    // Load plugin tools into tool executor
    if let Err(e) = agent.load_plugin_tools().await {
        output.on_warning(&format!("Failed to load plugin tools: {}", e));
    }

    // 将插件的 MCP 服务器连接并注册到工具执行器
    if let Some(pm) = &plugin_manager {
        let pm_lock = pm.lock().await;
        let mcp_entries = pm_lock.collect_mcp_entries();
        drop(pm_lock);
        if !mcp_entries.is_empty() {
            let (loaded, errors) = agent.load_mcp_from_entries(&mcp_entries).await;
            if !loaded.is_empty() {
                tracing::info!("Plugin MCP tools registered: {}", loaded.join(", "));
            }
            for err in &errors {
                output.on_warning(&format!("Plugin MCP: {}", err));
            }
        }
    }
    
    // 将插件的 system_prompt.md 追加到系统提示词。
    // 每个启用插件根目录下的 system_prompt.md 若存在，则按加载顺序依次追加。
    if let Some(pm) = &plugin_manager {
        let pm_lock = pm.lock().await;
        let extra = pm_lock.collect_system_prompts();
        drop(pm_lock);
        if !extra.is_empty() {
            agent.conversation.system_prompt.push_str(&extra);
        }
    }

    // 将插件 skills 注入 system_prompt。
    // @system 技能（项目内置）已由 conversation.rs 注入，这里只补充非 @system 的插件提供的技能。
    if let Some(pm) = &plugin_manager {
        let pm_lock = pm.lock().await;
        let plugin_skills: Vec<_> = pm_lock.get_all_skills()
            .into_iter()
            .filter(|s| s.plugin_id != "@system")
            .collect();
        if !plugin_skills.is_empty() {
            let mut section = "\n\n--- Plugin Skills ---".to_string();
            section.push_str("\n## Available Plugin Skills (use `load_skill` tool with the skill name to read full content)");
            for skill in &plugin_skills {
                let tags_hint = if skill.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [tags: {}]", skill.tags.join(", "))
                };
                section.push_str(&format!(
                    "\n- **{}** (plugin: {}){} — {}",
                    skill.name,
                    skill.plugin_id,
                    tags_hint,
                    skill.description,
                ));
            }
            agent.conversation.system_prompt.push_str(&section);
        }
    }

    // Print sandbox status
    if sandbox_enabled {
        let is_overlay = agent.sandbox.is_overlay().await;
        let is_disabled = agent.sandbox.is_disabled;
        if is_disabled {
            // fuse-overlayfs 不可用，sandbox 静默回退了——必须明确警告用户
            println!(
                "{}  {}",
                "⚠️ ",
                "Sandbox requested but fuse-overlayfs is NOT available — sandbox is DISABLED.".bright_red().bold()
            );
            println!(
                "   {}",
                "All file operations will affect the REAL project directory directly!".bright_red()
            );
            println!(
                "   Install fuse-overlayfs and restart to enable sandbox isolation.\n"
            );
        } else {
            let backend_label = if is_overlay { "overlay" } else { "snapshot" };
            println!(
                "{}  {}",
                "🔒",
                format!("Sandbox enabled ({}) — {}",
                    backend_label,
                    "original project untouched, all changes in overlay layer"
                ).bright_green()
            );
            println!(
                "   Use {} to view changes, {} to undo, {} to accept.\n",
                "/changes".bright_white(),
                "/rollback".bright_white(),
                "/commit".bright_white()
            );
        }
    }

    // Check for project summary at startup
    {
        if crate::summary::exists(&project_dir) {
            ui::print_summary_loaded();
        } else {
            ui::print_summary_hint();
        }
    }

    // If an initial prompt is provided, process it first
    if let Some(prompt) = initial_prompt {
        println!("{} {}\n", "👤".to_string(), prompt);
        match agent.process_message(&prompt).await {
            Ok(_) => {}
            Err(e) => ui::print_error(&format!("Error: {}", e)),
        }
    }

    // Set up the interactive line editor
    let mut rl = DefaultEditor::new()?;

    // Try to load command history
    let history_path = dirs::data_dir().map(|d| d.join("rust_agent").join("history.txt"));

    if let Some(ref path) = history_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        rl.load_history(path).ok();
    }

    'repl: loop {
        // Drain any pending service push notifications before the next prompt.
        // Notifications are shown above the prompt line before readline() is
        // called, so they never interfere with IME composition or raw-mode input.
        // (Using rustyline's ExternalPrinter would switch the read path from a
        // simple blocking read to select(), which disrupts CJK IME delivery.)
        agent.drain_service_events();

        let readline = rl.readline("🤖 > ");

        match readline {
            Ok(line) => {
                let input = line.trim();

                if input.is_empty() {
                    continue;
                }

                // Add to history
                rl.add_history_entry(input).ok();

                // Handle slash commands
                if input.starts_with('/') {
                    // /summary needs async, handle it separately
                    if input == "/summary" || input.starts_with("/summary ") {
                        handle_summary_command(input, &mut agent).await;
                        continue;
                    }
                    // /plan needs async, handle it separately
                    if input == "/plan" || input.starts_with("/plan ") {
                        handle_plan_command(input, &mut agent).await;
                        continue;
                    }
                    // /nodes probes all peers in global.db, prints status
                    if input == "/nodes" {
                        let db = crate::db::GlobalDb::open_or_create().ok();
                        let peers: Vec<crate::workspaces::PeerEntry> = db.as_ref()
                            .map(|d| crate::workspaces::load_peers_from_db(d))
                            .unwrap_or_default();
                        let cluster_tok = crate::workspaces::cluster_token_from_env();
                        handle_nodes_command(&peers, cluster_tok).await;
                        continue;
                    }
                    // Sandbox commands need async
                    if input == "/rollback" {
                        handle_rollback_command(&mut agent).await;
                        continue;
                    }
                    if input == "/commit" {
                        handle_commit_command(&mut agent).await;
                        continue;
                    }
                    if input == "/changes" {
                        handle_changes_command(&agent).await;
                        continue;
                    }
                    // Plugin commands
                    if input.starts_with("/plugin") {
                        let subcommand = input.strip_prefix("/plugin").map(|s| s.trim()).unwrap_or("");
                        handle_plugin_command(subcommand, &mut agent).await;
                        continue;
                    }
                    // Memory consolidation
                    if input == "/consolidate" {
                        handle_consolidate_command(&mut agent).await;
                        continue;
                    }
                    // File upload: /upload <source_path> [target_name]
                    if input.starts_with("/upload") {
                        let args = input.strip_prefix("/upload").map(|s| s.trim()).unwrap_or("");
                        handle_upload_command(args, &agent).await;
                        continue;
                    }
                    // Model fetch: /model fetch <url> [--key <key>]
                    if input.starts_with("/model fetch") || input == "/model fetch" {
                        let args = input.strip_prefix("/model fetch").map(|s| s.trim()).unwrap_or("");
                        handle_model_fetch_command(args).await;
                        continue;
                    }
                    
                    let handled = handle_slash_command(input, &mut agent);
                    match handled {
                        SlashResult::Continue => continue,
                        SlashResult::Quit => break,
                        SlashResult::NotACommand => {} // fall through to process as message
                    }
                }

                // Save terminal state before processing, so we can restore it
                // if a child process or tool panic corrupts termios settings.
                let saved_termios = save_terminal_state();

                // Run with Ctrl-C support.  A background task sets the interrupt
                // flag on SIGINT; process_message checks it at every tool-call
                // boundary and exits cleanly.  This avoids the select! approach
                // which would cancel the future at an arbitrary await point and
                // could leave the conversation in an inconsistent state.
                let result = run_interruptible(&mut agent, input).await;

                // Restore terminal state to prevent accumulated corruption
                if let Some(ref termios) = saved_termios {
                    restore_terminal_state(termios);
                }

                // If interrupted, offer an inline correction prompt.
                if crate::agent::is_interrupted() {
                    crate::agent::clear_interrupt();
                    println!(
                        "\n{}  {}",
                        "⚡".yellow().bold(),
                        "Interrupted. Type a correction and press Enter, or just Enter to stop:"
                            .bright_cyan()
                    );
                    let correction = rl.readline("✏️  > ").unwrap_or_default();
                    let correction = correction.trim().to_string();
                    if !correction.is_empty() {
                        rl.add_history_entry(&correction).ok();
                        // Handle slash commands typed at the correction prompt
                        // (e.g. the user types /quit to exit instead of correcting).
                        if correction.starts_with('/') {
                            let handled = handle_slash_command(&correction, &mut agent);
                            match handled {
                                SlashResult::Quit => break 'repl,
                                SlashResult::Continue => continue 'repl,
                                SlashResult::NotACommand => {} // fall through to LLM
                            }
                        }
                        // Also use run_interruptible here so Ctrl-C works
                        // during the correction run, not just the first run.
                        let saved2 = save_terminal_state();
                        match run_interruptible(&mut agent, &correction).await {
                            Ok(_) => { auto_save_session(&mut agent); }
                            Err(e) => ui::print_error(&format!("{:#}", e)),
                        }
                        if let Some(ref t) = saved2 { restore_terminal_state(t); }
                    }
                    continue 'repl;
                }

                match result {
                    Ok(_) => { auto_save_session(&mut agent); }
                    Err(e) => ui::print_error(&format!("{:#}", e)),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("\n{}", "Use /quit to exit".dimmed());
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Save session before exiting
                auto_save_session(&mut agent);
                println!("\n{}", "👋 Goodbye!".bright_green());
                break;
            }
            Err(err) => {
                ui::print_error(&format!("Input error: {}", err));
                break;
            }
        }
    }

    // Save command history
    if let Some(ref path) = history_path {
        rl.save_history(path).ok();
    }

    // Sandbox cleanup: unmount overlay if active
    if agent.sandbox.is_enabled().await {
        let has_changes = agent.sandbox.ops_count().await > 0;
        if has_changes && agent.sandbox.is_overlay().await {
            println!(
                "\n{}  {}",
                "⚠️",
                "Sandbox has uncommitted overlay changes — cleaning up mount...".yellow()
            );
        }
        agent.sandbox.cleanup().await;
    }

    Ok(())
}

/// Handle `/summary` command — generate or load project summary.
async fn handle_summary_command(input: &str, agent: &mut Agent) {
    let subcommand = input.strip_prefix("/summary").unwrap_or("").trim();
    let cwd = &agent.project_dir;

    match subcommand {
        "generate" => {
            // Force (re-)generate
            if crate::summary::exists(cwd) {
                println!(
                    "\n{}  {}",
                    "⚠️",
                    "A project summary already exists. Regenerating...".yellow()
                );
            }
            ui::print_summary_generating();
            match agent.generate_project_summary().await {
                Ok(_) => {
                    ui::print_summary_done();
                }
                Err(e) => {
                    ui::print_error(&format!("Failed to generate summary: {}", e));
                }
            }
        }
        "" => {
            // Show existing summary, or prompt to generate
            if let Some(summary) = crate::summary::load(cwd) {
                println!(
                    "\n{}  {}:\n",
                    "📋",
                    "Project Summary".bright_cyan().bold()
                );
                let skin = termimad::MadSkin::default();
                skin.print_text(&summary);
                println!();
                println!(
                    "  {} Run {} to regenerate.",
                    "💡".dimmed(),
                    "/summary generate".bright_white()
                );
            } else {
                println!("\n{}  No project summary found.", "📋");
                print!("  Generate one now? {} ", "[y/N]".bright_white());
                use std::io::Write;
                std::io::stdout().flush().ok();

                let mut answer = String::new();
                if std::io::stdin().read_line(&mut answer).is_ok() {
                    let answer = answer.trim().to_lowercase();
                    if answer == "y" || answer == "yes" {
                        ui::print_summary_generating();
                        match agent.generate_project_summary().await {
                            Ok(_) => {
                                ui::print_summary_done();
                            }
                            Err(e) => {
                                ui::print_error(&format!("Failed to generate summary: {}", e));
                            }
                        }
                    } else {
                        println!("  {}", "Skipped.".dimmed());
                    }
                }
            }
        }
        other => {
            println!(
                "\n{}  Unknown subcommand: {}. Usage: {} or {}",
                "⚠️",
                other,
                "/summary".bright_white(),
                "/summary generate".bright_white()
            );
        }
    }
}

/// Handle `/consolidate` — run a "dreaming pass": distil session log + existing
/// knowledge into a refined knowledge set using the LLM. No conversation context
/// is consumed; the output goes directly into `.agent/memory.md`.
async fn handle_consolidate_command(agent: &mut Agent) {
    if agent.memory.is_empty() {
        println!("\n{}  Memory is empty — nothing to consolidate.", "🧠");
        return;
    }
    println!("\n{}  Consolidating memory…", "🧠");
    match agent.consolidate_memory().await {
        Ok(0) => println!("  No new knowledge extracted."),
        Ok(n) => println!(
            "  {}  Extracted {} knowledge item{}. Run {} to review.",
            "✅",
            n,
            if n == 1 { "" } else { "s" },
            "/memory".bright_white()
        ),
        Err(e) => println!("  {}  Consolidation failed: {}", "❌", e),
    }
}

/// Handle `/plan` command — generate a plan for a task.
async fn handle_plan_command(input: &str, agent: &mut Agent) {
    let task = input.strip_prefix("/plan").unwrap_or("").trim();
    if task.is_empty() {
        println!(
            "\n{}  Usage: {} {}",
            "📋",
            "/plan".bright_white(),
            "<task description>".dimmed()
        );
        return;
    }
    
    println!("\n{}  Generating plan for: {}\n", "🧠", task.bright_cyan());
    
    // Plan generation not implemented in this branch
    println!("{}  Plan generation is not available in this branch.", "⚠️");
}

/// Handle `/rollback` — discard all sandbox changes (restore original).
async fn handle_rollback_command(agent: &mut Agent) {
    if !agent.sandbox.is_enabled().await {
        println!(
            "\n{}  {}",
            "⚠️",
            "Sandbox is not enabled. Start the agent with --sandbox to use this feature.".yellow()
        );
        return;
    }

    let ops = agent.sandbox.ops_count().await;
    if ops == 0 {
        println!(
            "\n{}  {}",
            "📋",
            "No changes to rollback.".dimmed()
        );
        return;
    }

    // Show what will be lost
    let changes = agent.sandbox.changed_files().await;
    println!(
        "\n{}  {} change(s) will be lost:",
        "⚠️",
        changes.len().to_string().bright_white()
    );
    for c in &changes {
        let icon = match c.kind {
            crate::sandbox::ChangeKind::Modified => "✏️ ",
            crate::sandbox::ChangeKind::Created => "📄",
            crate::sandbox::ChangeKind::Deleted => "🗑️",
        };
        println!("    {} {} ({})", icon, c.path.display().to_string().bright_white(), c.kind);
    }
    println!();
    println!("  {} [y/N]?", "Rollback all changes?".bright_red().bold());
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_ok() {
        let answer = answer.trim().to_lowercase();
        if answer == "y" || answer == "yes" {
            let result = agent.sandbox.rollback().await;
            if result.errors.is_empty() {
                println!(
                    "\n{}  Rolled back: {} restored, {} deleted. Project restored to original state.",
                    "✅",
                    result.restored.to_string().bright_green(),
                    result.deleted.to_string().bright_green()
                );
            } else {
                println!(
                    "\n{}  Rollback completed with {} error(s):",
                    "⚠️",
                    result.errors.len()
                );
                for err in &result.errors {
                    println!("    {} {}", "✗".bright_red(), err);
                }
            }
        } else {
            println!("  {}", "Rollback cancelled.".dimmed());
        }
    }
}

/// Handle `/commit` — accept all sandbox changes (discard snapshots).
async fn handle_commit_command(agent: &mut Agent) {
    if !agent.sandbox.is_enabled().await {
        println!(
            "\n{}  {}",
            "⚠️",
            "Sandbox is not enabled. Start the agent with --sandbox to use this feature.".yellow()
        );
        return;
    }

    let ops = agent.sandbox.ops_count().await;
    if ops == 0 {
        println!(
            "\n{}  {}",
            "📋",
            "No changes to commit.".dimmed()
        );
        return;
    }

    // Show what will be committed
    let changes = agent.sandbox.changed_files().await;
    println!(
        "\n{}  {} change(s) will be committed to the project:",
        "📦",
        changes.len().to_string().bright_white()
    );
    for c in &changes {
        let icon = match c.kind {
            crate::sandbox::ChangeKind::Modified => "✏️ ",
            crate::sandbox::ChangeKind::Created => "📄",
            crate::sandbox::ChangeKind::Deleted => "🗑️",
        };
        println!("    {} {} ({})", icon, c.path.display().to_string().bright_white(), c.kind);
    }
    println!();

    let result = agent.sandbox.commit().await;
    println!(
        "{}  Committed: {} modified, {} created.",
        "✅",
        result.modified.to_string().bright_green(),
        result.created.to_string().bright_green()
    );
    println!();
}

/// Handle `/changes` — display sandbox-tracked file modifications.
async fn handle_changes_command(agent: &Agent) {
    if !agent.sandbox.is_enabled().await {
        println!(
            "\n{}  {}",
            "⚠️",
            "Sandbox is not enabled. Start the agent with --sandbox to use this feature.".yellow()
        );
        return;
    }

    let changes = agent.sandbox.changed_files().await;
    if changes.is_empty() {
        println!(
            "\n{}  {}",
            "📋",
            "No changes tracked yet.".dimmed()
        );
        return;
    }

    let mut modified = 0usize;
    let mut created = 0usize;

    println!(
        "\n{}  {} tracked change(s):\n",
        "📋",
        changes.len().to_string().bright_white()
    );

    for c in &changes {
        let (icon, label) = match c.kind {
            crate::sandbox::ChangeKind::Modified => {
                modified += 1;
                ("✏️ ", "modified".bright_yellow().to_string())
            }
            crate::sandbox::ChangeKind::Created => {
                created += 1;
                ("📄", "created".bright_green().to_string())
            }
            crate::sandbox::ChangeKind::Deleted => {
                modified += 1; // count as a modification
                ("🗑️", "deleted".bright_red().to_string())
            }
        };
        let size_info = match (c.original_size, c.current_size) {
            (Some(orig), Some(curr)) if orig != curr => {
                format!(" ({} → {} bytes)", orig, curr)
            }
            (None, Some(curr)) => format!(" ({} bytes)", curr),
            _ => String::new(),
        };
        println!(
            "    {} {} [{}]{}",
            icon,
            c.path.display().to_string().bright_white(),
            label,
            size_info.dimmed()
        );
    }

    println!();
    println!(
        "  Summary: {} modified, {} created",
        modified.to_string().bright_yellow(),
        created.to_string().bright_green(),
    );
    println!(
        "  Use {} to undo all, {} to accept all.\n",
        "/rollback".bright_white(),
        "/commit".bright_white()
    );
}

/// Percent-encode a token value for use as a URL query-parameter.
fn probe_url_encode(s: &str) -> String {
    s.bytes()
        .flat_map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                vec![b as char]
            } else {
                format!("%{:02X}", b).chars().collect()
            }
        })
        .collect()
}

// ── /nodes command ────────────────────────────────────────────────────────────

/// Probe every `[[peer]]` entry collected from the plugin system, print hierarchical
/// status (physical server → virtual nodes with workdir/sandbox/tags), and populate
/// the in-process route table so that subsequent `any:<tag>` calls work immediately.
async fn handle_nodes_command(peers: &[crate::workspaces::PeerEntry], cluster_tok: Option<String>) {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let remotes = peers.to_vec();

    if remotes.is_empty() {
        println!(
            "\n{}",
            "📡  No [[peer]] entries found.".bright_yellow()
        );
        println!(
            "  在插件的 {} 文件中添加 [[peer]] 条目。",
            "workspaces.toml".bright_yellow()
        );
        return;
    }

    println!("\n{}", "📡  Probing remote nodes...".bright_cyan());

    for remote in &remotes {
        // Use /probe path so the server handles this inline (no worker fork).
        let remote_url = remote.url.as_str();
        let probe_base = crate::workspaces::with_path(remote_url, "/probe");
        // Resolve auth token: peer-level overrides cluster-level.
        let tok = remote.token.as_deref().or(cluster_tok.as_deref());
        let url = {
            let sep = if probe_base.contains('?') { '&' } else { '?' };
            match tok {
                Some(t) => format!("{}{}discover=1&token={}", probe_base, sep, probe_url_encode(t)),
                None    => format!("{}{}discover=1", probe_base, sep),
            }
        };

        let connect_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            connect_async(&url),
        ).await;

        match connect_result {
            Ok(Ok((ws_stream, _))) => {
                let (mut write, mut read) = ws_stream.split();
                let ready_result = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    async {
                        while let Some(msg) = read.next().await {
                            if let Ok(Message::Text(text)) = msg {
                                if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if ev["type"] == "ready" {
                                        return Some(ev);
                                    }
                                }
                            }
                        }
                        None
                    },
                ).await;
                let _ = write.send(Message::Close(None)).await;

                match ready_result {
                    Ok(Some(ref ev)) => {
                        let workdir = ev["data"]["workdir"].as_str().unwrap_or("(default)");
                        let sb_raw  = ev["data"]["sandbox"].as_bool().unwrap_or(false);
                        let sb_str  = if sb_raw { "on " } else { "off" };

                        // Parse caps for summary line.
                        let caps_line = if ev["data"]["caps"].is_object() {
                            let c    = &ev["data"]["caps"];
                            let arch = c["arch"].as_str().unwrap_or("?");
                            let os   = c["os"].as_str().unwrap_or("?");
                            let cpu  = c["cpu_cores"].as_u64().unwrap_or(0);
                            let ram  = c["ram_gb"].as_u64().unwrap_or(0);
                            let gpu_str = if let Some(gpus) = c["gpus"].as_array() {
                                if gpus.is_empty() {
                                    String::new()
                                } else {
                                    let names: Vec<&str> =
                                        gpus.iter().filter_map(|g| g["name"].as_str()).collect();
                                    format!("  GPU: {}", names.join(", "))
                                }
                            } else {
                                String::new()
                            };
                            let bins = if let Some(b) = c["bins"].as_array() {
                                let v: Vec<&str> = b.iter().filter_map(|x| x.as_str()).collect();
                                if v.is_empty() { String::new() } else { format!("  bins: {}", v.join(" ")) }
                            } else {
                                String::new()
                            };
                            format!(
                                "{}/{}  CPU:{} cores  RAM:{} GiB{}{}",
                                os, arch, cpu, ram, gpu_str, bins
                            )
                        } else {
                            String::new()
                        };

                        // Parse virtual nodes.
                        let virtual_nodes: Vec<crate::workspaces::VirtualNodeInfo> =
                            if let Some(arr) = ev["data"]["virtual_nodes"].as_array() {
                                arr.iter()
                                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                                    .collect()
                            } else {
                                vec![]
                            };

                        // Populate route table so any:<tag> works immediately.
                        if !virtual_nodes.is_empty() {
                            let raw_url = remote.url.as_str();
                            let base_url = raw_url.splitn(2, '?').next().unwrap_or(raw_url);
                            crate::workspaces::update_route_table(
                                &remote.name, base_url, &virtual_nodes,
                            );
                        }

                        // Print physical server header.
                        println!(
                            "  {} {}  sandbox:{}  {}",
                            "✅".green(),
                            remote.name.bright_white().bold(),
                            sb_str,
                            workdir.dimmed(),
                        );
                        if !caps_line.is_empty() {
                            println!("     {}", caps_line.dimmed());
                        }

                        // Print virtual nodes indented.
                        if !virtual_nodes.is_empty() {
                            println!("     {}", "Virtual nodes:".bright_cyan());
                            let last = virtual_nodes.len() - 1;
                            for (i, vn) in virtual_nodes.iter().enumerate() {
                                let prefix = if i == last { "└──" } else { "├──" };
                                let vn_sb = if vn.sandbox { "sandbox:on " } else { "sandbox:off" };
                                let tags_str = if vn.tags.is_empty() {
                                    String::new()
                                } else {
                                    format!("  [{}]", vn.tags.join(", "))
                                };
                                let desc = if vn.description.is_empty() {
                                    String::new()
                                } else {
                                    format!("  — {}", vn.description)
                                };
                                println!(
                                    "     {} {} {}  {}{}{}",
                                    prefix.dimmed(),
                                    vn.name.bright_white(),
                                    vn_sb,
                                    vn.workdir.dimmed(),
                                    tags_str.bright_yellow(),
                                    desc.dimmed(),
                                );
                            }
                        }
                        println!();
                    }
                    _ => {
                        println!(
                            "  {} {}  {}",
                            "✅".green(),
                            remote.name.bright_white().bold(),
                            "online (no ready data)".yellow(),
                        );
                        println!();
                    }
                }
            }
            _ => {
                println!(
                    "  {} {}  {}",
                    "❌".red(),
                    remote.name.bright_white().bold(),
                    "offline".red(),
                );
                println!();
            }
        }
    }
}
