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
        "write_file" => "✏️",
        "edit_file" => "🔧",
        "run_command" => "⚡",
        "grep_search" => "🔍",
        "file_search" => "📁",
        "list_directory" => "📂",
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
        _ => {
            println!("   {} {}", "Input:".dimmed(), input);
        }
    }
}

/// Print tool result
pub fn print_tool_result(_name: &str, result: &ToolResult) {
    if result.is_error {
        println!(
            "   {} {}",
            "❌ Error:".red().bold(),
            truncate_output(&result.output, 500).red()
        );
    } else {
        // Show a compact version of the result
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

/// Truncate long output for display
fn truncate_output(output: &str, max_chars: usize) -> String {
    if output.len() <= max_chars {
        output.to_string()
    } else {
        let half = max_chars / 2;

        // Find safe character boundaries
        let mut start_idx = half;
        while start_idx > 0 && !output.is_char_boundary(start_idx) {
            start_idx -= 1;
        }

        let mut end_idx = output.len() - half;
        while end_idx < output.len() && !output.is_char_boundary(end_idx) {
            end_idx += 1;
        }

        format!(
            "{}... ({} bytes truncated) ...{}",
            &output[..start_idx],
            end_idx - start_idx,
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
