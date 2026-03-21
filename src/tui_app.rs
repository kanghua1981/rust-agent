//! TUI mode — ratatui split-screen interface.
//!
//! Layout:
//!   ┌────────────────────────────────────────┐
//!   │  scrollable output (all agent output)  │
//!   ├────────────────────────────────────────┤
//!   │  status bar (● thinking… / ✓ ready)   │
//!   ├────────────────────────────────────────┤
//!   │  > [input — always active]             │
//!   └────────────────────────────────────────┘
//!
//! The agent runs in a separate tokio task. Input and output are fully
//! decoupled: the user can type the next message while the agent is still
//! processing a previous one — it will be queued.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{mpsc as std_mpsc, Arc};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
    KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use tokio::sync::mpsc as async_mpsc;

use crate::agent::Agent;
use crate::confirm::{ConfirmAction, ConfirmResult};
use crate::config::Config;
use crate::output::{AgentOutput, NotifyLevel, PlanReview, SubAgentOutputEvent};
use crate::sandbox::Sandbox;
use crate::tools::ToolResult;

// ───────────────────────────────────────────────────────────────────────────────
// Constants
// ───────────────────────────────────────────────────────────────────────────────

/// Target render interval: ~60 fps.
const RENDER_INTERVAL_MS: u64 = 16;
/// Lines scrolled per mouse wheel tick.
const MOUSE_SCROLL_STEP: usize = 3;
/// Maximum number of output lines kept in memory.
/// Oldest lines are dropped when this limit is reached, preventing unbounded growth.
const MAX_OUTPUT_LINES: usize = 5_000;

// ─────────────────────────────────────────────────────────────────────────────
// Events: agent task → TUI render loop
// ─────────────────────────────────────────────────────────────────────────────

pub enum TuiEvent {
    /// A complete, fully-styled output line.
    Line(Line<'static>),
    /// Begin a new streaming line (shows "◀ " prefix, content accumulates).
    StreamStart,
    /// Append text to the current streaming line.
    StreamToken(String),
    /// Streaming finished; no more tokens for this turn.
    StreamEnd,
    /// Agent became busy (true) or finished (false).
    AgentBusy(bool),
    /// Synchronous confirmation request. The agent task blocks until reply.
    Confirm(ConfirmAction, std_mpsc::SyncSender<ConfirmResult>),
    /// Ask the user a free-form question (e.g. tool clarification).
    AskUser(String, std_mpsc::SyncSender<String>),
    /// Show the pipeline plan and wait for approve/reject/refine.
    ReviewPlan(String, std_mpsc::SyncSender<PlanReview>),
    /// Agent task exited (panic, error, or clean quit).
    AgentDied(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// TuiOutput — implements AgentOutput, feeds TuiEvent into the render loop
// ─────────────────────────────────────────────────────────────────────────────

pub struct TuiOutput {
    tx: async_mpsc::UnboundedSender<TuiEvent>,
}

impl TuiOutput {
    pub fn new(tx: async_mpsc::UnboundedSender<TuiEvent>) -> Self {
        Self { tx }
    }

    fn send(&self, ev: TuiEvent) {
        self.tx.send(ev).ok();
    }

    fn line(&self, spans: Vec<Span<'static>>) {
        self.send(TuiEvent::Line(Line::from(spans)));
    }
}

fn s(fg: Color) -> Style {
    Style::default().fg(fg)
}
fn b(fg: Color) -> Style {
    Style::default().fg(fg).add_modifier(Modifier::BOLD)
}

impl AgentOutput for TuiOutput {
    fn on_thinking(&self) {
        self.line(vec![
            Span::styled("🤔 ", s(Color::Yellow)),
            Span::styled("thinking...", s(Color::DarkGray)),
        ]);
    }

    fn on_role_header(&self, label: &str, model: &str) {
        self.line(vec![
            Span::styled(format!("── {} ", label), b(Color::Cyan)),
            Span::styled(format!("[{}]", model), s(Color::DarkGray)),
        ]);
    }

    fn on_stage_end(&self, label: &str) {
        self.line(vec![Span::styled(
            format!("── {} done ──", label),
            s(Color::DarkGray),
        )]);
    }

    fn on_assistant_text(&self, text: &str) {
        for ln in text.lines() {
            self.line(vec![Span::raw(ln.to_owned())]);
        }
        if text.is_empty() {
            self.line(vec![]);
        }
    }

    fn on_stream_start(&self) {
        self.send(TuiEvent::StreamStart);
    }

    fn on_streaming_text(&self, token: &str) {
        self.send(TuiEvent::StreamToken(token.to_owned()));
    }

    fn on_stream_end(&self) {
        self.send(TuiEvent::StreamEnd);
    }

    fn on_tool_use(&self, name: &str, input: &serde_json::Value) {
        let detail = match name {
            "read_file" | "write_file" | "edit_file" | "multi_edit_file" => {
                input.get("path").and_then(|v| v.as_str()).unwrap_or("").to_owned()
            }
            "run_command" => {
                input.get("command").and_then(|v| v.as_str()).unwrap_or("").to_owned()
            }
            "list_dir" => {
                input.get("path").and_then(|v| v.as_str()).unwrap_or("").to_owned()
            }
            _ => String::new(),
        };
        let icon = match name {
            "read_file" | "batch_read" => "📖",
            "write_file" | "edit_file" | "multi_edit_file" => "✏️ ",
            "run_command" => "🔨",
            "list_dir" => "📂",
            _ => "🔧",
        };
        self.line(vec![
            Span::styled(format!("{} ", icon), s(Color::Yellow)),
            Span::styled(name.to_owned(), b(Color::Yellow)),
            if !detail.is_empty() {
                Span::styled(format!("  {}", detail), s(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]);
    }

    fn on_tool_result(&self, name: &str, result: &ToolResult) {
        if result.is_error {
            self.line(vec![
                Span::styled("  ✗ ", b(Color::Red)),
                Span::styled(format!("{}: {}", name, result.output), s(Color::Red)),
            ]);
        }
        // Successful tool results are silent to avoid noise.
    }

    fn on_diff(&self, path: &str, _old: &str, _new: &str) {
        self.line(vec![
            Span::styled("  ✏️  ", s(Color::Blue)),
            Span::styled(path.to_owned(), b(Color::Blue)),
            Span::styled(" [modified]", s(Color::DarkGray)),
        ]);
    }

    fn confirm(&self, action: &ConfirmAction) -> ConfirmResult {
        if crate::confirm::is_auto_approve() {
            return ConfirmResult::Yes;
        }
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(0);
        self.send(TuiEvent::Confirm(action.clone(), reply_tx));
        // block_in_place is safe because the agent runs inside tokio::spawn
        // on a worker thread separate from the TUI render loop thread.
        tokio::task::block_in_place(|| reply_rx.recv().unwrap_or(ConfirmResult::No))
    }

    fn on_warning(&self, msg: &str) {
        self.line(vec![
            Span::styled("⚠  ", b(Color::Yellow)),
            Span::styled(msg.to_owned(), s(Color::Yellow)),
        ]);
    }

    fn on_error(&self, msg: &str) {
        self.line(vec![
            Span::styled("✗  ", b(Color::Red)),
            Span::styled(msg.to_owned(), s(Color::Red)),
        ]);
    }

    fn on_context_warning(&self, usage_percent: f32, estimated: usize, max: usize) {
        self.line(vec![
            Span::styled("⚠ context ", b(Color::Yellow)),
            Span::styled(
                format!("{:.0}%  ({} / {})", usage_percent, estimated, max),
                s(Color::Yellow),
            ),
        ]);
    }

    fn on_sub_agent_event(&self, task_id: &str, event: &SubAgentOutputEvent) {
        let text = match event {
            SubAgentOutputEvent::StreamStart => format!("[sub:{}] ▶", task_id),
            SubAgentOutputEvent::StreamEnd => format!("[sub:{}] ■", task_id),
            SubAgentOutputEvent::Token(t) => format!("[sub:{}] {}", task_id, t),
            SubAgentOutputEvent::ToolUse { name } => format!("[sub:{}] 🔧 {}", task_id, name),
            SubAgentOutputEvent::ToolDone { name, is_error } => {
                if *is_error {
                    format!("[sub:{}] ✗ {}", task_id, name)
                } else {
                    return;
                }
            }
            SubAgentOutputEvent::Done(s) => format!("[sub:{}] ✓ {}", task_id, s),
            SubAgentOutputEvent::Error(e) => format!("[sub:{}] ✗ {}", task_id, e),
        };
        self.line(vec![Span::styled(text, s(Color::Magenta))]);
    }

    fn ask_user(&self, question: &str) -> String {
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(0);
        self.send(TuiEvent::AskUser(question.to_owned(), reply_tx));
        tokio::task::block_in_place(|| reply_rx.recv().unwrap_or_default())
    }

    fn review_plan(&self, plan_text: &str) -> PlanReview {
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(0);
        self.send(TuiEvent::ReviewPlan(plan_text.to_owned(), reply_tx));
        tokio::task::block_in_place(|| reply_rx.recv().unwrap_or(PlanReview::Reject))
    }

    fn inject_guidance(&self) -> Option<String> {
        // In TUI mode guidance injection is not yet wired to a hotkey;
        // return None so the agent continues normally.
        None
    }

    fn on_service_notification(&self, source: &str, level: NotifyLevel, message: &str) {
        let (icon, color) = match level {
            NotifyLevel::Info => ("ℹ ", Color::White),
            NotifyLevel::Warning => ("⚠ ", Color::Yellow),
            NotifyLevel::Alert => ("🔔 ", Color::Red),
        };
        self.line(vec![Span::styled(
            format!("[svc:{}] {} {}", source, icon, message),
            s(color),
        )]);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TuiApp — owns the rendering state and handles all events
// ─────────────────────────────────────────────────────────────────────────────

struct TuiApp {
    // Output
    output_lines: Vec<Line<'static>>,
    scroll: usize,      // first visible line index (used when auto_scroll=false)
    auto_scroll: bool,  // when true, always show the newest lines
    stream_buf: String, // accumulates streaming tokens for the current turn
    is_streaming: bool,

    // Input
    input: Vec<char>, // char-based for correct CJK cursor handling
    cursor: usize,    // char index
    history: Vec<String>,
    hist_idx: Option<usize>,
    saved_input: String, // saved draft while navigating history

    // Queue / state
    input_queue: VecDeque<String>,
    agent_running: bool,
    confirm_pending: Option<(ConfirmAction, std_mpsc::SyncSender<ConfirmResult>)>,
    ask_pending: Option<std_mpsc::SyncSender<String>>,
    plan_pending: Option<std_mpsc::SyncSender<PlanReview>>,
    quit: bool,

    // Channels
    rx: async_mpsc::UnboundedReceiver<TuiEvent>,
    input_tx: async_mpsc::UnboundedSender<String>,
}

impl TuiApp {
    fn new(
        rx: async_mpsc::UnboundedReceiver<TuiEvent>,
        input_tx: async_mpsc::UnboundedSender<String>,
    ) -> Self {
        let welcome = Line::from(vec![
            Span::styled("🤖  Agent TUI  ", b(Color::Cyan)),
            Span::styled(
                "PgUp/PgDn·wheel: scroll   Ctrl-C: interrupt   Ctrl-Q: quit",
                s(Color::DarkGray),
            ),
        ]);
        let hint = Line::from(vec![Span::styled(
            "  💡 Shift+drag to select & copy text  (mouse capture is active)",
            s(Color::DarkGray),
        )]);
        Self {
            output_lines: vec![welcome, hint],
            scroll: 0,
            auto_scroll: true,
            stream_buf: String::new(),
            is_streaming: false,
            input: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            hist_idx: None,
            saved_input: String::new(),
            input_queue: VecDeque::new(),
            agent_running: false,
            confirm_pending: None,
            ask_pending: None,
            plan_pending: None,
            quit: false,
            rx,
            input_tx,
        }
    }

    // ── Output helpers ────────────────────────────────────────────────────────

    fn push_line(&mut self, line: Line<'static>) {
        // If a stream was in progress, the last output_lines entry already holds
        // the partial content — just reset state, don't discard the line.
        if self.is_streaming {
            self.is_streaming = false;
            self.stream_buf.clear();
        }
        self.output_lines.push(line);
        // Cap memory usage: drop the oldest lines when the limit is reached.
        // Trim in chunks of 10% to amortise the cost of Vec::drain.
        if self.output_lines.len() > MAX_OUTPUT_LINES {
            let drop_n = MAX_OUTPUT_LINES / 10;
            self.output_lines.drain(..drop_n);
            // Keep the scroll position pointing at the same logical line.
            self.scroll = self.scroll.saturating_sub(drop_n);
        }
        if self.auto_scroll {
            self.scroll = self.output_lines.len();
        }
    }

    fn handle_tui_event(&mut self, ev: TuiEvent) {
        match ev {
            TuiEvent::Line(line) => self.push_line(line),

            TuiEvent::StreamStart => {
                self.is_streaming = true;
                self.stream_buf.clear();
                // Push an empty streaming line; StreamToken will update it.
                self.output_lines.push(Line::from(vec![
                    Span::styled("◀ ", s(Color::Green)),
                ]));
                if self.auto_scroll {
                    self.scroll = self.output_lines.len();
                }
            }

            TuiEvent::StreamToken(token) => {
                if self.is_streaming {
                    self.stream_buf.push_str(&token);

                    // Split on newlines so multi-line LLM output is shown correctly.
                    // ratatui Line = single terminal row; embedded \n would be invisible.
                    if self.stream_buf.contains('\n') {
                        let mut segments: Vec<String> = self
                            .stream_buf
                            .split('\n')
                            .map(|s| s.to_owned())
                            .collect();
                        // The last segment is the new partial (possibly empty) streaming line.
                        let partial = segments.pop().unwrap_or_default();

                        // Finalize the first segment into the current streaming line.
                        if let Some(last) = self.output_lines.last_mut() {
                            let first = segments.remove(0);
                            *last = Line::from(vec![
                                Span::styled("◀ ", s(Color::Green)),
                                Span::raw(first),
                            ]);
                        }
                        // Push all complete middle lines.
                        for seg in segments {
                            self.output_lines.push(Line::from(vec![
                                Span::styled("  ", s(Color::Green)),
                                Span::raw(seg),
                            ]));
                        }
                        // Push a new streaming line for the remaining partial content.
                        self.output_lines.push(Line::from(vec![
                            Span::styled("◀ ", s(Color::Green)),
                            Span::raw(partial.clone()),
                        ]));
                        self.stream_buf = partial;
                    } else {
                        // No newline yet — update the current streaming line in place.
                        if let Some(last) = self.output_lines.last_mut() {
                            *last = Line::from(vec![
                                Span::styled("◀ ", s(Color::Green)),
                                Span::raw(self.stream_buf.clone()),
                            ]);
                        }
                    }

                    if self.auto_scroll {
                        self.scroll = self.output_lines.len();
                    }
                }
            }

            TuiEvent::StreamEnd => {
                // The last output_lines entry already has the correct final content
                // (stream_buf has no embedded \n after the fix above).
                // Just promote the last streaming line to a plain (non-◀) line so
                // it looks settled, then reset state.
                if self.is_streaming {
                    if let Some(last) = self.output_lines.last_mut() {
                        let text = self.stream_buf.clone();
                        *last = Line::from(vec![
                            Span::styled("  ", s(Color::DarkGray)),
                            Span::raw(text),
                        ]);
                    }
                }
                self.is_streaming = false;
                self.stream_buf.clear();
            }

            TuiEvent::AgentBusy(busy) => {
                self.agent_running = busy;
                if !busy {
                    // Dispatch the next queued message if any.
                    if let Some(msg) = self.input_queue.pop_front() {
                        self.dispatch(msg);
                    }
                }
            }

            TuiEvent::Confirm(action, reply_tx) => {
                let desc = match &action {
                    ConfirmAction::WriteFile { path, lines } => {
                        format!("write {} ({} lines)", path, lines)
                    }
                    ConfirmAction::EditFile { path } => format!("edit {}", path),
                    ConfirmAction::RunCommand { command } => format!("run `{}`", command),
                    ConfirmAction::DeleteFile { path } => format!("delete {}", path),
                    ConfirmAction::ReviewPlan { .. } => "execute pipeline plan".to_owned(),
                };
                self.push_line(Line::from(vec![
                    Span::styled("? Approve: ", b(Color::Yellow)),
                    Span::raw(desc),
                    Span::styled("  [y / n / a(lways)]", s(Color::DarkGray)),
                ]));
                self.confirm_pending = Some((action, reply_tx));
                self.input.clear();
                self.cursor = 0;
            }

            TuiEvent::AskUser(question, reply_tx) => {
                self.push_line(Line::from(vec![
                    Span::styled("❓ ", b(Color::Cyan)),
                    Span::raw(question),
                ]));
                self.ask_pending = Some(reply_tx);
                self.input.clear();
                self.cursor = 0;
            }

            TuiEvent::ReviewPlan(plan_text, reply_tx) => {
                self.push_line(Line::from(vec![Span::styled(
                    "─────────────── Pipeline Plan ───────────────".to_owned(),
                    b(Color::Cyan),
                )]));
                for ln in plan_text.lines() {
                    self.push_line(Line::from(vec![Span::raw(ln.to_owned())]));
                }
                self.push_line(Line::from(vec![Span::styled(
                    "[y] approve  [n] reject  [or type feedback to refine]".to_owned(),
                    s(Color::DarkGray),
                )]));
                self.plan_pending = Some(reply_tx);
                self.input.clear();
                self.cursor = 0;
            }

            TuiEvent::AgentDied(reason) => {
                self.agent_running = false;
                // Release any blocked confirm/ask/plan by dropping the senders.
                self.confirm_pending = None;
                self.ask_pending = None;
                self.plan_pending = None;
                self.input_queue.clear();
                self.push_line(Line::from(vec![
                    Span::styled("✗ agent task died: ", b(Color::Red)),
                    Span::styled(reason, s(Color::Red)),
                ]));
                self.push_line(Line::from(vec![Span::styled(
                    "  TUI still active — Ctrl-Q to quit.",
                    s(Color::DarkGray),
                )]));
            }
        }
    }

    // ── Input helpers ─────────────────────────────────────────────────────────

    fn dispatch(&mut self, msg: String) {
        self.agent_running = true;
        self.push_line(Line::from(vec![
            Span::styled("▶ ", b(Color::Blue)),
            Span::raw(msg.clone()),
        ]));
        self.input_tx.send(msg).ok();
    }

    fn submit(&mut self) {
        let text: String = self.input.iter().collect();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        if text != self.history.last().map(|s| s.as_str()).unwrap_or("") {
            self.history.push(text.clone());
        }
        self.hist_idx = None;
        self.saved_input.clear();
        self.input.clear();
        self.cursor = 0;

        // Handle slash commands that the TUI owns immediately in the render thread
        // (these never block on the agent task, so they respond even while agent is busy).
        match text.as_str() {
            "/quit" | "/exit" | "/q" => {
                self.quit = true;
                return;
            }
            "/clear" => {
                self.output_lines.clear();
                self.scroll = 0;
                return;
            }
            "/help" | "/h" => {
                let head = |t: &str| Line::from(vec![Span::styled(t.to_string(), b(Color::Cyan))]);
                let item = |label: &'static str, desc: &'static str| Line::from(vec![
                    Span::styled(format!("  {:22}", label), s(Color::White)),
                    Span::styled(desc, s(Color::DarkGray)),
                ]);
                self.push_line(head("── Available Commands ─────────────────────────────────────────"));
                self.push_line(item("/help",         "Show this help (instant, even while busy)"));
                self.push_line(item("/clear",        "Clear output"));
                self.push_line(item("/usage",        "Token usage statistics"));
                self.push_line(item("/context",      "Context window status"));
                self.push_line(item("/memory",       "Show agent memory"));
                self.push_line(item("/skills",       "List loaded skills"));
                self.push_line(item("/model",        "List / switch models"));
                self.push_line(item("/mode",         "Set execution mode: simple/plan/pipeline/auto"));
                self.push_line(item("/save",         "Save current session"));
                self.push_line(item("/sessions",     "List saved sessions"));
                self.push_line(item("/yesall",       "Auto-approve all operations"));
                self.push_line(item("/confirm",      "Re-enable confirmations"));
                self.push_line(item("/summary",      "Generate project summary"));
                self.push_line(item("/plan <task>",  "Step 1: explore & plan (read-only)"));
                self.push_line(item("/plan run",     "Step 2: execute the pending plan"));
                self.push_line(item("/rollback",     "Rollback sandbox changes"));
                self.push_line(item("/commit",       "Commit sandbox changes"));
                self.push_line(item("/changes",      "Show sandbox diff"));
                self.push_line(item("/export [file]", "Export chat to Markdown file"));
                self.push_line(item("/quit",         "Exit the TUI"));
                self.push_line(head("── Keyboard shortcuts ─────────────────────────────────────────"));
                self.push_line(item("PgUp / PgDn",  "Scroll output"));
                self.push_line(item("Mouse wheel",   "Scroll output"));
                self.push_line(item("↑ / ↓",        "Command history"));
                self.push_line(item("Ctrl-C",        "Interrupt agent"));
                self.push_line(item("Ctrl-Q",        "Quit"));
                return;
            }
            _ => {}
        }

        // Confirmation mode: interpret as y/n/a.
        if let Some((_, ref reply_tx)) = self.confirm_pending {
            let result = match text.to_lowercase().as_str() {
                "y" | "yes" => ConfirmResult::Yes,
                "a" | "always" => {
                    crate::confirm::set_auto_approve(true);
                    ConfirmResult::AlwaysYes
                }
                _ => ConfirmResult::No,
            };
            reply_tx.send(result).ok();
            self.confirm_pending = None;
            return;
        }

        // Free-form ask_user response.
        if let Some(ref reply_tx) = self.ask_pending {
            reply_tx.send(text).ok();
            self.ask_pending = None;
            return;
        }

        // Pipeline plan review.
        if let Some(ref reply_tx) = self.plan_pending {
            let review = match text.to_lowercase().as_str() {
                "y" | "yes" => PlanReview::Approve,
                "n" | "no" => PlanReview::Reject,
                other => PlanReview::Refine(other.to_owned()),
            };
            reply_tx.send(review).ok();
            self.plan_pending = None;
            return;
        }

        if self.agent_running {
            if text.starts_with('/') {
                // Slash commands always bypass the user-message queue.
                // They will execute in the agent task as soon as the current
                // LLM iteration ends — but they won't be stuck behind queued
                // user messages and won't be shown as "⏳ queued".
                self.input_tx.send(text).ok();
            } else {
                self.push_line(Line::from(vec![
                    Span::styled("⏳ queued  ", s(Color::DarkGray)),
                    Span::raw(text.clone()),
                ]));
                self.input_queue.push_back(text);
            }
        } else {
            self.dispatch(text);
        }
    }

    // ── Keyboard handling ─────────────────────────────────────────────────────

    /// Returns `true` if the app should quit.
    fn handle_key(&mut self, key: KeyEvent, out_h: u16) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => return true,
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if !self.agent_running {
                        return true;
                    }
                    crate::agent::request_interrupt();
                    self.push_line(Line::from(vec![Span::styled(
                        "⚡ interrupt requested",
                        b(Color::Red),
                    )]));
                    return false;
                }
                KeyCode::Char('l') | KeyCode::Char('L') => {
                    self.output_lines.clear();
                    self.scroll = 0;
                    return false;
                }
                _ => {}
            }
        }

        let step = (out_h.saturating_sub(2)) as usize;
        let total = self.output_lines.len();

        match key.code {
            KeyCode::Enter => self.submit(),

            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.input.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),

            KeyCode::Up => {
                if self.history.is_empty() {
                    return false;
                }
                let new_idx = match self.hist_idx {
                    None => {
                        self.saved_input = self.input.iter().collect();
                        self.history.len() - 1
                    }
                    Some(i) if i > 0 => i - 1,
                    Some(i) => i,
                };
                self.hist_idx = Some(new_idx);
                let chars: Vec<char> = self.history[new_idx].chars().collect();
                self.cursor = chars.len();
                self.input = chars;
            }
            KeyCode::Down => {
                match self.hist_idx {
                    None => {}
                    Some(i) => {
                        if i + 1 < self.history.len() {
                            let new_idx = i + 1;
                            self.hist_idx = Some(new_idx);
                            let chars: Vec<char> = self.history[new_idx].chars().collect();
                            self.cursor = chars.len();
                            self.input = chars;
                        } else {
                            self.hist_idx = None;
                            let chars: Vec<char> = self.saved_input.chars().collect();
                            self.cursor = chars.len();
                            self.input = chars;
                        }
                    }
                }
            }

            KeyCode::PageUp => {
                let first = if self.auto_scroll {
                    total.saturating_sub(out_h as usize)
                } else {
                    self.scroll
                };
                self.auto_scroll = false;
                self.scroll = first.saturating_sub(step);
            }
            KeyCode::PageDown => {
                let new_first = self.scroll.saturating_add(step);
                if new_first + out_h as usize >= total {
                    self.auto_scroll = true;
                } else {
                    self.auto_scroll = false;
                    self.scroll = new_first;
                }
            }

            KeyCode::Char(c) => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }

            _ => {}
        }
        false
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, out_h: u16) {
        let total = self.output_lines.len();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                let first = if self.auto_scroll {
                    total.saturating_sub(out_h as usize)
                } else {
                    self.scroll
                };
                self.auto_scroll = false;
                self.scroll = first.saturating_sub(MOUSE_SCROLL_STEP);
            }
            MouseEventKind::ScrollDown => {
                let new_first = self.scroll.saturating_add(MOUSE_SCROLL_STEP);
                if new_first + out_h as usize >= total {
                    // Reached the bottom — re-enable auto-scroll.
                    self.auto_scroll = true;
                } else {
                    self.auto_scroll = false;
                    self.scroll = new_first;
                }
            }
            _ => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render(f: &mut ratatui::Frame, app: &TuiApp) -> u16 {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // output pane
            Constraint::Length(1), // status bar
            Constraint::Length(3), // input area (border + 1 line + border-top only)
        ])
        .split(size);

    let out_area = chunks[0];
    let status_area = chunks[1];
    let input_area = chunks[2];
    let out_h = out_area.height as usize;

    // ── Output pane ────────────────────────────────────────────────────────
    let total = app.output_lines.len();
    let first = if app.auto_scroll {
        total.saturating_sub(out_h)
    } else {
        app.scroll.min(total.saturating_sub(1))
    };
    let visible: Vec<ListItem<'_>> = app
        .output_lines
        .iter()
        .skip(first)
        .take(out_h)
        .map(|l| ListItem::new(l.clone()))
        .collect();
    // Scroll indicator in top-right when not at bottom
    let out_title = if !app.auto_scroll {
        format!("─ {}/{} ", first + 1, total)
    } else {
        String::new()
    };
    let list = List::new(visible).block(
        Block::default()
            .borders(Borders::NONE)
            .title(out_title),
    );
    f.render_widget(list, out_area);

    // ── Status bar ─────────────────────────────────────────────────────────
    let (status_text, status_style) = if app.confirm_pending.is_some() {
        (
            "  ● awaiting confirmation  [y = yes / n = no / a = always]".to_owned(),
            b(Color::Yellow),
        )
    } else if app.ask_pending.is_some() {
        ("  ● waiting for your answer".to_owned(), b(Color::Cyan))
    } else if app.plan_pending.is_some() {
        (
            "  ● review plan  [y = approve  n = reject  or type feedback]".to_owned(),
            b(Color::Yellow),
        )
    } else if app.agent_running {
        let q = app.input_queue.len();
        let txt = if q > 0 {
            format!("  ● thinking…   ({} queued)", q)
        } else {
            "  ● thinking…".to_owned()
        };
        (txt, s(Color::Green))
    } else {
        ("  ✓ ready".to_owned(), s(Color::DarkGray))
    };
    f.render_widget(Paragraph::new(status_text).style(status_style), status_area);

    // ── Input area ─────────────────────────────────────────────────────────
    let prefix = if app.confirm_pending.is_some() {
        "[y/n/a] > "
    } else if app.ask_pending.is_some() {
        "answer > "
    } else if app.plan_pending.is_some() {
        "[y/n/feedback] > "
    } else {
        "> "
    };
    let before: String = app.input[..app.cursor].iter().collect();
    let cursor_char: String = if app.cursor < app.input.len() {
        app.input[app.cursor].to_string()
    } else {
        " ".to_owned()
    };
    let after: String = if app.cursor < app.input.len() {
        app.input[app.cursor + 1..].iter().collect()
    } else {
        String::new()
    };

    let input_content = Line::from(vec![
        Span::styled(prefix, b(Color::Blue)),
        Span::raw(before),
        Span::styled(
            cursor_char,
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after),
    ]);
    let input_widget = Paragraph::new(input_content)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(input_widget, input_area);

    out_area.height
}

// ─────────────────────────────────────────────────────────────────────────────
// Slash command handler (runs inside the agent task, outputs via tui_tx)
// Returns true if the app should quit.
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_tui_slash(
    input: &str,
    agent: &mut Agent,
    tx: &async_mpsc::UnboundedSender<TuiEvent>,
) -> bool {
    macro_rules! line {
        ($spans:expr) => {{
            let _ = tx.send(TuiEvent::Line(Line::from($spans)));
        }};
    }
    macro_rules! info {
        ($text:expr) => {
            line!(vec![Span::raw($text.to_string())])
        };
    }
    macro_rules! head {
        ($text:expr) => {
            line!(vec![Span::styled($text.to_string(), b(Color::Cyan))])
        };
    }
    macro_rules! item {
        ($label:expr, $val:expr) => {
            line!(vec![
                Span::styled(format!("  {:20}", $label), s(Color::White)),
                Span::styled($val.to_string(), s(Color::DarkGray)),
            ])
        };
    }

    match input {
        "/quit" | "/exit" | "/q" => {
            crate::cli::auto_save_session(agent);
            line!(vec![Span::styled("👋 Goodbye! Happy coding!", s(Color::Green))]);
            return true;
        }

        "/help" | "/h" => {
            head!("── Available Commands ─────────────────────────────────────────");
            item!("/help", "Show this help");
            item!("/clear", "Clear conversation and output");
            item!("/usage", "Show token usage");
            item!("/context", "Show context window status");
            item!("/memory", "Show agent memory");
            item!("/skills", "List loaded skills");
            item!("/model", "List / switch models");
            item!("/mode", "Set execution mode: simple/plan/pipeline/auto");
            item!("/save", "Save current session");
            item!("/sessions", "List saved sessions");
            item!("/yesall", "Auto-approve all operations");
            item!("/confirm", "Re-enable confirmations");
            item!("/summary", "Generate project summary");
            item!("/plan <task>", "Step 1: explore & plan (read-only, no files changed)");
            item!("/plan run", "Step 2: execute the pending plan (modifies files)");
            item!("/rollback", "Rollback sandbox changes");
            item!("/commit", "Commit sandbox changes");
            item!("/changes", "Show sandbox diff");
            item!("/export [file]", "Export chat to Markdown file");
            item!("/quit", "Exit the TUI");
            head!("── Keyboard shortcuts ─────────────────────────────────────────");
            item!("PgUp / PgDn", "Scroll output");
            item!("Mouse wheel", "Scroll output");
            item!("↑ / ↓", "Command history");
            item!("Ctrl-C", "Interrupt agent");
            item!("Ctrl-Q", "Quit");
        }

        "/clear" => {
            agent.reset();
            line!(vec![Span::styled("🔄 Conversation cleared.", s(Color::Cyan))]);
        }

        "/usage" => {
            let (inp, out) = agent.token_usage();
            head!("── Token Usage ────────────────────────────────────────────────");
            item!("Input tokens", inp);
            item!("Output tokens", out);
            item!("Total", inp + out);
        }

        "/context" => {
            let st = crate::context::check_context(&agent.conversation, &agent.config.model);
            head!("── Context Window ─────────────────────────────────────────────");
            item!("Estimated tokens", st.estimated_tokens);
            item!("Max tokens", st.max_tokens);
            item!("Usage", format!("{:.1}%", st.usage_percent));
            item!("Messages", agent.conversation.messages.len());
        }

        "/memory" => {
            let mem = &agent.memory;
            if mem.is_empty() {
                info!("🧠 Memory is empty.");
            } else {
                head!("── Agent Memory ───────────────────────────────────────────────");
                if !mem.knowledge.is_empty() {
                    line!(vec![Span::styled("  📖 Project Knowledge:", b(Color::Cyan))]);
                    for fact in &mem.knowledge {
                        line!(vec![Span::raw(format!("    • {}", fact))]);
                    }
                }
                if !mem.file_map.is_empty() {
                    line!(vec![Span::styled("  📁 Key Files:", b(Color::Cyan))]);
                    for (path, desc) in &mem.file_map {
                        if desc.is_empty() {
                            line!(vec![Span::raw(format!("    • {}", path))]);
                        } else {
                            line!(vec![Span::raw(format!("    • {}  ({})", path, desc))]);
                        }
                    }
                }
                if !mem.session_log.is_empty() {
                    line!(vec![Span::styled("  📝 Session Log:", b(Color::Cyan))]);
                    for entry in mem.session_log.iter().rev().take(10) {
                        line!(vec![Span::styled(format!("    • {}", entry), s(Color::DarkGray))]);
                    }
                }
            }
        }

        "/skills" => {
            let loaded = crate::skills::load_skills(&agent.project_dir);
            if loaded.is_empty() {
                info!("📋 No skills found. Create AGENT.md or add .md files to .agent/skills/");
            } else {
                head!("── Skills ─────────────────────────────────────────────────────");
                for skill in &loaded.skills {
                    line!(vec![Span::raw(format!("  • {} ({}) [embedded]", skill.name, skill.source))]);
                }
                for entry in &loaded.index {
                    line!(vec![Span::raw(format!("  • {} ({}) [on-demand]", entry.name, entry.source))]);
                }
            }
        }

        "/save" => {
            if agent.global_session {
                match crate::persistence::save_session(&agent.conversation, agent.session_id(), &agent.project_dir) {
                    Ok(id) => {
                        agent.set_session_id(id.clone());
                        line!(vec![Span::styled(format!("💾 Session saved (global): {}", id), s(Color::Yellow))]);
                    }
                    Err(e) => line!(vec![Span::styled(format!("✗ Failed to save: {}", e), s(Color::Red))]),
                }
            } else {
                match crate::persistence::save_local_session(&agent.conversation, &agent.project_dir) {
                    Ok(()) => line!(vec![Span::styled("💾 Session saved to .agent/session.json", s(Color::Yellow))]),
                    Err(e) => line!(vec![Span::styled(format!("✗ Failed to save: {}", e), s(Color::Red))]),
                }
            }
        }

        "/sessions" => {
            match crate::persistence::list_sessions() {
                Ok(sessions) if sessions.is_empty() => info!("No saved sessions found."),
                Ok(sessions) => {
                    head!("── Saved Sessions ─────────────────────────────────────────────");
                    line!(vec![Span::styled(
                        format!("  {:<10} {:<24} {:<6} {}", "ID", "Updated", "Msgs", "Summary"),
                        b(Color::White),
                    )]);
                    for s_ in &sessions {
                        line!(vec![Span::raw(format!(
                            "  {:<10} {:<24} {:<6} {}",
                            s_.id, s_.updated_at, s_.message_count, s_.summary
                        ))]);
                    }
                }
                Err(e) => line!(vec![Span::styled(format!("✗ {}", e), s(Color::Red))]),
            }
        }

        "/yesall" => {
            crate::confirm::set_auto_approve(true);
            line!(vec![Span::styled("✅ Auto-approve enabled.", s(Color::Green))]);
        }

        "/confirm" => {
            crate::confirm::set_auto_approve(false);
            line!(vec![Span::styled("🔒 Confirmations re-enabled.", s(Color::Cyan))]);
        }

        _ if input == "/summary" || input.starts_with("/summary ") => {
            let subcommand = input.strip_prefix("/summary").unwrap_or("").trim();
            let cwd = agent.project_dir.clone();
            match subcommand {
                "generate" => {
                    if crate::summary::exists(&cwd) {
                        line!(vec![Span::styled("⚠️  Summary already exists. Regenerating...", s(Color::Yellow))]);
                    }
                    line!(vec![Span::styled("⏳ Generating project summary…", s(Color::Cyan))]);
                    match agent.generate_project_summary().await {
                        Ok(_) => line!(vec![Span::styled("✓ Project summary generated.", s(Color::Green))]),
                        Err(e) => line!(vec![Span::styled(format!("✗ Failed: {}", e), s(Color::Red))]),
                    }
                }
                "" => {
                    if let Some(summary) = crate::summary::load(&cwd) {
                        head!("── Project Summary ─────────────────────────────────────────────");
                        // Render markdown as plain text lines inside the TUI output buffer.
                        // (Cannot use termimad::print_text here — it writes directly to stdout
                        //  and would corrupt the ratatui alternate-screen buffer.)
                        for raw_line in summary.lines() {
                            // Strip common markdown markers for a clean TUI view.
                            let display = raw_line
                                .trim_start_matches("### ")
                                .trim_start_matches("## ")
                                .trim_start_matches("# ")
                                .trim_start_matches("**")
                                .trim_end_matches("**")
                                .trim_start_matches("- ")
                                .trim_start_matches("* ");
                            line!(vec![Span::raw(display.to_owned())]);
                        }
                        line!(vec![Span::styled(
                            "💡 Run /summary generate to regenerate.",
                            s(Color::DarkGray),
                        )]);
                    } else {
                        line!(vec![Span::styled(
                            "📋 No project summary found. Run /summary generate to create one.",
                            s(Color::Yellow),
                        )]);
                    }
                }
                _ => {
                    line!(vec![Span::styled("Usage: /summary  |  /summary generate", s(Color::Yellow))]);
                }
            }
        }

        _ if input == "/plan" || input.starts_with("/plan ") => {
            let subcommand = input.strip_prefix("/plan").unwrap_or("").trim();
            match subcommand {
                "" => {
                    head!("── Plan Mode ───────────────────────────────────────────────────");
                    line!(vec![Span::raw("  /plan <task>   Step 1: explore codebase, generate a plan (no files changed)")]);
                    line!(vec![Span::raw("  /plan run      Step 2: execute the pending plan (actually modifies files)")]);
                    line!(vec![Span::raw("  /plan show     Display the pending plan")]);
                    line!(vec![Span::raw("  /plan clear    Discard the pending plan")]);
                    if agent.pending_plan.is_some() {
                        line!(vec![Span::styled(
                            "💡 A pending plan exists — type /plan run to execute it.",
                            s(Color::Green),
                        )]);
                    } else {
                        line!(vec![Span::styled(
                            "💡 Tip: /plan only PLANS (read-only). Run /plan run afterwards to actually execute.",
                            s(Color::DarkGray),
                        )]);
                    }
                }
                "run" => {
                    if let Some(plan) = agent.pending_plan.clone() {
                        line!(vec![Span::styled("🚀 Executing plan…", b(Color::Cyan))]);
                        let _ = tx.send(TuiEvent::AgentBusy(true));
                        match agent.execute_plan(&plan).await {
                            Ok(_) => {
                                crate::cli::auto_save_session(agent);
                                line!(vec![Span::styled("✅ Plan executed.", s(Color::Green))]);
                            }
                            Err(e) => line!(vec![Span::styled(
                                format!("✗ Plan execution failed: {}", e), s(Color::Red)
                            )]),
                        }
                        let _ = tx.send(TuiEvent::AgentBusy(false));
                    } else {
                        line!(vec![Span::styled(
                            "⚠️  No pending plan. Use /plan <task> to generate one first.",
                            s(Color::Yellow),
                        )]);
                    }
                }
                "show" => {
                    if let Some(ref plan) = agent.pending_plan {
                        head!("── Pending Plan ────────────────────────────────────────────────");
                        for raw_line in plan.lines() {
                            let display = raw_line
                                .trim_start_matches("### ")
                                .trim_start_matches("## ")
                                .trim_start_matches("# ")
                                .trim_start_matches("**")
                                .trim_end_matches("**")
                                .trim_start_matches("- ")
                                .trim_start_matches("* ");
                            line!(vec![Span::raw(display.to_owned())]);
                        }
                        line!(vec![Span::styled(
                            "💡 Use /plan run to execute or /plan clear to discard.",
                            s(Color::DarkGray),
                        )]);
                    } else {
                        info!("📋 No pending plan.");
                    }
                }
                "clear" => {
                    if agent.pending_plan.is_some() {
                        agent.pending_plan = None;
                        line!(vec![Span::styled("🗑️  Pending plan cleared.", s(Color::Cyan))]);
                    } else {
                        info!("📋 No pending plan to clear.");
                    }
                }
                task => {
                    // Step 1: read-only exploration + plan generation.
                    // Mark busy so the status bar shows "● thinking…" during the LLM loop.
                    line!(vec![Span::styled(
                        "📝 Analyzing codebase and generating plan… (no files will be changed)",
                        s(Color::Cyan),
                    )]);
                    let _ = tx.send(TuiEvent::AgentBusy(true));
                    match agent.generate_plan(task).await {
                        Ok(_) => {
                            let _ = tx.send(TuiEvent::AgentBusy(false));
                            line!(vec![Span::styled(
                                "✅ Plan ready. Type /plan show to review, then /plan run to execute.",
                                b(Color::Green),
                            )]);
                            crate::cli::auto_save_session(agent);
                        }
                        Err(e) => {
                            let _ = tx.send(TuiEvent::AgentBusy(false));
                            line!(vec![Span::styled(
                                format!("✗ Plan generation failed: {}", e), s(Color::Red)
                            )]);
                        }
                    }
                }
            }
        }

        "/rollback" => {
            if !agent.sandbox.is_enabled().await {
                line!(vec![Span::styled(
                    "⚠️  Sandbox is not enabled. Start the agent with --sandbox to use this feature.",
                    s(Color::Yellow),
                )]);
            } else {
                let changes = agent.sandbox.changed_files().await;
                if changes.is_empty() {
                    info!("📋 No changes to rollback.");
                } else {
                    head!("── Rolling Back ────────────────────────────────────────────────");
                    for c in &changes {
                        let icon = match c.kind {
                            crate::sandbox::ChangeKind::Modified  => "✏️ ",
                            crate::sandbox::ChangeKind::Created   => "📄",
                            crate::sandbox::ChangeKind::Deleted   => "🗑️",
                            crate::sandbox::ChangeKind::Unchanged => "⚪",
                        };
                        line!(vec![Span::raw(format!("  {} {} ({})", icon, c.path.display(), c.kind))]);
                    }
                    let result = agent.sandbox.rollback().await;
                    if result.errors.is_empty() {
                        line!(vec![Span::styled(
                            format!("✅ Rolled back: {} restored, {} deleted.",
                                result.restored, result.deleted),
                            s(Color::Green),
                        )]);
                    } else {
                        line!(vec![Span::styled(
                            format!("⚠️  Rollback completed with {} error(s):", result.errors.len()),
                            s(Color::Yellow),
                        )]);
                        for err in &result.errors {
                            line!(vec![Span::styled(format!("  ✗ {}", err), s(Color::Red))]);
                        }
                    }
                }
            }
        }

        "/commit" => {
            if !agent.sandbox.is_enabled().await {
                line!(vec![Span::styled(
                    "⚠️  Sandbox is not enabled. Start the agent with --sandbox to use this feature.",
                    s(Color::Yellow),
                )]);
            } else {
                let changes = agent.sandbox.changed_files().await;
                if changes.is_empty() {
                    info!("📋 No changes to commit.");
                } else {
                    head!("── Committing ──────────────────────────────────────────────────");
                    for c in &changes {
                        let icon = match c.kind {
                            crate::sandbox::ChangeKind::Modified  => "✏️ ",
                            crate::sandbox::ChangeKind::Created   => "📄",
                            crate::sandbox::ChangeKind::Deleted   => "🗑️",
                            crate::sandbox::ChangeKind::Unchanged => "⚪",
                        };
                        line!(vec![Span::raw(format!("  {} {} ({})", icon, c.path.display(), c.kind))]);
                    }
                    let result = agent.sandbox.commit().await;
                    line!(vec![Span::styled(
                        format!("✅ Committed: {} modified, {} created. Snapshots discarded.",
                            result.modified, result.created),
                        s(Color::Green),
                    )]);
                }
            }
        }

        "/changes" => {
            if !agent.sandbox.is_enabled().await {
                line!(vec![Span::styled(
                    "⚠️  Sandbox is not enabled. Start the agent with --sandbox to use this feature.",
                    s(Color::Yellow),
                )]);
            } else {
                let changes = agent.sandbox.changed_files().await;
                if changes.is_empty() {
                    info!("📋 No changes tracked yet.");
                } else {
                    head!("── Sandbox Changes ─────────────────────────────────────────────");
                    let (mut modified, mut created, mut unchanged) = (0usize, 0usize, 0usize);
                    for c in &changes {
                        let (icon, color) = match c.kind {
                            crate::sandbox::ChangeKind::Modified  => { modified  += 1; ("✏️ ", Color::Yellow) }
                            crate::sandbox::ChangeKind::Created   => { created   += 1; ("📄", Color::Green)  }
                            crate::sandbox::ChangeKind::Deleted   => { modified  += 1; ("🗑️", Color::Red)    }
                            crate::sandbox::ChangeKind::Unchanged => { unchanged += 1; ("⚪", Color::DarkGray) }
                        };
                        let size_info = match (c.original_size, c.current_size) {
                            (Some(orig), Some(curr)) if orig != curr =>
                                format!(" ({} → {} bytes)", orig, curr),
                            (None, Some(curr)) => format!(" ({} bytes)", curr),
                            _ => String::new(),
                        };
                        line!(vec![Span::styled(
                            format!("  {} {} [{}]{}", icon, c.path.display(), c.kind, size_info),
                            s(color),
                        )]);
                    }
                    line!(vec![Span::raw(format!(
                        "  Summary: {} modified, {} created, {} unchanged",
                        modified, created, unchanged
                    ))]);
                    line!(vec![Span::styled(
                        "  Use /rollback to undo all, /commit to accept all.",
                        s(Color::DarkGray),
                    )]);
                }
            }
        }

        _ if input == "/export" || input.starts_with("/export ") => {
            use std::fmt::Write as FmtWrite;
            use std::time::{SystemTime, UNIX_EPOCH};
            use crate::conversation::{ContentBlock, Role};

            // Build a UTC timestamp string without any external crate.
            let ts_string = {
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let s  = secs % 60;
                let m  = (secs / 60) % 60;
                let h  = (secs / 3600) % 24;
                let days = secs / 86400;
                // Gregorian calendar from day-count (valid for ~2000-2100).
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

            // Determine output filename.
            let filename = {
                let custom = input.strip_prefix("/export").unwrap_or("").trim();
                if custom.is_empty() {
                    // YYYY-MM-DD-HHMMSS from the timestamp string.
                    let compact = ts_string
                        .replace(" UTC", "")
                        .replace(' ', "-")
                        .replace(':', "");
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
                // Skip messages that contain only ToolResult blocks (API protocol
                // artefacts — not meaningful to a human reader).
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
                        // Append tool results that belong to this assistant turn.
                        // They come as the next User message; we show them here
                        // so the export reads naturally.
                        let _ = writeln!(md, "---\n");
                    }
                    _ => {} // ToolResult-only user messages and System messages
                }
            }

            match std::fs::write(&path, &md) {
                Ok(()) => line!(vec![
                    Span::styled("💾 Saved: ", b(Color::Green)),
                    Span::styled(path.display().to_string(), s(Color::White)),
                    Span::styled(format!(" ({} bytes)", md.len()), s(Color::DarkGray)),
                ]),
                Err(e) => line!(vec![Span::styled(
                    format!("✗ Export failed: {}", e),
                    s(Color::Red),
                )]),
            }
        }

        _ if input == "/model" || input.starts_with("/model ") => {
            let subcommand = input.strip_prefix("/model").unwrap_or("").trim();
            match subcommand {
                "" => {
                    head!("── Model ───────────────────────────────────────────────────────");
                    line!(vec![
                        Span::styled("  Current: ", s(Color::DarkGray)),
                        Span::styled(agent.config.model.clone(), b(Color::White)),
                        Span::styled(format!(" ({})", agent.config.provider), s(Color::Cyan)),
                    ]);
                    if let Some(ref alias) = agent.config.model_alias {
                        line!(vec![
                            Span::styled("  Alias: ", s(Color::DarkGray)),
                            Span::styled(alias.clone(), s(Color::Yellow)),
                        ]);
                    }
                    let models_cfg = crate::model_manager::load();
                    if models_cfg.models.is_empty() {
                        line!(vec![Span::styled(
                            "  No models configured. Use CLI mode to add: /model add <alias>",
                            s(Color::DarkGray),
                        )]);
                    } else {
                        let default_alias = models_cfg.default.as_deref().unwrap_or("");
                        for alias in models_cfg.aliases() {
                            if let Some(entry) = models_cfg.models.get(alias) {
                                let star   = if alias == default_alias { " ⭐" } else { "" };
                                let active = agent.config.model_alias.as_deref() == Some(alias);
                                let prefix = if active { "▶" } else { "•" };
                                let color  = if active { Color::Green } else { Color::DarkGray };
                                line!(vec![Span::styled(
                                    format!("  {} {} → {}/{}{}",
                                        prefix, alias, entry.provider, entry.model, star),
                                    s(color),
                                )]);
                            }
                        }
                    }
                    line!(vec![Span::styled(
                        "  Switch: /model <alias>   (add/remove/default: use CLI mode)",
                        s(Color::DarkGray),
                    )]);
                }
                sub if sub.starts_with("add ") || sub.starts_with("remove ")
                    || sub.starts_with("default ") =>
                {
                    line!(vec![Span::styled(
                        "⚠️  Model add/remove/default require interactive prompts — use CLI mode.",
                        s(Color::Yellow),
                    )]);
                }
                alias => {
                    let models_cfg = crate::model_manager::load();
                    if let Some(resolved) = models_cfg.resolve(alias) {
                        agent.switch_model(&resolved);
                        line!(vec![Span::styled(
                            format!("🔄 Switched to '{}' → {} ({})",
                                alias, agent.config.model, agent.config.provider),
                            s(Color::Green),
                        )]);
                    } else {
                        line!(vec![Span::styled(
                            format!("❓ Model '{}' not found. Use /model to list available aliases.", alias),
                            s(Color::Yellow),
                        )]);
                    }
                }
            }
        }

        _ if input == "/mode" || input.starts_with("/mode ") => {
            use crate::router::ExecutionMode;
            let sub = input.strip_prefix("/mode").unwrap_or("").trim();
            match sub {
                "" => {
                    let current = match agent.force_mode {
                        Some(ExecutionMode::BasicLoop)      => "simple (forced)",
                        Some(ExecutionMode::PlanAndExecute) => "plan (forced)",
                        Some(ExecutionMode::FullPipeline)   => "pipeline (forced)",
                        None => "auto (router decides)",
                    };
                    head!("── Execution Mode ─────────────────────────────────────────────");
                    item!("Current", current);
                    item!("/mode simple",   "force single-model loop");
                    item!("/mode plan",     "force planner + executor");
                    item!("/mode pipeline", "force full pipeline");
                    item!("/mode auto",     "let router decide (default)");
                }
                "simple" => {
                    agent.set_force_mode(Some(ExecutionMode::BasicLoop));
                    line!(vec![Span::styled("🔀 Mode locked to simple: single-model loop.", s(Color::Green))]);
                }
                "plan" => {
                    agent.set_force_mode(Some(ExecutionMode::PlanAndExecute));
                    line!(vec![Span::styled("🔀 Mode locked to plan: planner + executor.", s(Color::Green))]);
                }
                "pipeline" => {
                    agent.set_force_mode(Some(ExecutionMode::FullPipeline));
                    line!(vec![Span::styled("🔀 Mode locked to pipeline: full pipeline.", s(Color::Green))]);
                }
                "auto" => {
                    agent.set_force_mode(None);
                    line!(vec![Span::styled("🔀 Mode reset to auto: router will classify each task.", s(Color::Green))]);
                }
                other => {
                    line!(vec![Span::styled(
                        format!("❓ Unknown mode: {}. Use simple/plan/pipeline/auto", other),
                        s(Color::Red),
                    )]);
                }
            }
        }

        _ => {
            line!(vec![
                Span::styled(format!("❓ Unknown command: {}  ", input), s(Color::Red)),
                Span::styled("Type /help for available commands.", s(Color::DarkGray)),
            ]);
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub async fn run(
    config: Config,
    project_dir: PathBuf,
    initial_prompt: Option<String>,
    resume_id: Option<String>,
    sandbox_enabled: bool,
    global_session: bool,
    yes: bool,
) -> Result<()> {
    // Set auto-approve from --yes flag.
    if yes {
        crate::confirm::set_auto_approve(true);
    }

    let (tui_tx, tui_rx) = async_mpsc::unbounded_channel::<TuiEvent>();
    let tui_out = Arc::new(TuiOutput::new(tui_tx.clone()));

    let (input_tx, mut input_rx) = async_mpsc::unbounded_channel::<String>();
    let mut app = TuiApp::new(tui_rx, input_tx);

    // ── Agent task (runs entirely on its own tokio worker thread) ─────────
    let agent_out: Arc<dyn AgentOutput> = tui_out.clone();
    let tui_tx2 = tui_tx.clone();
    let mut agent_task = tokio::spawn(async move {
        let sandbox = if sandbox_enabled {
            Sandbox::new(&project_dir)
        } else {
            Sandbox::disabled(&project_dir)
        };

        let mut agent = if let Some(ref session_id) = resume_id {
            match crate::persistence::load_session(session_id) {
                Ok(session) => {
                    let conv = crate::persistence::restore_conversation(&session);
                    let mut a = Agent::with_conversation(
                        config,
                        project_dir,
                        conv,
                        session_id.clone(),
                        agent_out,
                        sandbox,
                    );
                    a.global_session = global_session;
                    a
                }
                Err(e) => {
                    tui_tx2
                        .send(TuiEvent::Line(Line::from(vec![Span::styled(
                            format!("Error resuming session: {}", e),
                            b(Color::Red),
                        )])))
                        .ok();
                    let mut a = Agent::new(config, project_dir, agent_out, sandbox);
                    a.global_session = global_session;
                    a
                }
            }
        } else {
            let mut a = Agent::new(config, project_dir, agent_out, sandbox);
            a.global_session = global_session;
            a
        };

        // Handle --prompt / initial message.
        if let Some(prompt) = initial_prompt {
            tui_tx2.send(TuiEvent::AgentBusy(true)).ok();
            crate::agent::clear_interrupt();
            agent.drain_service_events();
            agent.process_message(&prompt).await.ok();
            agent.drain_service_events();
            tui_tx2.send(TuiEvent::AgentBusy(false)).ok();
        }

        // Main message loop.
        loop {
            match input_rx.recv().await {
                Some(msg) => {
                    if msg.starts_with('/') {
                        // Slash commands: handle without marking agent busy (they are fast)
                        // except async ones that need the agent to do real work.
                        let quit = handle_tui_slash(&msg, &mut agent, &tui_tx2).await;
                        if quit {
                            break;
                        }
                        tui_tx2.send(TuiEvent::AgentBusy(false)).ok();
                    } else {
                        tui_tx2.send(TuiEvent::AgentBusy(true)).ok();
                        crate::agent::clear_interrupt();
                        agent.drain_service_events();
                        agent.process_message(&msg).await.ok();
                        agent.drain_service_events();
                        tui_tx2.send(TuiEvent::AgentBusy(false)).ok();
                    }
                }
                None => break,
            }
        }
    });

    // ── Terminal compatibility check ──────────────────────────────────────
    // gnome-terminal (VTE) has poor alternate-screen isolation: the main
    // buffer "shows through" undrawn cells even after a full clear.
    // Detect it via $TERM_PROGRAM or $VTE_VERSION and warn the user.
    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_lowercase();
    let is_limited_terminal = term_program.contains("gnome")
        || term_program.contains("vte")
        || term_program == "vscode"
        || std::env::var("VTE_VERSION").is_ok()
        || std::env::var("VSCODE_INJECTION").is_ok();
    if is_limited_terminal {
        let name = if term_program == "vscode" || std::env::var("VSCODE_INJECTION").is_ok() {
            "VS Code integrated terminal"
        } else {
            "gnome-terminal (VTE)"
        };
        eprintln!(
            "\x1b[33mWarning: {} has known TUI rendering issues.\n\
             Recommended alternatives: kitty, alacritty, wezterm, or any xterm-compatible terminal.\x1b[0m",
            name
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // ── Terminal setup ────────────────────────────────────────────────────
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    // Force a full clear on entry — gnome-terminal (Ubuntu default) sometimes
    // retains scroll-back content when entering the alternate screen, causing
    // garbled rendering until the first resize event.
    terminal.clear()?;

    let mut out_h: u16 = 20;
    // Guard flag: prevents re-polling the JoinHandle after it resolves.
    let mut agent_alive = true;

    // ── Main render / event loop ──────────────────────────────────────────
    let result = async {
        loop {
            // Render frame.
            terminal.draw(|f| {
                out_h = render(f, &app);
            })?;

            if app.quit {
                break;
            }

            // Wait for agent output OR a 16 ms tick (≈60 fps), whichever comes first.
            // Also watch for the agent task dying unexpectedly (panic / error).
            tokio::select! {
                biased;
                Some(ev) = app.rx.recv() => {
                    app.handle_tui_event(ev);
                    // Drain all remaining buffered events before re-rendering.
                    while let Ok(ev) = app.rx.try_recv() {
                        app.handle_tui_event(ev);
                    }
                }
                result = &mut agent_task, if agent_alive => {
                    // Agent task exited (clean quit, panic, or error).
                    agent_alive = false;
                    let reason = match result {
                        Ok(()) => "agent task exited normally".to_owned(),
                        Err(e) if e.is_panic() => format!("panic: {:?}", e),
                        Err(e) => format!("error: {}", e),
                    };
                    app.handle_tui_event(TuiEvent::AgentDied(reason));
                }
                _ = tokio::time::sleep(Duration::from_millis(RENDER_INTERVAL_MS)) => {}
            }

            // Check keyboard / mouse (non-blocking poll; Duration::ZERO avoids blocking tokio).
            while crossterm::event::poll(Duration::ZERO)? {
                match crossterm::event::read() {
                    Ok(Event::Key(key)) => {
                        if app.handle_key(key, out_h) {
                            app.quit = true;
                            break;
                        }
                    }
                    Ok(Event::Mouse(mouse)) => {
                        app.handle_mouse(mouse, out_h);
                    }
                    Ok(Event::Resize(_, _)) => {
                        // Force a full redraw on the next iteration; ratatui
                        // handles the new geometry automatically via terminal.draw().
                        terminal.autoresize()?;
                    }
                    _ => {}
                }
            }

            if app.quit {
                break;
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;

    // ── Terminal teardown (always runs) ───────────────────────────────────
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    result
}
