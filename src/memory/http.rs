//! HTTP-based external memory provider (stub).
//!
//! This module provides an `HttpMemory` backend for connecting to an external
//! memory service over HTTP. The struct fields and configuration are wired up;
//! actual network calls are left as a future implementation.

use super::provider::{MemoryEvent, MemoryProvider};

/// Configuration for the HTTP memory backend.
pub struct HttpMemoryConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub space_id: Option<String>,
    pub timeout_secs: u64,
}

/// Memory provider that delegates to a remote HTTP memory service.
pub struct HttpMemory {
    #[allow(dead_code)]
    config: HttpMemoryConfig,
}

impl HttpMemory {
    pub fn new(config: HttpMemoryConfig) -> Self {
        Self { config }
    }
}

impl MemoryProvider for HttpMemory {
    fn record_event(&self, _event: MemoryEvent) {
        // TODO: send event to remote service
    }

    fn log_truncation(&self, _summary: &str) {
        // TODO: persist truncation summary to remote service
    }

    fn recall(&self) -> String {
        String::new()
    }

    fn recall_relevant(&self, _query: &str) -> String {
        String::new()
    }

    fn flush(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn add_knowledge(&self, _fact: &str) {
        // TODO: push fact to remote service
    }

    fn is_empty(&self) -> bool {
        true
    }

    fn entry_count(&self) -> usize {
        0
    }

    fn knowledge(&self) -> Vec<String> {
        vec![]
    }

    fn file_map(&self) -> Vec<(String, String)> {
        vec![]
    }

    fn session_log(&self) -> Vec<String> {
        vec![]
    }
}
