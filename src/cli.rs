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
) -> Result<()> {
    ui::print_banner();
    ui::print_workdir();

    // Create or restore agent
    let mut agent = if let Some(ref session_id) = resume_id {
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
                Agent::with_conversation(config, project_dir.clone(), conversation, session.meta.id, output.clone())
            }
            Err(e) => {
                ui::print_error(&format!("Failed to resume session: {}", e));
                println!("Starting a new session instead.\n");
                Agent::new(config, project_dir.clone(), output.clone())
            }
        }
    } else {
        Agent::new(config, project_dir.clone(), output.clone())
    };

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

    loop {
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
                    let handled = handle_slash_command(input, &mut agent);
                    match handled {
                        SlashResult::Continue => continue,
                        SlashResult::Quit => break,
                        SlashResult::NotACommand => {} // fall through to process as message
                    }
                }

                // Process the user's message
                match agent.process_message(input).await {
                    Ok(_) => {
                        // Auto-save session after each interaction
                        auto_save_session(&mut agent);
                    }
                    Err(e) => {
                        ui::print_error(&format!("{:#}", e));
                    }
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
            ui::print_usage(input_tokens, output_tokens);
            SlashResult::Continue
        }
        "/save" => {
            match persistence::save_session(&agent.conversation, agent.session_id(), &agent.project_dir) {
                Ok(id) => {
                    agent.set_session_id(id.clone());
                    println!(
                        "\n{}  Session saved: {}",
                        "💾",
                        id.bright_yellow()
                    );
                }
                Err(e) => ui::print_error(&format!("Failed to save session: {}", e)),
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

/// Handle `/plan` command (async because planning calls the LLM).
///
/// - `/plan <task>`    — generate a plan for the task (read-only exploration, no execution)
/// - `/plan run`       — execute the pending plan
/// - `/plan show`      — display the pending plan again
/// - `/plan clear`     — discard the pending plan
async fn handle_plan_command(input: &str, agent: &mut Agent) {
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
async fn handle_summary_command(input: &str, agent: &mut Agent) {
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

/// Auto-save the session (silent, won't error to user)
fn auto_save_session(agent: &mut Agent) {
    if agent.conversation.messages.is_empty() {
        return;
    }
    match persistence::save_session(&agent.conversation, agent.session_id(), &agent.project_dir) {
        Ok(id) => {
            agent.set_session_id(id);
        }
        Err(e) => {
            tracing::warn!("Auto-save failed: {}", e);
        }
    }
}
