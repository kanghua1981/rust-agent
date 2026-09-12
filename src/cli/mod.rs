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
    // Ctrl-\ (SIGQUIT) → guidance flag consumed by the loop between iterations
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


mod commands;
use commands::*;
