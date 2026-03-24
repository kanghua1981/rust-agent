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

/// Main entry point for the CLI interaction loop
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    initial_prompt: Option<String>,
    resume_id: Option<String>,
    output: Arc<dyn AgentOutput>,
    isolation: crate::container::IsolationMode,
    global_session: bool,
) -> Result<()> {
    ui::print_banner();
    ui::print_workdir();

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
                Agent::with_conversation(config, project_dir.clone(), conversation, session.meta.id, output.clone(), sandbox)
            }
            Err(e) => {
                ui::print_error(&format!("Failed to resume session: {}", e));
                println!("Starting a new session instead.\n");
                Agent::new(config, project_dir.clone(), output.clone(), sandbox)
            }
        }
    } else if !global_session {
        // Default: auto-load local session from .agent/session.json
        match persistence::load_local_session(&project_dir) {
            Ok(Some(session)) => {
                let msg_count = session.messages.len();
                let conversation = persistence::restore_conversation(&session);
                println!(
                    "{}  Resumed local session ({} messages)\n",
                    "🔄",
                    msg_count.to_string().bright_white()
                );
                Agent::with_conversation(config, project_dir.clone(), conversation, "local".to_string(), output.clone(), sandbox)
            }
            Ok(None) => Agent::new(config, project_dir.clone(), output.clone(), sandbox),
            Err(e) => {
                tracing::warn!("Failed to load local session: {}", e);
                Agent::new(config, project_dir.clone(), output.clone(), sandbox)
            }
        }
    } else {
        Agent::new(config, project_dir.clone(), output.clone(), sandbox)
    };
    agent.global_session = global_session;

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
                    // /nodes probes all [[remote]] entries in workspaces.toml
                    if input == "/nodes" {
                        handle_nodes_command(&agent.project_dir).await;
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
                match persistence::save_local_session(&agent.conversation, &agent.project_dir) {
                    Ok(()) => println!("\n{}  Session saved to {}", "💾", ".agent/session.json".bright_yellow()),
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
            let mem = &agent.memory;
            if mem.is_empty() {
                println!(
                    "\n{}  Memory is empty. It will grow as you use the agent.",
                    "🧠"
                );
            } else {
                println!("\n{}  Agent Memory ({} entries):", "🧠", mem.entry_count());
                if !mem.knowledge.is_empty() {
                    println!("  {} {}:", "📖", "Project Knowledge".bright_cyan());
                    for fact in &mem.knowledge {
                        println!("    {} {}", "•".dimmed(), fact);
                    }
                }
                if !mem.file_map.is_empty() {
                    println!("  {} {}:", "📁", "Key Files".bright_cyan());
                    for (path, desc) in &mem.file_map {
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
                if !mem.session_log.is_empty() {
                    println!("  {} {}:", "📝", "Session Log".bright_cyan());
                    for entry in &mem.session_log {
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
        _ if input == "/export" || input.starts_with("/export ") => {
            use std::fmt::Write as FmtWrite;
            use std::time::{SystemTime, UNIX_EPOCH};
            use crate::conversation::{ContentBlock, Role};

            let ts_string = {
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let s  = secs % 60;
                let m  = (secs / 60) % 60;
                let h  = (secs / 3600) % 24;
                let days = secs / 86400;
                let z   = days + 719468;
                let era = z / 146097;
                let doe = z - era * 146097;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                let y   = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp  = (5 * doy + 2) / 153;
                let d   = doy - (153 * mp + 2) / 5 + 1;
                let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
                let yr  = if mo <= 2 { y + 1 } else { y };
                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", yr, mo, d, h, m, s)
            };

            let filename = {
                let custom = input.strip_prefix("/export").unwrap_or("").trim();
                if custom.is_empty() {
                    let compact = ts_string.replace(" UTC", "").replace(' ', "-").replace(':', "");
                    format!("conversation-{}.md", compact)
                } else if custom.ends_with(".md") {
                    custom.to_owned()
                } else {
                    format!("{}.md", custom)
                }
            };
            let path = agent.project_dir.join(&filename);

            let mut md = String::new();
            let _ = writeln!(md, "# Conversation Export\n");
            let _ = writeln!(md, "Generated: {}\n", ts_string);
            let _ = writeln!(md, "---\n");

            for msg in &agent.conversation.messages {
                    // Skip messages that contain only ToolResult blocks.
                    let is_tool_result_only = msg.content.iter().all(|b| {
                        matches!(b, ContentBlock::ToolResult { .. })
                    });

                    match msg.role {
                        Role::User if !is_tool_result_only => {
                            let _ = writeln!(md, "## \u{1F9D1} You\n");
                            for block in &msg.content {
                                if let ContentBlock::Text { text } = block {
                                    let _ = writeln!(md, "{}\n", text.trim());
                                }
                            }
                            let _ = writeln!(md, "---\n");
                        }
                        Role::Assistant => {
                            let _ = writeln!(md, "## \u{1F916} Agent\n");
                            for block in &msg.content {
                                match block {
                                    ContentBlock::Text { text } if !text.trim().is_empty() => {
                                        let _ = writeln!(md, "{}\n", text.trim());
                                    }
                                    ContentBlock::ToolUse { name, input: tool_input, .. } => {
                                        let pretty = serde_json::to_string_pretty(tool_input)
                                            .unwrap_or_else(|_| tool_input.to_string());
                                        let _ = writeln!(md, "**Tool:** `{}`\n", name);
                                        let _ = writeln!(md, "```json");
                                        let _ = writeln!(md, "{}", pretty);
                                        let _ = writeln!(md, "```\n");
                                    }
                                    _ => {}
                                }
                            }
                            let _ = writeln!(md, "---\n");
                        }
                        _ => {}
                    }
                }

            match std::fs::write(&path, &md) {
                Ok(()) => println!(
                    "\n{}  Saved: {}  {}",
                    "💾",
                    path.display().to_string().bright_white(),
                    format!("({} bytes)", md.len()).dimmed()
                ),
                Err(e) => ui::print_error(&format!("Export failed: {}", e)),
            }
            SlashResult::Continue
        }
        _ => {
            // Unknown slash command, treat as regular message
            if input.starts_with('/') {
                println!(
                    "\n{}  Unknown command: {}. Type {} for available commands.",
                    "❓",
                    input.bright_red(),
                    "/help".bright_white()
                );
                SlashResult::Continue
            } else {
                SlashResult::NotACommand
            }
        }
    }
}

/// Handle `/model` command — list, switch, add, remove, default.
///
/// - `/model`                — show current model & list all configured models
/// - `/model <alias>`        — switch to the model with the given alias
/// - `/model add <alias>`    — interactively add a new model entry
/// - `/model remove <alias>` — remove a model entry
/// - `/model default <alias>`— set the default model alias
pub fn handle_model_command(input: &str, agent: &mut Agent) {
    let subcommand = input.strip_prefix("/model").unwrap_or("").trim();

    match subcommand {
        "" => {
            // Show current model and list all configured models
            println!(
                "\n{}  Current model: {} ({})",
                "🤖",
                agent.config.model.bright_white().bold(),
                agent.config.provider.to_string().bright_cyan()
            );
            if let Some(ref alias) = agent.config.model_alias {
                println!("  Alias: {}", alias.bright_yellow());
            }
            println!();

            let models_cfg = crate::model_manager::load();
            if models_cfg.models.is_empty() {
                println!(
                    "  {}  No models configured in {}",
                    "📋",
                    crate::model_manager::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "models.toml".to_string())
                        .dimmed()
                );
                println!(
                    "  Use {} to add a model.\n",
                    "/model add <alias>".bright_white()
                );
            } else {
                let default_alias = models_cfg.default.as_deref().unwrap_or("");
                println!("  {}  Configured models:", "📋");
                for alias in models_cfg.aliases() {
                    if let Some(entry) = models_cfg.models.get(alias) {
                        let marker = if alias == default_alias { " ⭐" } else { "" };
                        let active = agent
                            .config
                            .model_alias
                            .as_deref()
                            .map(|a| a == alias)
                            .unwrap_or(false);
                        let prefix = if active {
                            "▶".bright_green().to_string()
                        } else {
                            "•".dimmed().to_string()
                        };
                        println!(
                            "    {} {} {} ({}/{}){}", 
                            prefix,
                            alias.bright_yellow(),
                            "→".dimmed(),
                            entry.provider.bright_cyan(),
                            entry.model.bright_white(),
                            marker
                        );
                    }
                }
                println!();

                // ── Pipeline / Role status ────────────────────────────────
                let pipeline_enabled = agent.pipeline_enabled();
                let pipeline_label = if pipeline_enabled {
                    "✅ 已启用".bright_green().to_string()
                } else {
                    "❌ 已禁用".dimmed().to_string()
                };
                println!("  {}  Pipeline: {}", "🔀", pipeline_label);

                if !models_cfg.roles.is_empty() {
                    println!("  {}  角色配置:", "🎭");
                    let role_icons = [("planner", "🧠"), ("executor", "⚙️ "), ("checker", "🔍")];
                    // Print known roles in order first
                    for (role, icon) in &role_icons {
                        if let Some(r) = models_cfg.roles.get(*role) {
                            println!(
                                "    {} {} → {}",
                                icon,
                                role.bright_yellow(),
                                r.model.bright_white()
                            );
                        }
                    }
                    // Then any custom roles
                    let known = ["planner", "executor", "checker"];
                    for (role, r) in &models_cfg.roles {
                        if !known.contains(&role.as_str()) {
                            println!(
                                "    🔧 {} → {}",
                                role.bright_yellow(),
                                r.model.bright_white()
                            );
                        }
                    }
                    println!();
                    if !pipeline_enabled {
                        println!(
                            "  {}  开启流水线：在 {} 中设置 {} = {}",
                            "💡".to_string().dimmed(),
                            "models.toml".bright_white(),
                            "[pipeline] enabled".bright_yellow(),
                            "true".bright_green()
                        );
                    }
                } else {
                    println!(
                        "  {}  未配置角色。在 {} 中添加 {} 以启用多角色流水线。",
                        "💡".to_string().dimmed(),
                        "models.toml".bright_white(),
                        "[roles.planner]".bright_yellow()
                    );
                }

                println!();
                println!(
                    "  Switch: {}  Add: {}  Remove: {}",
                    "/model <alias>".bright_white(),
                    "/model add <alias>".bright_white(),
                    "/model remove <alias>".bright_white()
                );
                println!();
            }
        }
        sub if sub.starts_with("add ") => {
            let alias = sub.strip_prefix("add ").unwrap().trim();
            if alias.is_empty() {
                println!("\n{}  Usage: /model add <alias>", "❓");
                return;
            }
            // Interactive prompts
            println!(
                "\n{}  Adding model '{}'",
                "➕",
                alias.bright_yellow()
            );
            let provider = prompt_line("  Provider (anthropic/openai/compatible): ");
            let model = prompt_line("  Model name: ");
            let base_url_raw = prompt_line("  Base URL (leave blank for default): ");
            let api_key_raw = prompt_line("  API key (leave blank to use env var): ");

            let base_url = if base_url_raw.is_empty() {
                None
            } else {
                Some(base_url_raw)
            };
            let api_key = if api_key_raw.is_empty() {
                None
            } else {
                Some(api_key_raw)
            };

            let entry = crate::model_manager::ModelEntry {
                provider,
                model,
                base_url,
                api_key,
                max_tokens: None,
            };

            let mut models_cfg = crate::model_manager::load();
            models_cfg.add(alias.to_string(), entry);

            // If this is the first model, also set it as default
            if models_cfg.models.len() == 1 {
                models_cfg.set_default(alias.to_string());
            }

            match crate::model_manager::save(&models_cfg) {
                Ok(_) => {
                    println!(
                        "\n{}  Model '{}' saved to {}",
                        "✅",
                        alias.bright_yellow(),
                        crate::model_manager::config_path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                            .dimmed()
                    );
                }
                Err(e) => {
                    ui::print_error(&format!("Failed to save models config: {}", e));
                }
            }
        }
        sub if sub.starts_with("remove ") => {
            let alias = sub.strip_prefix("remove ").unwrap().trim();
            if alias.is_empty() {
                println!("\n{}  Usage: /model remove <alias>", "❓");
                return;
            }
            let mut models_cfg = crate::model_manager::load();
            if models_cfg.remove(alias) {
                match crate::model_manager::save(&models_cfg) {
                    Ok(_) => {
                        println!(
                            "\n{}  Model '{}' removed.",
                            "🗑️",
                            alias.bright_yellow()
                        );
                    }
                    Err(e) => {
                        ui::print_error(&format!("Failed to save models config: {}", e));
                    }
                }
            } else {
                println!(
                    "\n{}  Model '{}' not found.",
                    "❓",
                    alias.bright_red()
                );
            }
        }
        sub if sub.starts_with("default ") => {
            let alias = sub.strip_prefix("default ").unwrap().trim();
            if alias.is_empty() {
                println!("\n{}  Usage: /model default <alias>", "❓");
                return;
            }
            let mut models_cfg = crate::model_manager::load();
            if models_cfg.models.contains_key(alias) {
                models_cfg.set_default(alias.to_string());
                match crate::model_manager::save(&models_cfg) {
                    Ok(_) => {
                        println!(
                            "\n{}  Default model set to '{}'.",
                            "⭐",
                            alias.bright_yellow()
                        );
                    }
                    Err(e) => {
                        ui::print_error(&format!("Failed to save models config: {}", e));
                    }
                }
            } else {
                println!(
                    "\n{}  Model '{}' not found. Use {} to see available models.",
                    "❓",
                    alias.bright_red(),
                    "/model".bright_white()
                );
            }
        }
        alias => {
            // Switch to model by alias
            let models_cfg = crate::model_manager::load();
            if let Some(resolved) = models_cfg.resolve(alias) {
                agent.switch_model(&resolved);
                println!(
                    "\n{}  Switched to '{}' → {} ({})",
                    "🔄",
                    alias.bright_yellow(),
                    agent.config.model.bright_white(),
                    agent.config.provider.to_string().bright_cyan()
                );
            } else {
                println!(
                    "\n{}  Model '{}' not found. Use {} to see available models.",
                    "❓",
                    alias.bright_red(),
                    "/model".bright_white()
                );
            }
        }
    }
}

/// Small helper to prompt a line from stdin (used by /model add).
fn prompt_line(prompt: &str) -> String {
    use std::io::Write;
    print!("{}", prompt);
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    buf.trim().to_string()
}

/// Handle `/plan` command (async because planning calls the LLM).
///
/// - `/plan <task>`    — generate a plan for the task (read-only exploration, no execution)
/// - `/plan run`       — execute the pending plan
/// - `/plan show`      — display the pending plan again
/// - `/plan clear`     — discard the pending plan
pub async fn handle_plan_command(input: &str, agent: &mut Agent) {
    let subcommand = input.strip_prefix("/plan").unwrap_or("").trim();

    match subcommand {
        "" => {
            // No argument — show usage
            println!(
                "\n{}  {}",
                "📝",
                "Plan Mode — think first, execute later".bright_cyan().bold()
            );
            println!();
            println!("  {}  Generate a plan for a task", "/plan <task>".bright_white());
            println!("  {}       Execute the pending plan", "/plan run".bright_white());
            println!("  {}      Display the pending plan", "/plan show".bright_white());
            println!("  {}     Discard the pending plan", "/plan clear".bright_white());

            if agent.pending_plan.is_some() {
                println!(
                    "\n  {}  {}",
                    "💡",
                    "A pending plan exists. Use /plan show to view, /plan run to execute.".bright_green()
                );
            }
            println!();
        }
        "run" => {
            if let Some(plan) = agent.pending_plan.clone() {
                println!(
                    "\n{}  {}",
                    "🚀",
                    "Executing plan...".bright_cyan().bold()
                );
                println!("{}\n", "─".repeat(60).dimmed());
                match agent.execute_plan(&plan).await {
                    Ok(_) => {
                        auto_save_session(agent);
                    }
                    Err(e) => {
                        ui::print_error(&format!("Plan execution failed: {}", e));
                    }
                }
            } else {
                println!(
                    "\n{}  {}",
                    "⚠️",
                    "No pending plan. Use /plan <task> to generate one first.".yellow()
                );
            }
        }
        "show" => {
            if let Some(ref plan) = agent.pending_plan {
                println!(
                    "\n{}  {}:\n",
                    "📋",
                    "Pending Plan".bright_cyan().bold()
                );
                let skin = termimad::MadSkin::default();
                skin.print_text(plan);
                println!("\n{}", "─".repeat(60).dimmed());
                println!(
                    "  {} Use {} to execute or {} to discard.",
                    "💡",
                    "/plan run".bright_white(),
                    "/plan clear".bright_white()
                );
                println!();
            } else {
                println!(
                    "\n{}  {}",
                    "📋",
                    "No pending plan.".dimmed()
                );
            }
        }
        "clear" => {
            if agent.pending_plan.is_some() {
                agent.pending_plan = None;
                println!(
                    "\n{}  {}",
                    "🗑️",
                    "Pending plan cleared.".bright_cyan()
                );
            } else {
                println!(
                    "\n{}  {}",
                    "📋",
                    "No pending plan to clear.".dimmed()
                );
            }
        }
        task => {
            // Generate a plan for the given task
            println!(
                "\n{}  {}",
                "📝",
                "Generating plan (read-only exploration)...".bright_cyan()
            );
            println!("{}\n", "─".repeat(60).dimmed());

            match agent.generate_plan(task).await {
                Ok(plan) => {
                    println!("\n{}", "─".repeat(60).dimmed());
                    println!(
                        "\n{}  {}",
                        "✅",
                        "Plan generated and saved.".bright_green()
                    );
                    println!(
                        "  {} Use {} to execute or {} to view again.\n",
                        "💡",
                        "/plan run".bright_white(),
                        "/plan show".bright_white()
                    );
                    // Also save to memory for traceability
                    agent.memory.log_action(&format!(
                        "generated plan ({} chars)",
                        plan.len()
                    ));
                }
                Err(e) => {
                    ui::print_error(&format!("Failed to generate plan: {}", e));
                }
            }
        }
    }
}

/// Handle `/summary` command (async because generation calls the LLM).
///
/// - `/summary`            — show existing summary, or offer to generate
/// - `/summary generate`   — force (re-)generate the project summary
pub async fn handle_summary_command(input: &str, agent: &mut Agent) {
    let subcommand = input.strip_prefix("/summary").unwrap_or("").trim();
    let cwd = &agent.project_dir;

    match subcommand {
        "generate" => {
            // Force (re-)generate
            if crate::summary::exists(&cwd) {
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
            if let Some(summary) = crate::summary::load(&cwd) {
                println!("\n{}  {}:\n", "📋", "Project Summary".bright_cyan().bold());
                let skin = termimad::MadSkin::default();
                skin.print_text(&summary);
                println!();
                println!(
                    "  {} Run {} to regenerate.",
                    "💡".to_string().dimmed(),
                    "/summary generate".bright_white()
                );
            } else {
                println!(
                    "\n{}  No project summary found.",
                    "📋"
                );
                print!(
                    "  Generate one now? {} ",
                    "[y/N]".bright_white()
                );
                use std::io::Write;
                std::io::stdout().flush().ok();

                // Read a single line for confirmation
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
                "❓",
                other.bright_red(),
                "/summary".bright_white(),
                "/summary generate".bright_white()
            );
        }
    }
}

/// Handle `/rollback` — restore all files to their pre-sandbox state.
pub async fn handle_rollback_command(agent: &mut Agent) {
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

    // Show what will be rolled back
    let changes = agent.sandbox.changed_files().await;
    println!(
        "\n{}  {} tracked change(s) will be rolled back:",
        "⏪",
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

    // Confirm
    print!(
        "  {} ",
        "Proceed with rollback? [y/N]".bright_yellow()
    );
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
pub async fn handle_commit_command(agent: &mut Agent) {
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
pub async fn handle_changes_command(agent: &Agent) {
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
        if let Err(e) = persistence::save_local_session(&agent.conversation, &agent.project_dir) {
            tracing::warn!("Auto-save (local) failed: {}", e);
        }
    }
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

// ── /nodes command ────────────────────────────────────────────────────────────

/// Probe every `[[remote]]` entry in workspaces.toml, print hierarchical status
/// (physical server → virtual nodes with workdir/sandbox/tags), and populate the
/// in-process route table so that subsequent `any:<tag>` calls work immediately.
async fn handle_nodes_command(project_dir: &std::path::Path) {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let cfg = crate::workspaces::load(project_dir);
    let remotes = cfg.all_remotes();

    if remotes.is_empty() {
        println!(
            "\n{}",
            "📡  No [[remote]] entries in workspaces.toml.".bright_yellow()
        );
        println!(
            "  Add entries to {} or {}",
            ".agent/workspaces.toml".bright_yellow(),
            "~/.config/rust_agent/workspaces.toml".bright_yellow()
        );
        return;
    }

    println!("\n{}", "📡  Probing remote nodes...".bright_cyan());

    for remote in &remotes {
        // Use /probe path so the server handles this inline (no worker fork).
        let probe_base = crate::workspaces::with_path(&remote.url, "/probe");
        let url = if probe_base.contains('?') {
            format!("{}&discover=1", probe_base)
        } else {
            format!("{}?discover=1", probe_base)
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
                            let base_url = remote.url.splitn(2, '?').next().unwrap_or(&remote.url);
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
