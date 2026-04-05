//! Intelligent memory provider.
//!
//! Combines the reliability of `LocalFileMemory` (markdown persistence) with
//! a richer JSON store that tracks full interaction episodes, project knowledge,
//! and user preferences.
//!
//! Storage:
//! - `.agent/memory.md`        — human-readable log (backward compatible)
//! - `.agent/intelligent.json` — structured episode store (JSON)

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::Memory;
use super::provider::{MemoryEvent, MemoryProvider};

const INTEL_FILE: &str = ".agent/intelligent.json";
const MAX_INTENTS: usize = 100;
const MAX_KNOWLEDGE: usize = 200;
const MAX_PATTERNS: usize = 50;

// ── Persistent Data Types ────────────────────────────────────────────────

/// A complete record of one user interaction episode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIntent {
    pub id: String,
    pub timestamp: u64,
    /// Original user prompt
    pub prompt: String,
    /// LLM's understanding of what was actually wanted
    pub intent_summary: String,
    pub category: IntentCategory,
    pub tools_used: Vec<String>,
    pub outcome: IntentOutcome,
    /// Lessons learned from this interaction
    pub lessons: Vec<String>,
    /// Explicit user feedback ("Thanks", "Not what I wanted", …)
    pub feedback: Option<String>,
    pub tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentCategory {
    CodeWrite,
    CodeModify,
    CodeReview,
    FileOperation,
    CommandRun,
    Information,
    Debug,
    Plan,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentOutcome {
    Success { summary: String },
    PartialSuccess { achieved: String, missed: String },
    Failure { reason: String },
    Unknown,
}

/// A persistent knowledge fact about the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeFact {
    pub id: String,
    pub timestamp: u64,
    pub topic: String,
    pub fact: String,
    /// How many times this fact was referenced / re-confirmed
    pub reference_count: u32,
}

/// A detected interaction pattern (auto-derived from repeated intents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionPattern {
    pub id: String,
    pub category: IntentCategory,
    pub description: String,
    pub confidence: f32,
    pub observation_count: u32,
    pub last_observed: u64,
}

/// A learned user preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreference {
    pub id: String,
    /// "code_style" | "detail_level" | "communication_style" | …
    pub kind: String,
    pub value: String,
    pub confidence: f32,
    pub last_used: u64,
}

// ── Persistent JSON Store ────────────────────────────────────────────────

/// All intelligent data persisted to `.agent/intelligent.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct IntelStore {
    intents: VecDeque<UserIntent>,
    knowledge: Vec<KnowledgeFact>,
    patterns: Vec<InteractionPattern>,
    preferences: Vec<UserPreference>,
    total_interactions: u64,
    successful_interactions: u64,
}

impl IntelStore {
    fn load(project_dir: &Path) -> Self {
        let path = project_dir.join(INTEL_FILE);
        if let Ok(content) = std::fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self, project_dir: &Path) -> anyhow::Result<()> {
        let path = project_dir.join(INTEL_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    fn add_intent(&mut self, intent: UserIntent) {
        self.total_interactions += 1;
        if matches!(intent.outcome, IntentOutcome::Success { .. }) {
            self.successful_interactions += 1;
        }
        self.intents.push_front(intent);
        if self.intents.len() > MAX_INTENTS {
            self.intents.pop_back();
        }
        self.detect_patterns();
    }

    fn add_knowledge_fact(&mut self, topic: &str, fact: &str) {
        // Deduplicate by exact fact text
        if let Some(existing) = self.knowledge.iter_mut().find(|k| k.fact == fact) {
            existing.reference_count += 1;
            return;
        }
        self.knowledge.push(KnowledgeFact {
            id: new_id(),
            timestamp: now_ms(),
            topic: topic.to_string(),
            fact: fact.to_string(),
            reference_count: 1,
        });
        if self.knowledge.len() > MAX_KNOWLEDGE {
            // Evict least-referenced fact
            self.knowledge.sort_by_key(|k| k.reference_count);
            self.knowledge.remove(0);
        }
    }

    /// Auto-detect patterns from the most recent 30 intents.
    fn detect_patterns(&mut self) {
        let mut by_cat: HashMap<String, u32> = HashMap::new();
        for intent in self.intents.iter().take(30) {
            *by_cat.entry(format!("{:?}", intent.category)).or_default() += 1;
        }
        for (cat_key, count) in &by_cat {
            if *count < 3 {
                continue;
            }
            let confidence = (*count as f32 / 10.0).min(0.95);
            if let Some(p) = self.patterns.iter_mut().find(|p| format!("{:?}", p.category) == *cat_key) {
                p.observation_count = *count;
                p.confidence = confidence;
                p.last_observed = now_ms();
            } else if self.patterns.len() < MAX_PATTERNS {
                // Derive category from the matching intents
                let cat = self.intents.iter()
                    .find(|i| format!("{:?}", i.category) == *cat_key)
                    .map(|i| i.category.clone())
                    .unwrap_or(IntentCategory::Other);
                self.patterns.push(InteractionPattern {
                    id: new_id(),
                    category: cat.clone(),
                    description: format!("User frequently asks for {:?} tasks", cat),
                    confidence,
                    observation_count: *count,
                    last_observed: now_ms(),
                });
            }
        }
    }

    /// Build a context string from past intents and knowledge relevant to `query`.
    fn recall_relevant(&self, query: &str) -> String {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        if query_words.is_empty() {
            return String::new();
        }

        // Score intents by keyword overlap, boosting successes
        let mut scored: Vec<(f32, &UserIntent)> = self.intents.iter()
            .map(|intent| {
                let text = format!(
                    "{} {} {}",
                    intent.prompt, intent.intent_summary, intent.lessons.join(" ")
                ).to_lowercase();
                let overlap = query_words.iter().filter(|&&w| text.contains(w)).count();
                let mut score = overlap as f32 / query_words.len() as f32;
                if matches!(intent.outcome, IntentOutcome::Success { .. }) {
                    score *= 1.3;
                }
                (score, intent)
            })
            .filter(|(s, _)| *s > 0.25)
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Find knowledge matching any query word
        let mut relevant_knowledge: Vec<&KnowledgeFact> = self.knowledge.iter()
            .filter(|k| {
                let text = format!("{} {}", k.topic, k.fact).to_lowercase();
                query_words.iter().any(|&w| text.contains(w))
            })
            .collect();
        relevant_knowledge.sort_by(|a, b| b.reference_count.cmp(&a.reference_count));

        if scored.is_empty() && relevant_knowledge.is_empty() {
            return String::new();
        }

        let mut out = String::new();

        if !scored.is_empty() {
            out.push_str("## Similar Past Interactions\n");
            for (_, intent) in scored.iter().take(3) {
                out.push_str(&format!("- Prompt: {}\n", intent.prompt));
                out.push_str(&format!("  Intent: {}\n", intent.intent_summary));
                match &intent.outcome {
                    IntentOutcome::Success { summary } => {
                        out.push_str(&format!("  Result: ✓ {}\n", summary));
                    }
                    IntentOutcome::Failure { reason } => {
                        out.push_str(&format!("  Result: ✗ {}\n", reason));
                    }
                    IntentOutcome::PartialSuccess { achieved, .. } => {
                        out.push_str(&format!("  Result: ~ {}\n", achieved));
                    }
                    IntentOutcome::Unknown => {}
                }
                for lesson in &intent.lessons {
                    out.push_str(&format!("  Lesson: {}\n", lesson));
                }
                if let Some(fb) = &intent.feedback {
                    out.push_str(&format!("  Feedback: {}\n", fb));
                }
            }
            out.push('\n');
        }

        if !relevant_knowledge.is_empty() {
            out.push_str("## Relevant Knowledge\n");
            for k in relevant_knowledge.iter().take(5) {
                out.push_str(&format!("- [{}] {}\n", k.topic, k.fact));
            }
        }

        out
    }
}

// ── IntelligentMemory ────────────────────────────────────────────────────

/// Enhanced memory provider that combines:
/// - The reliable markdown log of `LocalFileMemory` (`.agent/memory.md`)
/// - A structured JSON episode store (`.agent/intelligent.json`)
///
/// Both stores persist across restarts. `recall_relevant` fuses results from
/// both, returning richer context than the plain keyword search of `LocalFileMemory`.
pub struct IntelligentMemory {
    base: Mutex<Memory>,
    intel: Mutex<IntelStore>,
    project_dir: PathBuf,
}

impl IntelligentMemory {
    pub fn load(project_dir: &Path) -> Self {
        Self {
            base: Mutex::new(Memory::load(project_dir)),
            intel: Mutex::new(IntelStore::load(project_dir)),
            project_dir: project_dir.to_path_buf(),
        }
    }

    fn save_intel(&self) {
        if let Err(e) = self.intel.lock().unwrap().save(&self.project_dir) {
            tracing::warn!("Failed to save intelligent memory: {}", e);
        }
    }
}

impl MemoryProvider for IntelligentMemory {
    fn record_event(&self, event: MemoryEvent) {
        let mut base = self.base.lock().unwrap();
        match &event {
            MemoryEvent::KnowledgeExtracted { facts } => {
                for fact in facts {
                    base.add_knowledge(fact);
                }
                base.log_action(&format!("extracted {} knowledge items", facts.len()));
            }
            MemoryEvent::FileRead { path } => {
                base.touch_file(path, "read");
                base.log_action(&format!("read {}", path));
            }
            MemoryEvent::FileWritten { path, lines } => {
                base.touch_file(path, &format!("written ({} lines)", lines));
                base.log_action(&format!("wrote {}", path));
            }
            MemoryEvent::FileEdited { path } => {
                base.touch_file(path, "edited");
                base.log_action(&format!("edited {}", path));
            }
            MemoryEvent::FileMultiEdited { path, edits } => {
                base.touch_file(path, &format!("multi-edited ({} edits)", edits));
                base.log_action(&format!("multi-edited {} ({} edits)", path, edits));
            }
            MemoryEvent::FileSearched { path } => {
                base.touch_file(path, "searched");
            }
            MemoryEvent::BatchFilesRead { paths } => {
                let count = paths.len();
                for p in paths {
                    base.touch_file(p, "read");
                }
                base.log_action(&format!("batch-read {} files", count));
            }
            MemoryEvent::PdfRead { path } => {
                base.touch_file(path, "read (PDF)");
                base.log_action(&format!("read PDF {}", path));
            }
            MemoryEvent::CommandRun { command } => {
                base.log_action(&format!("ran `{}`", command));
            }
            MemoryEvent::GrepSearch { pattern, path } => {
                base.log_action(&format!("searched for `{}`", pattern));
                if let Some(p) = path {
                    base.touch_file(p, "searched");
                }
            }
            MemoryEvent::FileFind { pattern } => {
                base.log_action(&format!("found files matching `{}`", pattern));
            }
            MemoryEvent::DirectoryListed { path } => {
                base.log_action(&format!("listed {}", path));
            }
            MemoryEvent::Custom { action } => {
                base.log_action(action);
            }
        }
        if let Err(e) = base.save() {
            tracing::warn!("Failed to auto-save base memory: {}", e);
        }
    }

    fn record_interaction(
        &self,
        prompt: &str,
        intent_summary: &str,
        tools_used: &[String],
        outcome_success: bool,
        outcome_detail: &str,
        lessons: &[String],
        feedback: Option<&str>,
        tokens: u32,
    ) {
        let outcome = if outcome_success {
            IntentOutcome::Success { summary: outcome_detail.to_string() }
        } else if outcome_detail.is_empty() {
            IntentOutcome::Unknown
        } else {
            IntentOutcome::Failure { reason: outcome_detail.to_string() }
        };

        let category = categorize_prompt(prompt);

        let mut base = self.base.lock().unwrap();
        base.log_action(&format!("completed: {}", intent_summary));
        drop(base);

        let mut intel = self.intel.lock().unwrap();
        intel.add_intent(UserIntent {
            id: new_id(),
            timestamp: now_ms(),
            prompt: prompt.to_string(),
            intent_summary: intent_summary.to_string(),
            category,
            tools_used: tools_used.to_vec(),
            outcome,
            lessons: lessons.to_vec(),
            feedback: feedback.map(str::to_string),
            tokens,
        });
        if let Err(e) = intel.save(&self.project_dir) {
            tracing::warn!("Failed to save intelligent memory: {}", e);
        }
    }

    fn log_truncation(&self, summary: &str) {
        let mut base = self.base.lock().unwrap();
        base.log_truncation_summary(summary);
        if let Err(e) = base.save() {
            tracing::warn!("Failed to save truncation summary: {}", e);
        }
    }

    fn recall(&self) -> String {
        self.base.lock().unwrap().to_system_prompt_knowledge()
    }

    /// Fuses results from both the markdown base and the JSON episode store.
    fn recall_relevant(&self, query: &str) -> String {
        let base_recall = self.base.lock().unwrap().recall_relevant(query);
        let intel_recall = self.intel.lock().unwrap().recall_relevant(query);
        match (base_recall.is_empty(), intel_recall.is_empty()) {
            (true, true) => String::new(),
            (true, false) => intel_recall,
            (false, true) => base_recall,
            (false, false) => format!("{}\n{}", base_recall, intel_recall),
        }
    }

    fn flush(&self) -> anyhow::Result<()> {
        self.base.lock().unwrap().save()?;
        self.intel.lock().unwrap().save(&self.project_dir)?;
        Ok(())
    }

    fn add_knowledge(&self, fact: &str) {
        let mut base = self.base.lock().unwrap();
        base.add_knowledge(fact);
        if let Err(e) = base.save() {
            tracing::warn!("Failed to save knowledge to base: {}", e);
        }
        drop(base);
        // Mirror into structured store under "general" topic
        self.intel.lock().unwrap().add_knowledge_fact("general", fact);
        self.save_intel();
    }

    fn is_empty(&self) -> bool {
        self.base.lock().unwrap().is_empty()
            && self.intel.lock().unwrap().intents.is_empty()
    }

    fn entry_count(&self) -> usize {
        self.base.lock().unwrap().entry_count()
            + self.intel.lock().unwrap().intents.len()
    }

    fn knowledge(&self) -> Vec<String> {
        self.base.lock().unwrap().knowledge.clone()
    }

    fn file_map(&self) -> Vec<(String, String)> {
        self.base.lock().unwrap().file_map
            .iter()
            .map(|e| (
                e.path.clone(),
                format!("{} (×{}, {})", e.description, e.access_count, e.last_accessed),
            ))
            .collect()
    }

    fn session_log(&self) -> Vec<String> {
        self.base.lock().unwrap().session_log.clone()
    }
}

// ── Public helpers ───────────────────────────────────────────────────────

/// Categorize a user prompt into an `IntentCategory`.
pub fn categorize_prompt(prompt: &str) -> IntentCategory {
    let p = prompt.to_lowercase();
    if p.contains("debug") || p.contains("error") || p.contains("issue") || p.contains("broken") {
        IntentCategory::Debug
    } else if p.contains("write") || p.contains("create") || p.contains("implement") || p.contains("add") {
        IntentCategory::CodeWrite
    } else if p.contains("modify") || p.contains("edit") || p.contains("change") || p.contains("update") || p.contains("refactor") {
        IntentCategory::CodeModify
    } else if p.contains("review") || p.contains("analyze") || p.contains("check") {
        IntentCategory::CodeReview
    } else if p.contains("run") || p.contains("execute") || p.contains("command") || p.contains("build") {
        IntentCategory::CommandRun
    } else if p.contains("explain") || p.contains("what") || p.contains("how does") || p.contains("why") {
        IntentCategory::Information
    } else if p.contains("plan") || p.contains("design") || p.contains("architect") {
        IntentCategory::Plan
    } else {
        IntentCategory::Other
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn new_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{:x}", d.as_nanos()))
        .unwrap_or_else(|_| "0".to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_round_trip_persistence() {
        let dir = tempdir().unwrap();
        let mem = IntelligentMemory::load(dir.path());

        mem.record_interaction(
            "Write a factorial function",
            "Create recursive factorial in Rust",
            &["write_file".to_string()],
            true,
            "Created factorial.rs with tests",
            &["User prefers recursive style".to_string()],
            Some("Perfect!"),
            120,
        );

        // Reload from disk
        let mem2 = IntelligentMemory::load(dir.path());
        let ctx = mem2.recall_relevant("how to write factorial");
        assert!(ctx.contains("factorial"), "recall should surface the recorded intent");
    }

    #[test]
    fn test_knowledge_deduplication() {
        let mut store = IntelStore::default();
        store.add_knowledge_fact("Rust", "Use `?` for error propagation");
        store.add_knowledge_fact("Rust", "Use `?` for error propagation");
        assert_eq!(store.knowledge.len(), 1);
        assert_eq!(store.knowledge[0].reference_count, 2);
    }

    #[test]
    fn test_pattern_detection() {
        let mut store = IntelStore::default();
        for _ in 0..4 {
            store.add_intent(UserIntent {
                id: new_id(),
                timestamp: now_ms(),
                prompt: "Write a function".to_string(),
                intent_summary: "Create code".to_string(),
                category: IntentCategory::CodeWrite,
                tools_used: vec![],
                outcome: IntentOutcome::Success { summary: "Done".to_string() },
                lessons: vec![],
                feedback: None,
                tokens: 50,
            });
        }
        assert!(!store.patterns.is_empty(), "should detect CodeWrite pattern");
    }

    #[test]
    fn test_categorize_prompt() {
        assert_eq!(categorize_prompt("debug this error"), IntentCategory::Debug);
        assert_eq!(categorize_prompt("write a parser"), IntentCategory::CodeWrite);
        assert_eq!(categorize_prompt("explain async/await"), IntentCategory::Information);
    }
}
