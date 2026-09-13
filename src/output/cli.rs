use super::*;

// ═══════════════════════════════════════════════════════════════════
//  CLI output — wraps existing ui::*, confirm::*, diff::* functions
// ═══════════════════════════════════════════════════════════════════

/// Terminal (CLI) output: colored text, interactive confirmation, diffs.
pub struct CliOutput;

impl CliOutput {
    pub fn new() -> Self {
        CliOutput
    }
}

#[async_trait::async_trait]
impl AgentOutput for CliOutput {
    fn on_thinking(&self) {
        crate::ui::print_thinking();
    }

    fn on_thinking_start(&self) {
        use colored::Colorize;
        print!("\r{}", " ".repeat(20)); // clear the spinner line
        print!("\r{}  ", "💭".dimmed());
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    fn on_thinking_token(&self, token: &str) {
        use colored::Colorize;
        use std::io::Write;
        print!("{}", token.dimmed());
        std::io::stdout().flush().ok();
    }

    fn on_thinking_end(&self) {
        use colored::Colorize;
        println!("\n{}", "─ ─ ─".dimmed());
    }

    fn on_role_header(&self, label: &str, model: &str) {
        crate::ui::print_role_header(label, model);
    }

    fn on_stage_end(&self, label: &str) {
        crate::ui::print_stage_end(label);
    }

    fn on_assistant_text(&self, text: &str) {
        crate::ui::print_assistant_text(text);
    }

    fn on_streaming_text(&self, token: &str) {
        use std::io::Write;
        print!("{}", token);
        std::io::stdout().flush().ok();
    }

    fn on_stream_start(&self) {
        println!("\n{}", "─".repeat(60));
    }

    fn on_stream_end(&self) {
        println!("\n{}", "─".repeat(60));
    }

    fn on_tool_use(&self, name: &str, input: &serde_json::Value, _tool_id: &str) {
        crate::ui::print_tool_use(name, input);
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        crate::ui::print_tool_result(name, result);
    }

    fn on_diff(&self, path: &str, old: &str, new: &str) {
        crate::diff::print_diff(path, old, new);
    }

    fn confirm(&self, action: &ConfirmAction) -> crate::confirm::ConfirmResult {
        crate::confirm::confirm(action)
    }

    fn ask_user(&self, question: &str) -> String {
        use std::io::{self, Write};
        use colored::Colorize;
        println!("\n{}  {}", "❓", question.bright_cyan());
        print!("   {} ", "Your answer:".bright_white().bold());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        input.trim().to_string()
    }

    fn review_plan(&self, plan_text: &str) -> PlanReview {
        use std::io::{self, Write};
        use colored::Colorize;
        println!("\n{}  {}", "📋", "Plan:".yellow().bold());
        println!("{}", "─".repeat(60));
        // Print the plan with termimad (or plain text)
        println!("{}", plan_text);
        println!("{}", "─".repeat(60));
        println!(
            "   {} {}",
            "Review:".bright_cyan().bold(),
            "[y] approve  [n] reject  [type feedback to refine]".dimmed()
        );
        // Flush any keystrokes that were buffered while the plan was streaming
        // (e.g. an accidental Enter press), so we read fresh user intent only.
        #[cfg(unix)]
        unsafe {
            libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
        }
        print!("   {} ", ">".bright_white());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let trimmed = input.trim();
        match trimmed.to_lowercase().as_str() {
            "y" | "yes" => {
                // Offer a one-time chance to add background context before execution.
                println!(
                    "   {} {}",
                    "Context:".bright_cyan(),
                    "add background info for the executor (Enter to skip)".dimmed()
                );
                #[cfg(unix)]
                unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH); }
                print!("   {} ", ">".bright_white());
                io::stdout().flush().ok();
                let mut ctx = String::new();
                io::stdin().read_line(&mut ctx).ok();
                let ctx = ctx.trim().to_string();
                if ctx.is_empty() {
                    PlanReview::Approve
                } else {
                    PlanReview::ApproveWithContext(ctx)
                }
            }
            "n" | "no" => PlanReview::Reject,
            _ if trimmed.is_empty() => PlanReview::Reject,
            _ => PlanReview::Refine(trimmed.to_string()),
        }
    }

    fn inject_guidance(&self) -> Option<String> {
        use std::io::{self, Write};
        use colored::Colorize;
        // Start on a fresh line after any streaming output
        println!();
        println!(
            "{}  {} {}",
            "⚡",
            "Guidance:".yellow().bold(),
            "type a note for the executor (or press Enter to continue)".dimmed()
        );
        #[cfg(unix)]
        unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH); }
        print!("   {} ", ">".bright_white());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let text = input.trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }

    fn on_warning(&self, msg: &str) {
        crate::ui::print_warning(msg);
    }

    fn on_error(&self, msg: &str) {
        crate::ui::print_error(msg);
    }

    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize) {
        crate::ui::print_context_warning(usage_percent, estimated, max);
    }

    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        use colored::Colorize;
        use std::io::Write;
        let prefix = format!("[sub:{}]", task_id).cyan().bold().to_string();
        match event {
            SubAgentOutputEvent::StreamStart => {}
            SubAgentOutputEvent::StreamEnd   => { println!(); }
            SubAgentOutputEvent::Token(t) => {
                print!("{}", t);
                std::io::stdout().flush().ok();
            }
            SubAgentOutputEvent::ToolUse { name } => {
                println!("  {} ⚙  {}", prefix, name.bright_white());
            }
            SubAgentOutputEvent::ToolDone { name, is_error } => {
                if *is_error {
                    println!("  {} ✗  {}", prefix, name.red());
                } else {
                    println!("  {} ✓  {}", prefix, name.green());
                }
            }
            SubAgentOutputEvent::Done(text) => {
                let preview = crate::ui::truncate_str(text, 100);
                println!("  {} ✅ {}", prefix, preview.dimmed());
            }
            SubAgentOutputEvent::Error(msg) => {
                println!("  {} ❌ {}", prefix, msg.red());
            }
        }
    }

    fn on_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        use colored::Colorize;
        let (icon, msg_colored) = match level {
            NotifyLevel::Info    => ("ℹ", message.white().to_string()),
            NotifyLevel::Warning => ("⚠", message.yellow().to_string()),
            NotifyLevel::Alert   => ("🔔", message.red().bold().to_string()),
        };
        println!("  {} {} {}", format!("[{}]", source).magenta().bold(), icon, msg_colored);
    }
}
