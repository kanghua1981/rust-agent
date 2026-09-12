use super::*;

/// Handle `/upload <source_path> [target_name]` command.
/// Reads a local file and copies it to the project's uploads/ directory.
pub(super) async fn handle_upload_command(args: &str, agent: &Agent) {
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
pub(super) async fn handle_plugin_command(subcommand: &str, agent: &mut Agent) {
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
/// Handle `/mode [simple|plan|auto]` command.
///
/// - `/mode`              — show current override (or "auto")
/// - `/mode simple`       — force the single-model loop (clears plan mode)
/// - `/mode plan`         — enter plan mode (read-only plan, then act)
/// - `/mode auto`         — clear override, default single-model loop
pub(super) fn handle_mode_command(input: &str, agent: &mut Agent) {
    use crate::router::ExecutionMode;

    let sub = input.strip_prefix("/mode").unwrap_or("").trim();

    match sub {
        "" => {
            let current = if agent.plan_mode {
                "plan".to_string()
            } else {
                match agent.force_mode {
                    Some(ExecutionMode::BasicLoop) => "simple (forced)".to_string(),
                    None => "auto".to_string(),
                }
            };
            println!("\n{}  Current execution mode: {}", "🔀", current.bright_white());
            println!("  Use {} to change:", "/mode <option>".bright_cyan());
            println!("    {}      — single-model loop, fast & cheap", "simple".bright_yellow());
            println!("    {}         — plan read-only, then act after approval", "plan".bright_yellow());
            println!("    {}        — let the agent decide (default)", "auto".bright_yellow());
            println!();
        }
        "simple" => {
            agent.set_force_mode(Some(ExecutionMode::BasicLoop));
            agent.set_plan_mode(false);
            println!("\n{}  Mode locked to {}: single-model loop for all messages.", "🔀", "simple".bright_green());
        }
        "plan" => {
            agent.set_force_mode(None);
            agent.set_plan_mode(true);
            println!("\n{}  Mode locked to {}: read-only plan, then act after approval.", "🔀", "plan".bright_green());
        }
        "auto" => {
            agent.set_force_mode(None);
            agent.set_plan_mode(false);
            println!("\n{}  Mode reset to {}: default single-model loop.", "🔀", "auto".bright_green());
        }
        other => {
            println!(
                "\n{}  Unknown mode: {}. Valid options: simple, plan, auto",
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
pub(super) fn handle_model_command(input: &str, agent: &mut Agent) {
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
pub(super) fn handle_endpoint_command(input: &str) {
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
pub(super) async fn handle_model_fetch_command(args: &str) {
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
pub(super) fn auto_endpoint_name(base_url: &str, source: &str) -> String {
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
/// Handle `/summary` command — generate or load project summary.
pub(super) async fn handle_summary_command(input: &str, agent: &mut Agent) {
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
pub(super) async fn handle_consolidate_command(agent: &mut Agent) {
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
pub(super) async fn handle_plan_command(input: &str, agent: &mut Agent) {
    let rest = input.strip_prefix("/plan").unwrap_or("").trim();
    if rest == "off" {
        agent.set_plan_mode(false);
        println!("\n{}  Exited plan mode. Next messages execute normally.", "📋");
        return;
    }
    if rest.is_empty() || rest == "on" {
        agent.set_plan_mode(true);
        println!(
            "\n{}  Entered plan mode. The next message is analyzed read-only; submit the plan via exit_plan_mode.",
            "📋"
        );
        return;
    }
    // With a task: enter plan mode and plan (read-only) before any execution.
    agent.set_plan_mode(true);
    println!("\n{}  Planning (read-only): {}\n", "🧠", rest.bright_cyan());
    match agent.process_message(rest).await {
        Ok(text) => println!("{}", text),
        Err(e) => println!("\n{}  {}", "⚠️", e),
    }
}

/// Handle `/rollback` — discard all sandbox changes (restore original).
pub(super) async fn handle_rollback_command(agent: &mut Agent) {
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
pub(super) async fn handle_commit_command(agent: &mut Agent) {
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
pub(super) async fn handle_changes_command(agent: &Agent) {
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
pub(super) fn probe_url_encode(s: &str) -> String {
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
pub(super) async fn handle_nodes_command(peers: &[crate::workspaces::PeerEntry], cluster_tok: Option<String>) {
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
