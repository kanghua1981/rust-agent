//! Memory provider abstraction.
//!
//! Defines the `MemoryProvider` trait that decouples the agent from any specific
//! memory backend. The default implementation (`LocalFileMemory`) wraps the
//! existing `Memory` struct and is a drop-in replacement with zero behaviour change.
//!
//! # Extension points
//!
//! The memory landscape is evolving rapidly (OpenViking, Anda Hippocampus, …).
//! By programming to this trait, the agent can switch backends via configuration
//! without touching agent logic:
//!
//! ```text
//! Arc<dyn MemoryProvider>
//!      │
//!      ├── LocalFileMemory   ← default, no dependencies (.agent/memory.md)
//!      ├── NullMemory        ← tests / sandboxes / stateless runs
//!      └── HttpMemory        ← (future) any external memory service
//! ```
//!
//! # Design
//!
//! All methods are **synchronous** with interior mutability (`Mutex`) so they can
//! be called from both sync and async contexts without additional ceremony.

use std::sync::Mutex;

use super::Memory;

// ── Event vocabulary ─────────────────────────────────────────────────────────

/// Semantic events that the agent records as it operates.
///
/// Using an enum keeps the call-sites clean and lets each backend decide
/// exactly how to represent each event in its storage model.
pub enum MemoryEvent {
    /// Raw knowledge facts extracted by LLM from the conversation.
    KnowledgeExtracted { facts: Vec<String> },
    FileRead { path: String },
    FileWritten { path: String, lines: usize },
    FileEdited { path: String },
    FileMultiEdited { path: String, edits: usize },
    FileSearched { path: String },
    BatchFilesRead { paths: Vec<String> },
    PdfRead { path: String },
    CommandRun { command: String },
    GrepSearch { pattern: String, path: Option<String> },
    FileFind { pattern: String },
    DirectoryListed { path: String },
    Custom { action: String },
}

// ── Trait ────────────────────────────────────────────────────────────────────

/// The interface every memory backend must implement.
///
/// Three semantic categories mirror the Formation / Recall / Maintenance
/// paradigm used in modern agent-memory literature:
///
/// - **Formation** (`record_event`, `log_truncation`) — write new observations
/// - **Recall**    (`recall`)                          — read for context injection
/// - **Maintenance** (`flush`)                         — consolidate / persist
pub trait MemoryProvider: Send + Sync {
    // ── Formation ──────────────────────────────────────────────────────────

    /// Record an agent action event.
    fn record_event(&self, event: MemoryEvent);

    /// Record a context-window truncation summary (produced when history is compressed).
    fn log_truncation(&self, summary: &str);

    /// Record a complete interaction episode (prompt → outcome → lessons).
    ///
    /// Providers that support rich episode storage (e.g. `IntelligentMemory`)
    /// override this. Simple providers use the no-op default.
    #[allow(clippy::too_many_arguments)]
    fn record_interaction(
        &self,
        _prompt: &str,
        _intent_summary: &str,
        _tools_used: &[String],
        _outcome_success: bool,
        _outcome_detail: &str,
        _lessons: &[String],
        _feedback: Option<&str>,
        _tokens: u32,
    ) {
        // no-op for basic providers
    }

    // ── Recall ─────────────────────────────────────────────────────────────

    /// Return the **project knowledge** section for injection into the system prompt.
    ///
    /// Intentionally small (≤5 entries). File-map and session-log are *not*
    /// included here — use `recall_relevant()` for per-turn contextual recall.
    fn recall(&self) -> String;

    /// Retrieve file-map and session-log entries relevant to `query`.
    ///
    /// Scores entries by keyword overlap and access frequency. The result is
    /// prepended to the user message for the current turn only, so irrelevant
    /// entries consume zero tokens.
    fn recall_relevant(&self, query: &str) -> String;

    // ── Maintenance ────────────────────────────────────────────────────────

    /// Persist any in-memory state to durable storage.
    fn flush(&self) -> anyhow::Result<()>;

    /// Directly add a knowledge fact to the knowledge section.
    /// Used by the knowledge extraction pipeline.
    fn add_knowledge(&self, fact: &str);

    // ── Introspection (for CLI display) ────────────────────────────────────

    /// True if no entries have been recorded yet.
    fn is_empty(&self) -> bool;

    /// Total number of entries across all sections.
    fn entry_count(&self) -> usize;

    /// Returns all knowledge entries.
    fn knowledge(&self) -> Vec<String>;

    /// Returns `(path, description)` pairs for the file map.
    fn file_map(&self) -> Vec<(String, String)>;

    /// Returns session log entries.
    fn session_log(&self) -> Vec<String>;
}

// ── LocalFileMemory ──────────────────────────────────────────────────────────

/// Default implementation backed by `.agent/memory.md`.
///
/// Wraps the existing `Memory` struct with a `Mutex` to satisfy `Send + Sync`.
/// Behaviour is identical to direct `Memory` usage — this is a zero-risk Phase 1
/// refactor.
pub struct LocalFileMemory {
    inner: Mutex<Memory>,
}

impl LocalFileMemory {
    /// Load memory from `.agent/memory.md` under `project_dir`.
    /// Returns an empty memory store if the file does not exist.
    pub fn load(project_dir: &std::path::Path) -> Self {
        Self {
            inner: Mutex::new(Memory::load(project_dir)),
        }
    }
}

impl MemoryProvider for LocalFileMemory {
    fn record_event(&self, event: MemoryEvent) {
        let mut m = self.inner.lock().unwrap();
        match event {
            MemoryEvent::FileRead { path } => {
                m.touch_file(&path, "read");
                m.log_action(&format!("read {}", path));
            }
            MemoryEvent::FileWritten { path, lines } => {
                m.touch_file(&path, &format!("written ({} lines)", lines));
                m.log_action(&format!("wrote {}", path));
            }
            MemoryEvent::FileEdited { path } => {
                m.touch_file(&path, "edited");
                m.log_action(&format!("edited {}", path));
            }
            MemoryEvent::FileMultiEdited { path, edits } => {
                m.touch_file(&path, &format!("multi-edited ({} edits)", edits));
                m.log_action(&format!("multi-edited {} ({} edits)", path, edits));
            }
            MemoryEvent::FileSearched { path } => {
                m.touch_file(&path, "searched");
            }
            MemoryEvent::BatchFilesRead { paths } => {
                let count = paths.len();
                for p in &paths {
                    m.touch_file(p, "read");
                }
                m.log_action(&format!("batch-read {} files", count));
            }
            MemoryEvent::PdfRead { path } => {
                m.touch_file(&path, "read (PDF)");
                m.log_action(&format!("read PDF {}", path));
            }
            MemoryEvent::CommandRun { command } => {
                m.log_action(&format!("ran `{}`", command));
            }
            MemoryEvent::GrepSearch { pattern, path } => {
                m.log_action(&format!("searched for `{}`", pattern));
                if let Some(p) = path {
                    m.touch_file(&p, "searched");
                }
            }
            MemoryEvent::FileFind { pattern } => {
                m.log_action(&format!("found files matching `{}`", pattern));
            }
            MemoryEvent::DirectoryListed { path } => {
                m.log_action(&format!("listed {}", path));
            }
            MemoryEvent::KnowledgeExtracted { facts } => {
                for fact in &facts {
                    m.add_knowledge(fact);
                }
                m.log_action(&format!("extracted {} knowledge items", facts.len()));
            }
            MemoryEvent::Custom { action } => {
                m.log_action(&action);
            }
        }

        // Auto-save after each event — preserves current behaviour exactly.
        if let Err(e) = m.save() {
            tracing::warn!("Failed to auto-save memory: {}", e);
        }
    }

    fn log_truncation(&self, summary: &str) {
        let mut m = self.inner.lock().unwrap();
        m.log_truncation_summary(summary);
        if let Err(e) = m.save() {
            tracing::warn!("Failed to save truncation summary to memory: {}", e);
        }
    }

    fn recall(&self) -> String {
        self.inner.lock().unwrap().to_system_prompt_knowledge()
    }

    fn recall_relevant(&self, query: &str) -> String {
        self.inner.lock().unwrap().recall_relevant(query)
    }

    fn flush(&self) -> anyhow::Result<()> {
        self.inner.lock().unwrap().save().map_err(Into::into)
    }

    fn add_knowledge(&self, fact: &str) {
        let mut m = self.inner.lock().unwrap();
        m.add_knowledge(fact);
        if let Err(e) = m.save() {
            tracing::warn!("Failed to save knowledge: {}", e);
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    fn entry_count(&self) -> usize {
        self.inner.lock().unwrap().entry_count()
    }

    fn knowledge(&self) -> Vec<String> {
        self.inner.lock().unwrap().knowledge.clone()
    }

    fn file_map(&self) -> Vec<(String, String)> {
        self.inner.lock().unwrap().file_map
            .iter()
            .map(|e| (e.path.clone(), format!("{} (×{}, {})", e.description, e.access_count, e.last_accessed)))
            .collect()
    }

    fn session_log(&self) -> Vec<String> {
        self.inner.lock().unwrap().session_log.clone()
    }
}

// ── NullMemory ───────────────────────────────────────────────────────────────

/// No-op memory provider.
///
/// Useful for:
/// - Unit tests that should not touch the filesystem
/// - Sandboxed / ephemeral agent runs
/// - `--no-memory` flag (future)
pub struct NullMemory;

impl MemoryProvider for NullMemory {
    fn record_event(&self, _event: MemoryEvent) {}
    fn log_truncation(&self, _summary: &str) {}
    fn recall(&self) -> String { String::new() }
    fn recall_relevant(&self, _query: &str) -> String { String::new() }
    fn flush(&self) -> anyhow::Result<()> { Ok(()) }
    fn add_knowledge(&self, _fact: &str) {}
    fn is_empty(&self) -> bool { true }
    fn entry_count(&self) -> usize { 0 }
    fn knowledge(&self) -> Vec<String> { vec![] }
    fn file_map(&self) -> Vec<(String, String)> { vec![] }
    fn session_log(&self) -> Vec<String> { vec![] }
}
