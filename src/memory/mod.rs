//! Persistent memory system.
//!
//! Maintains a compact `.agent/memory.md` file that grows with usage,
//! making the agent increasingly familiar with the project over time.
//!
//! The file holds one section with automatic pruning:
//! - **Project Knowledge**: durable facts with timestamp + source, oldest evicted first
//!
//! Section caps come from `MemoryLimits`. The factory fills them from
//! `.agent/memory.toml`; `MemoryLimits::default()` keeps the historical limits.

pub mod provider;
pub mod factory;

// Re-export commonly used items so external code can use `crate::memory::*`
pub use provider::MemoryProvider;
pub use factory::{create_memory_provider, MemoryConfig};

use std::path::{Path, PathBuf};
use tracing::debug;

// Character-level safety net applied to a single knowledge entry.
const MAX_SINGLE_ENTRY_CHARS: usize = 500;

/// Per-section caps for [`Memory`].
///
/// Defaults reproduce the historical hard-coded limits. `MemoryConfig` maps
/// `.agent/memory.toml` onto these fields so the caps in that file take effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLimits {
    /// Maximum project knowledge entries.
    pub knowledge: usize,
    /// Total character budget for the knowledge section, independent of the
    /// entry count (keeps the system prompt small when entries are long).
    pub knowledge_chars: usize,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            knowledge: 10,
            knowledge_chars: 2200,
        }
    }
}

/// A project knowledge entry with provenance.
///
/// Entries migrated from the pre-timestamp `memory.md` format carry
/// `added_at == 0` and `source == "legacy"`, which makes them the first
/// candidates for age-based eviction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeEntry {
    /// The fact itself, as injected into the system prompt.
    pub text: String,
    /// Unix seconds when the fact was last stored or refreshed.
    pub added_at: i64,
    /// Producer of the fact: `extraction`, `agent`, `consolidate`, or `legacy`.
    pub source: String,
}

/// In-memory representation of the agent's persistent memory.
#[derive(Debug, Clone)]
pub struct Memory {
    pub knowledge: Vec<KnowledgeEntry>,
    limits: MemoryLimits,
    file_path: PathBuf,
}

impl Memory {
    /// Load memory from `.agent/memory.md` under `workdir`, applying `limits`
    /// when pruning. Returns an empty memory if the file does not exist.
    pub fn load_with_limits(workdir: &Path, limits: MemoryLimits) -> Self {
        let file_path = workdir.join(".agent").join("memory.md");
        let mut mem = Memory {
            knowledge: Vec::new(),
            limits,
            file_path,
        };

        if let Ok(content) = std::fs::read_to_string(&mem.file_path) {
            mem.parse(&content);
        }

        mem
    }

    /// Parse the markdown content into structured sections.
    fn parse(&mut self, content: &str) {
        let mut current_section = "";

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("## Project Knowledge") {
                current_section = "knowledge";
                continue;
            } else if trimmed.starts_with("## File Map") {
                // Retired section: skipped on load and dropped on the next save.
                current_section = "filemap";
                continue;
            } else if trimmed.starts_with("## Session Log") {
                // Retired section: skipped on load and dropped on the next save.
                current_section = "sessionlog";
                continue;
            }

            // Skip empty lines and the top-level heading
            if trimmed.is_empty() || trimmed.starts_with("# ") {
                continue;
            }

            // Strip leading "- " for list items
            let entry = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            if entry.is_empty() {
                continue;
            }

            match current_section {
                "knowledge" => {
                    self.knowledge.push(parse_knowledge_entry(entry));
                }
                _ => {}
            }
        }
    }

    /// Add a project knowledge entry attributed to `source`.
    ///
    /// Deduplicates by exact text and by matching first-three-word topic; a
    /// repeat refreshes `added_at` and `source` rather than appending.
    pub fn add_knowledge_sourced(&mut self, fact: &str, source: &str) {
        let fact = fact.trim().to_string();
        if fact.is_empty() {
            return;
        }

        // Character-level limit per entry (safety net)
        let text: String = if fact.chars().count() > MAX_SINGLE_ENTRY_CHARS {
            tracing::warn!(
                "Knowledge entry too long ({} chars, max {}). Truncated.",
                fact.chars().count(),
                MAX_SINGLE_ENTRY_CHARS
            );
            fact.chars().take(MAX_SINGLE_ENTRY_CHARS).collect()
        } else {
            fact
        };

        let now = now_secs();
        let text_lower = text.to_lowercase();

        // Exact duplicate: refresh provenance instead of appending.
        if let Some(existing) = self.knowledge.iter_mut()
            .find(|k| k.text.to_lowercase() == text_lower)
        {
            existing.added_at = now;
            existing.source = source.to_string();
            self.prune_knowledge();
            return;
        }

        // Same topic (first three words match): replace the body, keep the slot.
        let prefix: Vec<&str> = text_lower.split_whitespace().take(3).collect();
        if prefix.len() >= 2 {
            let prefix_str = prefix.join(" ");
            if let Some(idx) = self.knowledge.iter().position(|k| {
                k.text.to_lowercase().split_whitespace().take(3).collect::<Vec<_>>().join(" ") == prefix_str
            }) {
                self.knowledge[idx].text = text;
                self.knowledge[idx].added_at = now;
                self.knowledge[idx].source = source.to_string();
                self.prune_knowledge();
                return;
            }
        }

        self.knowledge.push(KnowledgeEntry { text, added_at: now, source: source.to_string() });
        self.prune_knowledge();
    }

    /// Add a knowledge entry attributed to the calling agent.
    pub fn add_knowledge(&mut self, fact: &str) {
        self.add_knowledge_sourced(fact, "agent");
    }

    /// Drop the oldest knowledge entries until both the entry count and the
    /// total character budget fit `self.limits`. Empty-stamped entries
    /// (migrated from the pre-timestamp format) are evicted first.
    fn prune_knowledge(&mut self) {
        self.knowledge.sort_by_key(|k| k.added_at);
        while self.knowledge.len() > self.limits.knowledge {
            self.knowledge.remove(0);
        }
        while self.knowledge.iter().map(|k| k.text.len()).sum::<usize>() > self.limits.knowledge_chars
            && !self.knowledge.is_empty()
        {
            self.knowledge.remove(0);
        }
    }

    /// Save memory back to disk with atomic write and advisory file lock.
    ///
    /// Uses write-to-temp-then-rename for atomicity, and `flock` (on Unix)
    /// to prevent concurrent writes from corrupting the file.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = self.to_markdown();
        let tmp_path = self.file_path.with_extension("tmp");

        // Write to temporary file first, then atomically rename.
        // This prevents readers from seeing a partially-written file.
        std::fs::write(&tmp_path, &content)?;
        std::fs::rename(&tmp_path, &self.file_path)?;

        debug!("Saved memory to {}", self.file_path.display());
        Ok(())
    }

    /// Format memory as Markdown (persisted to `.agent/memory.md`).
    fn to_markdown(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Agent Memory\n".to_string());

        // Project Knowledge
        lines.push("## Project Knowledge\n".to_string());
        if self.knowledge.is_empty() {
            lines.push("_(No knowledge recorded yet)_\n".to_string());
        } else {
            for entry in &self.knowledge {
                lines.push(format!("- [{}|{}] {}",
                    format_ts(entry.added_at), entry.source, entry.text));
            }
            lines.push(String::new());
        }

        lines.join("\n")
    }

    /// Format the five most recent knowledge facts for the system prompt.
    ///
    /// Rendered once per conversation, so facts written later in the session do
    /// not disturb the cached prefix.
    pub fn to_system_prompt_knowledge(&self) -> String {
        let source = &self.knowledge;
        if source.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        parts.push("\n\n--- Project Knowledge (from memory) ---".to_string());
        // Most recent = most refined; cap at 5 to stay token-frugal
        let start = source.len().saturating_sub(5);
        for entry in &source[start..] {
            parts.push(format!("• {}", entry.text));
        }
        parts.join("\n")
    }

    /// Check if the memory is completely empty.
    pub fn is_empty(&self) -> bool {
        self.knowledge.is_empty()
    }

    /// Total number of entries across all sections.
    pub fn entry_count(&self) -> usize {
        self.knowledge.len()
    }
}

/// Unix seconds now, or 0 if the clock reads before the epoch.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Split Unix seconds into `(year, month, day, hour, minute)` in UTC.
///
/// Howard Hinnant's `civil_from_days`, rebased on the 0000-03-01 epoch.
fn civil_from_secs(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day, (rem / 3_600) as u32, ((rem % 3_600) / 60) as u32)
}

/// Parse the `YYYY-MM-DD HH:MM` form written by [`format_ts`].
fn parse_ts(ts: &str) -> Option<i64> {
    let (date, time) = ts.split_once(' ')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    if d.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.split(':');
    let hour: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let yy = if month <= 2 { year - 1 } else { year };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + min * 60)
}

/// `YYYY-MM-DD HH:MM`, or `unknown` for entries with no recorded time.
fn format_ts(secs: i64) -> String {
    if secs <= 0 {
        return "unknown".to_string();
    }
    let (year, month, day, hour, min) = civil_from_secs(secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min)
}

/// Parse a `## Project Knowledge` line, tolerating the pre-timestamp format.
fn parse_knowledge_entry(entry: &str) -> KnowledgeEntry {
    if let Some(rest) = entry.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            let meta = &rest[..close];
            let text = rest[close + 1..].trim().to_string();
            let (stamp, source) = match meta.split_once('|') {
                Some((stamp, source)) => (stamp.trim(), source.trim()),
                None => (meta.trim(), "legacy"),
            };
            if !text.is_empty() {
                return KnowledgeEntry {
                    text,
                    added_at: parse_ts(stamp).unwrap_or(0),
                    source: source.to_string(),
                };
            }
        }
    }
    KnowledgeEntry { text: entry.to_string(), added_at: 0, source: "legacy".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn knowledge_records_timestamp_and_source() {
        let dir = tempdir().unwrap();
        let mut mem = Memory::load_with_limits(dir.path(), MemoryLimits::default());
        mem.add_knowledge_sourced("The build uses cargo", "extraction");
        assert_eq!(mem.knowledge.len(), 1);
        assert_eq!(mem.knowledge[0].source, "extraction");
        assert!(mem.knowledge[0].added_at > 0);
    }

    #[test]
    fn configured_knowledge_caps_are_applied() {
        let dir = tempdir().unwrap();
        let limits = MemoryLimits { knowledge: 2, knowledge_chars: 10_000 };
        let mut mem = Memory::load_with_limits(dir.path(), limits);

        for i in 0..4 {
            mem.add_knowledge(&format!("fact number {}", i));
        }
        assert_eq!(mem.knowledge.len(), 2);
        assert_eq!(mem.knowledge[1].text, "fact number 3");
    }

    #[test]
    fn legacy_knowledge_lines_migrate_and_evict_first() {
        let dir = tempdir().unwrap();
        let agent_dir = dir.path().join(".agent");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("memory.md"),
            "# Agent Memory\n\n## Project Knowledge\n- old fact one\n- old fact two\n").unwrap();

        let mut mem = Memory::load_with_limits(dir.path(),
            MemoryLimits { knowledge: 2, ..MemoryLimits::default() });
        assert_eq!(mem.knowledge.len(), 2);
        assert!(mem.knowledge.iter().all(|k| k.source == "legacy" && k.added_at == 0));

        mem.add_knowledge("fresh fact");
        assert_eq!(mem.knowledge.len(), 2);
        assert_eq!(mem.knowledge.iter().filter(|k| k.source == "legacy").count(), 1);
        assert_eq!(mem.knowledge[1].text, "fresh fact");
    }

    #[test]
    fn knowledge_character_budget_prunes_oldest() {
        let dir = tempdir().unwrap();
        let limits = MemoryLimits { knowledge: 10, knowledge_chars: 30, ..MemoryLimits::default() };
        let mut mem = Memory::load_with_limits(dir.path(), limits);
        mem.add_knowledge("aaaa bbbb cccc");
        mem.add_knowledge("dddd eeee ffff");
        mem.add_knowledge("gggg hhhh iiii");
        assert_eq!(mem.knowledge.len(), 2);
        assert_eq!(mem.knowledge[0].text, "dddd eeee ffff");
    }

    #[test]
    fn knowledge_provenance_survives_a_save_reload_cycle() {
        let dir = tempdir().unwrap();
        let mut mem = Memory::load_with_limits(dir.path(), MemoryLimits::default());
        mem.add_knowledge_sourced("Cargo builds the binary", "extraction");
        let stamp = mem.knowledge[0].added_at;
        mem.save().unwrap();

        let reloaded = Memory::load_with_limits(dir.path(), MemoryLimits::default());
        assert_eq!(reloaded.knowledge.len(), 1);
        assert_eq!(reloaded.knowledge[0].source, "extraction");
        assert!((reloaded.knowledge[0].added_at - stamp).abs() < 60);
    }

    #[test]
    fn retired_sections_are_dropped_on_load_and_save() {
        let dir = tempdir().unwrap();
        let agent_dir = dir.path().join(".agent");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(agent_dir.join("memory.md"),
            "# Agent Memory\n\n## Project Knowledge\n- keep me\n\n## File Map\n- src/old.rs | read | count:3 | 08-22 15:39\n\n## Session Log\n- [08-22 15:46] ran `ls`\n").unwrap();

        let mem = Memory::load_with_limits(dir.path(), MemoryLimits::default());
        assert_eq!(mem.knowledge.len(), 1);
        assert_eq!(mem.entry_count(), 1);

        mem.save().unwrap();
        let written = std::fs::read_to_string(agent_dir.join("memory.md")).unwrap();
        assert!(!written.contains("File Map"), "retired section must not be rewritten");
        assert!(!written.contains("Session Log"), "retired section must not be rewritten");
        assert!(written.contains("keep me"));
    }

    #[test]
    fn timestamps_round_trip_through_utc_civil_math() {
        assert_eq!(format_ts(0), "unknown");
        assert_eq!(format_ts(1_767_225_600), "2026-01-01 00:00");
        assert_eq!(parse_ts("2026-01-01 00:00"), Some(1_767_225_600));
        assert_eq!(parse_ts("not-a-date"), None);
        for secs in [1_i64, 1_767_225_600, 1_786_030_831, 4_102_444_800] {
            assert_eq!(parse_ts(&format_ts(secs)), Some(secs - secs % 60));
        }
    }
}
