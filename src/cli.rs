/// CLI module - handles interactive REPL and slash commands.

use std::sync::Arc;
use std::path::PathBuf;
use anyhow::Result;

use crate::agent::Agent;
use crate::config::Config;
use crate::output::AgentOutput;
use crate::container::IsolationMode;
use crate::sandbox::Sandbox;

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
        _ => {
            println!("\n🔌  Plugin command usage:");
            println!("  /plugin list          - list all plugins");
            println!("  /plugin enable <name> - enable a plugin");
            println!("  /plugin disable <name>- disable a plugin");
            println!("  /plugin info <name>   - show plugin information");
            println!("  /plugin tools         - list plugin tools");
        }
    }
}

/// List all saved sessions and exit
pub fn list_sessions_and_exit() -> Result<()> {
    use crate::persistence;
    
    match persistence::list_sessions() {
        Ok(sessions) if sessions.is_empty() => {
            println!("No saved sessions found.");
        }
        Ok(sessions) => {
            println!("\n📋  Saved Sessions:");
            for (i, session) in sessions.iter().enumerate() {
                println!("  {}. {} - {}", i + 1, session.id, session.updated_at);
            }
            println!("\nUse --resume <session-id> to resume a session.");
        }
        Err(e) => {
            eprintln!("Failed to list sessions: {}", e);
        }
    }
    std::process::exit(0);
}

/// Auto-save the current session
pub fn auto_save_session(agent: &mut Agent) {
    use crate::persistence;
    
    if let Some(session_id) = agent.session_id() {
        // Save to global session store
        let session_id_str = session_id.to_string();
        match persistence::save_session(&agent.conversation, Some(&session_id_str), &agent.project_dir) {
            Ok(id) => {
                tracing::info!("Session saved (global): {}", id);
            }
            Err(e) => {
                tracing::warn!("Failed to save global session: {}", e);
            }
        }
    }
    
    // Also save to local session file
    match persistence::save_local_session(&agent.conversation, &agent.project_dir) {
        Ok(()) => {
            tracing::info!("Session saved to .agent/session.json");
        }
        Err(e) => {
            tracing::warn!("Failed to save local session: {}", e);
        }
    }
}

/// Main CLI run function - handles the interactive REPL loop
pub async fn run(
    config: Config,
    project_dir: PathBuf,
    initial_prompt: Option<String>,
    resume_session: Option<String>,
    output: Arc<dyn AgentOutput>,
    isolation: IsolationMode,
    global_session: bool,
    plugin_manager: Option<Arc<tokio::sync::Mutex<crate::plugin::PluginManager>>>,
) -> Result<()> {
    use crate::persistence;
    
    // Create sandbox
    let sandbox = match isolation {
        IsolationMode::Normal => Sandbox::disabled(&project_dir),
        IsolationMode::Container => Sandbox::new(&project_dir),
        IsolationMode::Sandbox => {
            let sb = Sandbox::new(&project_dir);
            if sb.is_disabled {
                tracing::warn!("Sandbox mode requested but fuse-overlayfs unavailable, falling back to container mode");
            }
            sb
        }
    };
    
    // Create agent
    let mut agent = if let Some(session_id) = &resume_session {
        // Try to load existing session
        match persistence::load_session(session_id) {
            Ok(session) => {
                let conv = persistence::restore_conversation(&session);
                output.on_warning(&format!("Resumed session: {}", session_id));
                Agent::with_conversation(config, project_dir.clone(), conv, session_id.clone(), output.clone(), sandbox, plugin_manager.clone())
            }
            Err(e) => {
                output.on_warning(&format!("Failed to load session '{}': {}", session_id, e));
                Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone())
            }
        }
    } else {
        Agent::new(config, project_dir.clone(), output.clone(), sandbox, plugin_manager.clone())
    };
    
    agent.global_session = global_session;
    
    // Load MCP tools if configured
    agent.load_mcp_tools().await;
    
    // Load plugin tools
    if let Some(pm) = &plugin_manager {
        let mut pm_lock = pm.lock().await;
        if let Err(e) = pm_lock.load_all_plugins() {
            output.on_warning(&format!("Failed to load plugins: {}", e));
        }
    }
    
    // Load plugin tools into tool executor
    if let Err(e) = agent.load_plugin_tools().await {
        output.on_warning(&format!("Failed to load plugin tools: {}", e));
    }
    
    // Process initial prompt if provided
    if let Some(prompt) = &initial_prompt {
        match agent.process_message(prompt).await {
            Ok(response) => {
                output.on_assistant_text(&response);
            }
            Err(e) => {
                output.on_error(&format!("Error processing prompt: {}", e));
            }
        }
    }
    
    // Main REPL loop - use ask_user for input
    loop {
        // Get user input using ask_user
        let input = output.ask_user("> ");
        
        // Check for exit commands
        let input_trimmed = input.trim();
        if input_trimmed.is_empty() {
            continue;
        }
        
        // Handle slash commands
        if let Some(cmd) = input_trimmed.strip_prefix('/') {
            // Handle plugin command
            if cmd.starts_with("plugin") {
                let subcommand = cmd.strip_prefix("plugin").map(|s| s.trim()).unwrap_or("");
                handle_plugin_command(subcommand, &mut agent).await;
                continue;
            }
            
            // Handle other commands
            match cmd {
                "quit" | "exit" | "q" => {
                    auto_save_session(&mut agent);
                    output.on_warning("👋 Goodbye! Happy coding!");
                    break;
                }
                "help" | "h" => {
                    output.on_warning("Available commands: /plugin, /quit, /save, /help");
                    continue;
                }
                "save" => {
                    auto_save_session(&mut agent);
                    output.on_warning("Session saved.");
                    continue;
                }
                _ => {
                    output.on_warning(&format!("Unknown command: /{}", cmd));
                    continue;
                }
            }
        }
        
        // Process as normal message
        match agent.process_message(input_trimmed).await {
            Ok(response) => {
                output.on_assistant_text(&response);
            }
            Err(e) => {
                output.on_error(&format!("Error: {}", e));
            }
        }
    }
    
    Ok(())
}
