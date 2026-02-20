//! User confirmation mechanism for dangerous operations.
//!
//! Before executing file writes, edits, or shell commands, the agent
//! asks for user confirmation to prevent accidental damage.

use colored::*;
use std::io::{self, Write};

/// Actions that require user confirmation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ConfirmAction {
    WriteFile { path: String, lines: usize },
    EditFile { path: String },
    RunCommand { command: String },
    DeleteFile { path: String },
}

/// Result of the confirmation prompt
#[derive(Debug, PartialEq)]
pub enum ConfirmResult {
    Yes,
    No,
    AlwaysYes,  // Skip future confirmations for this session
}

/// Global flag to skip confirmations (set by --yes flag or /yesall command)
static AUTO_APPROVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set auto-approve mode (skip all confirmations)
pub fn set_auto_approve(value: bool) {
    AUTO_APPROVE.store(value, std::sync::atomic::Ordering::Relaxed);
}

/// Check if auto-approve is enabled
pub fn is_auto_approve() -> bool {
    AUTO_APPROVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Ask the user to confirm a dangerous action.
/// Returns true if the action should proceed.
pub fn confirm(action: &ConfirmAction) -> ConfirmResult {
    if is_auto_approve() {
        // Print a brief note so the user knows confirmation was skipped
        let desc = match action {
            ConfirmAction::WriteFile { path, .. } => format!("write {}", path),
            ConfirmAction::EditFile { path } => format!("edit {}", path),
            ConfirmAction::RunCommand { command } => {
                let short = crate::ui::truncate_str(command, 50);
                format!("run `{}`", short)
            }
            ConfirmAction::DeleteFile { path } => format!("delete {}", path),
        };
        println!(
            "   {} {} {}",
            "⚡",
            "auto-approved:".dimmed(),
            desc.dimmed()
        );
        return ConfirmResult::Yes;
    }

    // Print what the action will do
    match action {
        ConfirmAction::WriteFile { path, lines } => {
            println!(
                "\n{}  {} {} ({} lines)",
                "📝",
                "Write file:".yellow().bold(),
                path.bright_white(),
                lines
            );
        }
        ConfirmAction::EditFile { path } => {
            println!(
                "\n{}  {} {}",
                "🔧",
                "Edit file:".yellow().bold(),
                path.bright_white()
            );
        }
        ConfirmAction::RunCommand { command } => {
            println!(
                "\n{}  {} {}",
                "⚡",
                "Run command:".yellow().bold(),
                command.bright_white()
            );
        }
        ConfirmAction::DeleteFile { path } => {
            println!(
                "\n{}  {} {}",
                "🗑️",
                "Delete file:".red().bold(),
                path.bright_white()
            );
        }
    }

    // Ask for confirmation
    print!(
        "   {} {}",
        "Proceed?".bright_cyan().bold(),
        "[y/n/a(always)] ".dimmed()
    );
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return ConfirmResult::No;
    }

    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => ConfirmResult::Yes,
        "a" | "always" | "yesall" => {
            set_auto_approve(true);
            println!("   {} {}", "✅", "Auto-approve enabled for this session".bright_green());
            ConfirmResult::AlwaysYes
        }
        _ => {
            println!("   {} {}", "⏭️", "Skipped".dimmed());
            ConfirmResult::No
        }
    }
}

/// Convenience function: confirm and return bool
pub fn should_proceed(action: &ConfirmAction) -> bool {
    let result = confirm(action);
    matches!(result, ConfirmResult::Yes | ConfirmResult::AlwaysYes)
}
