//! Memory provider abstraction.
//!
//! `MemoryProvider` decouples the agent from the concrete store. The default
//! implementation (`LocalFileMemory`) wraps `Memory`, backed by
//! `.agent/memory.md`.
//!
//! ```text
//! Arc<dyn MemoryProvider>
//!      |
//!      +-- LocalFileMemory   <- default (.agent/memory.md)
//!      +-- NullMemory        <- tests / sandboxes / stateless runs
//! ```
//!
//! # Design
//!
//! All methods are **synchronous** with interior mutability (`Mutex`) so they
//! can be called from both sync and async contexts. Writes are persisted as
//! they happen, so there is no explicit flush step.

use std::sync::Mutex;

use super::Memory;

// ── Trait ────────────────────────────────────────────────────────────────────

/// The interface every memory backend must implement.
///
/// Three roles, mirroring the Formation / Recall / Maintenance paradigm:
///
/// - **Formation** (`add_knowledge`) — write durable project facts
/// - **Recall** (`recall`) — render those facts for the system prompt
/// - **Lifecycle** (`on_*`) — react to turn and session boundaries
pub trait MemoryProvider: Send + Sync {
    // ── Recall ─────────────────────────────────────────────────────────────

    /// Render the project knowledge section for the system prompt.
    fn recall(&self) -> String;

    // ── Formation ──────────────────────────────────────────────────────────

    /// Store a durable project fact.
    fn add_knowledge(&self, fact: &str);

    // ── Introspection (for CLI display) ────────────────────────────────────

    /// True if no knowledge has been recorded yet.
    fn is_empty(&self) -> bool;

    /// Number of stored knowledge entries.
    fn entry_count(&self) -> usize;

    /// All knowledge entries, without timestamps or sources.
    fn knowledge(&self) -> Vec<String>;

    // ── Lifecycle hooks (default no-ops, override to opt in) ──────────────

    /// Called at the start of each turn with the user message.
    ///
    /// Use for turn-counting, scope management, periodic maintenance.
    fn on_turn_start(&self, _turn_number: u32, _message: &str, _remaining_tokens: u64, _model: &str) {}

    /// Called when the agent switches session_id mid-process.
    ///
    /// Fires on session resume, branch, reset, and context compression.
    fn on_session_switch(&self, _new_session_id: &str, _parent_session_id: &str, _reset: bool) {}

    /// Called before context compression discards old messages.
    ///
    /// Return text to inject into the compression summary prompt; an empty
    /// string contributes nothing.
    fn on_pre_compress(&self, _messages: &[crate::conversation::Message]) -> String {
        String::new()
    }

    /// Called when the memory tool writes an entry (add/replace/remove).
    fn on_memory_write(&self, _action: &str, _target: &str, _content: &str) {}
}

// ── LocalFileMemory ──────────────────────────────────────────────────────────

/// Default implementation backed by `.agent/memory.md`.
pub struct LocalFileMemory {
    inner: Mutex<Memory>,
}

impl LocalFileMemory {
    /// Load memory from `.agent/memory.md` under `project_dir`, pruning each
    /// section with `limits`. Returns an empty store if the file is missing.
    pub fn load_with_limits(project_dir: &std::path::Path, limits: super::MemoryLimits) -> Self {
        Self {
            inner: Mutex::new(Memory::load_with_limits(project_dir, limits)),
        }
    }
}

impl MemoryProvider for LocalFileMemory {
    fn recall(&self) -> String {
        self.inner.lock().unwrap().to_system_prompt_knowledge()
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
        self.inner.lock().unwrap().knowledge.iter().map(|k| k.text.clone()).collect()
    }
}

// ── NullMemory ───────────────────────────────────────────────────────────────

/// No-op memory provider.
///
/// Useful for:
/// - Unit tests that should not touch the filesystem
/// - Sandboxed / ephemeral agent runs
pub struct NullMemory;

impl MemoryProvider for NullMemory {
    fn recall(&self) -> String { String::new() }
    fn add_knowledge(&self, _fact: &str) {}
    fn is_empty(&self) -> bool { true }
    fn entry_count(&self) -> usize { 0 }
    fn knowledge(&self) -> Vec<String> { vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryLimits;
    use tempfile::tempdir;

    /// `recall()` is the only path that feeds project knowledge into the
    /// system prompt, so a fact must survive a reload and show up there.
    #[test]
    fn recall_renders_stored_knowledge() {
        let dir = tempdir().unwrap();
        let memory = LocalFileMemory::load_with_limits(dir.path(), MemoryLimits::default());
        assert!(memory.recall().is_empty());

        memory.add_knowledge("The build uses cargo");
        assert!(memory.recall().contains("The build uses cargo"));

        let reloaded = LocalFileMemory::load_with_limits(dir.path(), MemoryLimits::default());
        assert!(reloaded.recall().contains("The build uses cargo"));
        assert_eq!(reloaded.knowledge(), vec!["The build uses cargo".to_string()]);
    }
}
