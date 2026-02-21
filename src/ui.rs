//! Terminal UI output - banners, tool display, warnings, help text.

use colored::*;

use crate::tools::ToolResult;

/// Print the welcome banner
pub fn print_banner() {
    let banner = r#"
╔══════════════════════════════════════════════════════════╗
║                                                          ║
║   🤖  Rust Coding Agent  v0.2                            ║
║   An AI-powered CLI coding assistant                     ║
║                                                          ║
║   Type /help for commands, /quit to exit                 ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝
"#;
    println!("{}", banner.bright_cyan());
}

/// Print working directory info
pub fn print_workdir() {
    if let Ok(cwd) = std::env::current_dir() {
        println!(
            "{}  Working directory: {}\n",
            "📂",
            cwd.display().to_string().bright_blue()
        );
    }
}

/// Print that the agent is thinking
pub fn print_thinking() {
    print!("\r{}", "⏳ Thinking...".dimmed());
    use std::io::Write;
    std::io::stdout().flush().ok();
}

/// Print assistant's text response (for non-streaming providers)
pub fn print_assistant_text(text: &str) {
    println!("\n{}", "─".repeat(60).dimmed());
    // Use termimad for markdown rendering
    let skin = termimad::MadSkin::default();
    skin.print_text(text);
    println!("{}", "─".repeat(60).dimmed());
}

/// Print tool usage
pub fn print_tool_use(name: &str, input: &serde_json::Value) {
    let icon = match name {
        "read_file" => "📖",
        "batch_read_files" => "📚",
        "write_file" => "✏️",
        "edit_file" => "🔧",
        "multi_edit_file" => "🔧",
        "run_command" => "⚡",
        "grep_search" => "🔍",
        "file_search" => "📁",
        "list_directory" => "📂",
        "read_pdf" => "📄",
        "think" => "💭",
        "fetch_url" => "🌐",
        "read_ebook" => "📕",
        "load_skill" => "🎓",
        "create_skill" => "📝",
        _ => "🔨",
    };

    println!(
        "\n{} {} {}",
        icon,
        "Tool:".yellow().bold(),
        name.yellow()
    );

    // Print a concise summary of the input
    match name {
        "read_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                println!("   {} {}", "Path:".dimmed(), path.bright_white());
            }
        }
        "write_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                let lines = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|c| c.lines().count())
                    .unwrap_or(0);
                println!(
                    "   {} {} ({} lines)",
                    "Path:".dimmed(),
                    path.bright_white(),
                    lines
                );
            }
        }
        "edit_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                println!("   {} {}", "Path:".dimmed(), path.bright_white());
            }
        }
        "run_command" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                println!("   {} {}", "$".dimmed(), cmd.bright_white());
            }
        }
        "grep_search" | "file_search" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                println!("   {} {}", "Pattern:".dimmed(), pattern.bright_white());
            }
        }
        "list_directory" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            println!("   {} {}", "Path:".dimmed(), path.bright_white());
        }
        "multi_edit_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                let count = input
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!(
                    "   {} {} ({} edits)",
                    "Path:".dimmed(),
                    path.bright_white(),
                    count
                );
            }
        }
        "batch_read_files" => {
            if let Some(paths) = input.get("paths").and_then(|v| v.as_array()) {
                println!("   {} {} files", "Reading:".dimmed(), paths.len());
                for p in paths.iter().take(5) {
                    if let Some(s) = p.as_str() {
                        println!("   {} {}", "•".dimmed(), s.bright_white());
                    }
                }
                if paths.len() > 5 {
                    println!("   {} ... and {} more", "•".dimmed(), paths.len() - 5);
                }
            }
        }
        "read_pdf" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                let pages = match (input.get("start_page").and_then(|v| v.as_u64()),
                                   input.get("end_page").and_then(|v| v.as_u64())) {
                    (Some(s), Some(e)) => format!(" (pages {}-{})", s, e),
                    (Some(s), None) => format!(" (from page {})", s),
                    _ => String::new(),
                };
                println!("   {} {}{}", "Path:".dimmed(), path.bright_white(), pages);
            }
        }
        "think" => {
            if let Some(thought) = input.get("thought").and_then(|v| v.as_str()) {
                let preview = truncate_str(thought, 100);
                println!("   {}", preview.dimmed());
            }
        }
        "fetch_url" => {
            if let Some(url) = input.get("url").and_then(|v| v.as_str()) {
                println!("   {} {}", "URL:".dimmed(), url.bright_white());
            }
        }
        "read_ebook" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                println!("   {} {}", "Path:".dimmed(), path.bright_white());
            }
        }
        "load_skill" => {
            if let Some(name) = input.get("name").and_then(|v| v.as_str()) {
                println!("   {} {}", "Skill:".dimmed(), name.bright_white());
            }
        }
        "create_skill" => {
            if let Some(name) = input.get("name").and_then(|v| v.as_str()) {
                let desc = input.get("description").and_then(|v| v.as_str()).unwrap_or("");
                println!("   {} {} — {}", "Skill:".dimmed(), name.bright_white(), desc);
            }
        }
        _ => {
            println!("   {} {}", "Input:".dimmed(), input);
        }
    }
}

/// Print tool result
pub fn print_tool_result(name: &str, result: &ToolResult) {
    // Commands deserve more visible output so the user can see build logs, errors, etc.
    let is_command = name == "run_command";

    if result.is_error {
        let limit = if is_command { 5000 } else { 500 };
        println!(
            "   {} {}",
            "❌ Error:".red().bold(),
            truncate_output(&result.output, limit).red()
        );
    } else if is_command {
        // Show command output prominently (up to 5000 chars)
        let output = truncate_output(&result.output, 5000);
        for line in output.lines() {
            println!("   {}", line);
        }
    } else {
        // Show a compact version of the result for other tools
        let output = truncate_output(&result.output, 300);
        println!("   {} {}", "✅", output.dimmed());
    }
}

/// Print a warning message
pub fn print_warning(msg: &str) {
    println!("\n{} {}", "⚠️  Warning:".yellow().bold(), msg.yellow());
}

/// Print an error message
pub fn print_error(msg: &str) {
    println!("\n{} {}", "❌ Error:".red().bold(), msg.red());
}

/// Print token usage
pub fn print_usage(input_tokens: u64, output_tokens: u64) {
    println!("\n{}", "📊 Token Usage:".bright_cyan().bold());
    println!(
        "   Input tokens:  {}",
        input_tokens.to_string().bright_white()
    );
    println!(
        "   Output tokens: {}",
        output_tokens.to_string().bright_white()
    );
    println!(
        "   Total tokens:  {}",
        (input_tokens + output_tokens).to_string().bright_white()
    );
}

/// Print context window warning
pub fn print_context_warning(usage_percent: f32, estimated: usize, max: usize) {
    println!(
        "\n{}  Context window at {:.0}% ({}/{} est. tokens). Truncating old messages...",
        "⚠️ ".yellow().bold(),
        usage_percent,
        format_number(estimated).bright_yellow(),
        format_number(max).dimmed()
    );
}

/// Print context window status
pub fn print_context_status(estimated: usize, max: usize, usage_percent: f32, msg_count: usize) {
    println!("\n{}", "📊 Context Window:".bright_cyan().bold());

    // Color the bar based on usage
    let bar_width = 30;
    let filled = ((usage_percent / 100.0) * bar_width as f32) as usize;
    let empty = bar_width - filled;

    let bar_color = if usage_percent > 80.0 {
        "red"
    } else if usage_percent > 60.0 {
        "yellow"
    } else {
        "green"
    };

    let filled_str = "█".repeat(filled);
    let empty_str = "░".repeat(empty);

    let bar = match bar_color {
        "red" => format!("{}{}", filled_str.red(), empty_str.dimmed()),
        "yellow" => format!("{}{}", filled_str.yellow(), empty_str.dimmed()),
        _ => format!("{}{}", filled_str.green(), empty_str.dimmed()),
    };

    println!("   [{}] {:.1}%", bar, usage_percent);
    println!(
        "   Est. tokens:  {} / {}",
        format_number(estimated).bright_white(),
        format_number(max).dimmed()
    );
    println!(
        "   Messages:     {}",
        msg_count.to_string().bright_white()
    );
}

/// Print help
pub fn print_help() {
    println!("\n{}", "Available Commands:".bright_cyan().bold());
    println!("  {}     - Show this help message", "/help".bright_white());
    println!(
        "  {}    - Clear conversation history",
        "/clear".bright_white()
    );
    println!(
        "  {}    - Show token usage statistics",
        "/usage".bright_white()
    );
    println!(
        "  {}  - Show context window status",
        "/context".bright_white()
    );
    println!(
        "  {}     - Save current session",
        "/save".bright_white()
    );
    println!(
        "  {} - List saved sessions",
        "/sessions".bright_white()
    );
    println!(
        "  {}   - Skip all confirmations",
        "/yesall".bright_white()
    );
    println!(
        "  {}  - Re-enable confirmations",
        "/confirm".bright_white()
    );
    println!(
        "  {}   - List loaded project skills",
        "/skills".bright_white()
    );
    println!(
        "  {}   - Show persistent memory",
        "/memory".bright_white()
    );
    println!(
        "  {}  - View or generate project summary",
        "/summary".bright_white()
    );
    println!(
        "  {} - Plan before executing (plan/run/show/clear)",
        "/plan".bright_white()
    );
    println!(
        "  {}     - Exit the agent",
        "/quit".bright_white()
    );
    println!();
    println!("{}", "CLI Flags:".bright_cyan().bold());
    println!(
        "  {}           - Skip all confirmations",
        "--yes / -y".bright_white()
    );
    println!(
        "  {}   - Resume a saved session",
        "--resume <ID>".bright_white()
    );
    println!(
        "  {}     - List saved sessions",
        "--sessions".bright_white()
    );
    println!();
    println!("{}", "Tips:".bright_cyan().bold());
    println!("  • Ask me to read, write, or edit files");
    println!("  • Ask me to run terminal commands");
    println!("  • Ask me to search through your codebase");
    println!("  • Ask me to explain, refactor, or debug code");
    println!("  • File writes & commands require confirmation (use /yesall to skip)");
    println!();
}

/// Truncate a string to at most `max_chars` characters (not bytes),
/// appending "..." if truncated. Safe for multi-byte UTF-8 (CJK, emoji, etc.).
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    let mut char_iter = s.char_indices();
    let boundary = char_iter.nth(max_chars).map(|(idx, _)| idx);
    match boundary {
        Some(idx) => format!("{}...", &s[..idx]),
        None => s.to_string(),
    }
}

/// Truncate long output for display
fn truncate_output(output: &str, max_chars: usize) -> String {
    if output.chars().count() <= max_chars {
        output.to_string()
    } else {
        let half = max_chars / 2;

        // Find safe start boundary (end of first half)
        let start_idx = output
            .char_indices()
            .nth(half)
            .map(|(idx, _)| idx)
            .unwrap_or(output.len());

        // Find safe end boundary (start of last half)
        let total_chars = output.chars().count();
        let end_idx = output
            .char_indices()
            .nth(total_chars - half)
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        format!(
            "{}... ({} chars truncated) ...{}",
            &output[..start_idx],
            total_chars - max_chars,
            &output[end_idx..]
        )
    }
}

/// Format a number with thousands separators
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Print that the agent is generating a project summary
pub fn print_summary_generating() {
    println!(
        "\n{}  {}",
        "📝",
        "Generating project summary...".bright_cyan()
    );
}

/// Print that the project summary has been saved
pub fn print_summary_done() {
    println!(
        "{}  {}\n",
        "✅",
        "Project summary saved to .agent/summary.md".bright_green()
    );
}

/// Print that an existing project summary was loaded
pub fn print_summary_loaded() {
    println!(
        "{}  {}\n",
        "📋",
        "Project summary loaded from .agent/summary.md".dimmed()
    );
}

/// Print a hint when no project summary exists
pub fn print_summary_hint() {
    println!(
        "{}  {}\n",
        "💡",
        "No project summary found. Run /summary to generate one.".dimmed()
    );
}
