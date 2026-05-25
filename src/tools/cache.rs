//! Tool result cache — Layer 1 of the 3-layer persistence system.
//!
//! Caches results from **read-only** tools so that repeated identical calls
//! (e.g. re-reading the same file) skip the actual operation.  Write tools
//! (write_file, edit_file, multi_edit_file) automatically invalidate cache
//! entries for the paths they touch.
//!
//! ## 3-Layer Design
//! 1. **Memory cache** (this module) — fast, ephemeral, per-session
//! 2. **Result size limits** — prevent context pollution from huge outputs
//! 3. **Session log persistence** — already handled by `record_tool_to_memory`

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::ToolResult;

/// Tools whose results are safe to cache (pure read operators).
const CACHEABLE_TOOLS: &[&str] = &[
    "read_file",
    "grep_search",
    "file_search",
    "list_directory",
    "batch_read_files",
    "read_pdf",
    "todo_read",
    "list_nodes",
    "list_services",
];

/// Maximum characters before a tool result is truncated (Layer 2).
const MAX_RESULT_CHARS: usize = 20_000;

/// A single cached tool result.
struct CachedResult {
    result: ToolResult,
    timestamp: Instant,
}

/// In-memory cache for read-only tool results.
///
/// Uses LRU-like eviction: when the cache reaches `max_entries`, the oldest
/// entry is evicted.  Entries also expire after `ttl`.
pub struct ToolResultCache {
    entries: HashMap<(String, u64), CachedResult>,
    /// Track which cache keys are associated with each file path,
    /// so write operations can selectively invalidate.
    file_keys: HashMap<PathBuf, Vec<(String, u64)>>,
    max_entries: usize,
    ttl: Duration,
    hits: u64,
    misses: u64,
}

impl ToolResultCache {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            file_keys: HashMap::new(),
            max_entries,
            ttl: Duration::from_secs(ttl_secs),
            hits: 0,
            misses: 0,
        }
    }

    /// Check if a tool name is eligible for caching.
    pub fn is_cacheable(tool_name: &str) -> bool {
        CACHEABLE_TOOLS.contains(&tool_name)
    }

    /// Look up a cached result.  Returns `None` if not found or expired.
    pub fn get(&mut self, tool_name: &str, args_hash: u64) -> Option<ToolResult> {
        let key = (tool_name.to_string(), args_hash);
        if let Some(entry) = self.entries.get(&key) {
            if entry.timestamp.elapsed() < self.ttl {
                self.hits += 1;
                return Some(entry.result.clone());
            }
            // Expired — remove
            self.entries.remove(&key);
        }
        self.misses += 1;
        None
    }

    /// Store a tool result in the cache.  Optionally associates it with a file
    /// path so that subsequent writes to that path will invalidate it.
    pub fn put(
        &mut self,
        tool_name: &str,
        args_hash: u64,
        file_path: Option<&Path>,
        result: ToolResult,
    ) {
        let key = (tool_name.to_string(), args_hash);

        // Evict oldest entry if at capacity
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&key) {
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.timestamp)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }

        // Track file→key mapping
        if let Some(path) = file_path {
            self.file_keys
                .entry(path.to_path_buf())
                .or_default()
                .push(key.clone());
        }

        self.entries.insert(
            key,
            CachedResult {
                result,
                timestamp: Instant::now(),
            },
        );
    }

    /// Invalidate all cached reads that touch `path`.
    ///
    /// Called after write_file / edit_file / multi_edit_file so the next read
    /// will see the updated content.
    pub fn invalidate_path(&mut self, path: &Path) {
        // Exact match
        if let Some(keys) = self.file_keys.remove(path) {
            for key in &keys {
                self.entries.remove(key);
            }
        }
        // Prefix match (e.g. write to "src/" invalidates all "src/..." reads)
        let prefix_keys: Vec<PathBuf> = self
            .file_keys
            .keys()
            .filter(|k| k.starts_with(path))
            .cloned()
            .collect();
        for pk in prefix_keys {
            if let Some(keys) = self.file_keys.remove(&pk) {
                for key in &keys {
                    self.entries.remove(key);
                }
            }
        }
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.file_keys.clear();
    }

    /// Return cache hit/miss stats.
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}

/// Apply Layer-2 truncation: if the result exceeds `MAX_RESULT_CHARS`,
/// truncate it and append a summary note.
pub fn enforce_result_size_limit(result: &mut ToolResult) {
    if result.output.len() > MAX_RESULT_CHARS {
        let original_len = result.output.len();
        // Truncate to MAX_RESULT_CHARS, trying to break at a line boundary
        let truncate_at = result
            .output
            .char_indices()
            .take(MAX_RESULT_CHARS)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(MAX_RESULT_CHARS);
        result.output.truncate(truncate_at);
        result.output.push_str(&format!(
            "\n\n... [truncated: {} total chars, showing first {}]",
            original_len,
            truncate_at
        ));
    }
}

/// Hash tool arguments for cache keys.  Uses the same deterministic hashing
/// as Agent::hash_tool_args to ensure consistency.
pub fn hash_args(tool_name: &str, input: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut hasher);
    input.to_string().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let mut cache = ToolResultCache::new(10, 60);
        let hash = hash_args("read_file", &serde_json::json!({"path": "test.txt"}));

        assert!(cache.get("read_file", hash).is_none());
        cache.put(
            "read_file",
            hash,
            Some(Path::new("test.txt")),
            ToolResult::success("hello"),
        );
        let result = cache.get("read_file", hash);
        assert!(result.is_some());
        assert_eq!(result.unwrap().output, "hello");
    }

    #[test]
    fn test_invalidation_on_write() {
        let mut cache = ToolResultCache::new(10, 60);
        let hash = hash_args("read_file", &serde_json::json!({"path": "src/main.rs"}));

        cache.put(
            "read_file",
            hash,
            Some(Path::new("src/main.rs")),
            ToolResult::success("old content"),
        );

        // Write should invalidate
        cache.invalidate_path(Path::new("src/main.rs"));
        assert!(cache.get("read_file", hash).is_none());
    }

    #[test]
    fn test_result_truncation() {
        let mut result = ToolResult::success("x".repeat(25_000));
        enforce_result_size_limit(&mut result);
        assert!(result.output.len() <= MAX_RESULT_CHARS + 100); // +100 for truncation note
        assert!(result.output.contains("truncated"));
    }

    #[test]
    fn test_cacheable_tools() {
        assert!(ToolResultCache::is_cacheable("read_file"));
        assert!(ToolResultCache::is_cacheable("grep_search"));
        assert!(!ToolResultCache::is_cacheable("write_file"));
        assert!(!ToolResultCache::is_cacheable("run_command"));
        assert!(!ToolResultCache::is_cacheable("edit_file"));
    }
}
