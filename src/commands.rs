//! Central registry of slash commands.
//!
//! This is the **single source of truth** for every slash command's name,
//! argument hint, and description.  Both the CLI (`ui::print_help`) and the
//! TUI (`handle_tui_slash /help`) iterate over [`ALL_COMMANDS`] to build
//! their help output — so adding a new command only requires one edit here.
//!
//! Dispatch logic still lives in `cli.rs` and `tui_app.rs` because async
//! function pointers in Rust are cumbersome; but at least the help text
//! never drifts out of sync.

/// Metadata for one slash command.
pub struct CommandMeta {
    /// Primary command name, e.g. `"/nodes"`.
    pub name: &'static str,
    /// Optional argument hint shown after the name, e.g. `"[alias]"` or `"<task>"`.
    /// Use `""` when the command takes no arguments.
    pub args: &'static str,
    /// One-line description for help display.
    pub description: &'static str,
}

impl CommandMeta {
    const fn new(
        name: &'static str,
        args: &'static str,
        description: &'static str,
    ) -> Self {
        Self { name, args, description }
    }

    /// Returns the display label: `"/cmd"` or `"/cmd <args>"`.
    pub fn label(&self) -> String {
        if self.args.is_empty() {
            self.name.to_string()
        } else {
            format!("{} {}", self.name, self.args)
        }
    }
}

/// All slash commands, in display order.
///
/// Keep entries grouped logically: conversation → session → models/mode →
/// workspace/nodes → sandbox → misc.
pub const ALL_COMMANDS: &[CommandMeta] = &[
    // ── Conversation ─────────────────────────────────────────────────────────
    CommandMeta::new("/help",    "",                           "Show this help message"),
    CommandMeta::new("/clear",   "",                           "Clear conversation history"),
    CommandMeta::new("/usage",   "",                           "Show token usage statistics"),
    CommandMeta::new("/context", "",                           "Show context window status"),
    CommandMeta::new("/memory",  "",                           "Show persistent memory"),
    CommandMeta::new("/skills",  "",                           "List loaded project skills"),
    // ── Session ──────────────────────────────────────────────────────────────
    CommandMeta::new("/save",     "",                          "Save current session"),
    CommandMeta::new("/sessions", "",                          "List saved sessions"),
    CommandMeta::new("/export",   "[file]",                    "Export conversation to Markdown"),
    // ── Models / mode ────────────────────────────────────────────────────────
    CommandMeta::new("/model",    "[alias|add|remove|default]","List / switch / manage models"),
    CommandMeta::new("/mode",     "[simple|plan|pipeline|auto]","Set execution mode"),
    // ── Planning ─────────────────────────────────────────────────────────────
    CommandMeta::new("/summary",  "[generate]",                "View or generate project summary"),
    CommandMeta::new("/plan",     "<task>|run|show|clear",     "Explore & plan, then execute"),
    // ── Remote nodes ─────────────────────────────────────────────────────────
    CommandMeta::new("/nodes",    "",                          "Probe remote agent nodes (workspaces.toml [[peer]])"),
    // ── Sandbox ──────────────────────────────────────────────────────────────
    CommandMeta::new("/changes",  "",                          "Show sandbox-tracked file changes"),
    CommandMeta::new("/rollback", "",                          "Undo all sandbox changes"),
    CommandMeta::new("/commit",   "",                          "Accept all sandbox changes"),
    // ── Confirmations ────────────────────────────────────────────────────────
    CommandMeta::new("/yesall",  "",                           "Auto-approve all operations"),
    CommandMeta::new("/confirm", "",                           "Re-enable confirmations"),
    // ── Exit ─────────────────────────────────────────────────────────────────
    CommandMeta::new("/quit",    "",                           "Exit the agent"),
];
