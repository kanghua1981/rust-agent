//! Memory provider factory and configuration.
//!
//! Supports multiple memory backends with configuration-driven selection.

use std::path::Path;
use std::sync::Arc;

use super::provider::{LocalFileMemory, MemoryProvider, NullMemory};
use super::http::HttpMemory;
use super::intelligent::IntelligentMemory;

/// Memory backend type
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MemoryBackend {
    /// Local file-based memory (.agent/memory.md)
    LocalFile,
    /// Enriched provider: markdown log + JSON episode store (.agent/intelligent.json)
    Intelligent,
    /// HTTP-based external memory service
    Http,
    /// No-op memory for tests/sandboxes
    Null,
}

/// Configuration for memory system
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MemoryConfig {
    /// Memory backend type
    pub backend: MemoryBackend,
    
    /// HTTP memory configuration (if backend is Http)
    #[serde(default)]
    pub http: HttpMemoryConfig,
    
    /// Maximum knowledge entries
    #[serde(default = "default_max_knowledge")]
    pub max_knowledge: usize,
    
    /// Maximum file map entries
    #[serde(default = "default_max_file_map")]
    pub max_file_map: usize,
    
    /// Maximum session log entries
    #[serde(default = "default_max_session_log")]
    pub max_session_log: usize,
    
    /// Knowledge extraction frequency (every N turns)
    #[serde(default = "default_extraction_frequency")]
    pub extraction_frequency: usize,
}

/// HTTP memory configuration
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct HttpMemoryConfig {
    /// Base URL of the memory service
    pub base_url: Option<String>,
    
    /// API key for authentication
    pub api_key: Option<String>,
    
    /// Space/workspace ID
    pub space_id: Option<String>,
    
    /// Request timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    
    /// Whether to enable async event sending
    #[serde(default = "default_async_events")]
    pub async_events: bool,
    
    /// Batch size for event sending
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    
    /// Cache TTL for recall results (seconds)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: u64,
}

// Default values
fn default_max_knowledge() -> usize { 10 }
fn default_max_file_map() -> usize { 20 }
fn default_max_session_log() -> usize { 30 }
fn default_extraction_frequency() -> usize { 5 }
fn default_timeout_secs() -> u64 { 30 }
fn default_async_events() -> bool { true }
fn default_batch_size() -> usize { 10 }
fn default_cache_ttl() -> u64 { 300 } // 5 minutes

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: MemoryBackend::LocalFile,
            http: HttpMemoryConfig::default(),
            max_knowledge: default_max_knowledge(),
            max_file_map: default_max_file_map(),
            max_session_log: default_max_session_log(),
            extraction_frequency: default_extraction_frequency(),
        }
    }
}

/// Create a memory provider based on configuration
pub fn create_memory_provider(
    config: &MemoryConfig,
    project_dir: &Path,
) -> anyhow::Result<Arc<dyn MemoryProvider>> {
    match config.backend {
        MemoryBackend::LocalFile => {
            Ok(Arc::new(LocalFileMemory::load(project_dir)))
        }
        MemoryBackend::Intelligent => {
            Ok(Arc::new(IntelligentMemory::load(project_dir)))
        }
        MemoryBackend::Http => {
            let http_config = super::http::HttpMemoryConfig {
                base_url: config.http.base_url.clone()
                    .ok_or_else(|| anyhow::anyhow!("HTTP memory requires base_url"))?,
                api_key: config.http.api_key.clone(),
                space_id: config.http.space_id.clone(),
                timeout_secs: config.http.timeout_secs,
            };
            Ok(Arc::new(HttpMemory::new(http_config)))
        }
        MemoryBackend::Null => {
            Ok(Arc::new(NullMemory))
        }
    }
}

/// Load memory configuration from file or use defaults
pub fn load_memory_config(config_path: Option<&Path>) -> MemoryConfig {
    if let Some(path) = config_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            match toml::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    tracing::warn!("Failed to parse memory config {}: {}", path.display(), e);
                }
            }
        }
    }
    
    // Try to load from default location
    let default_path = Path::new(".agent").join("memory.toml");
    if let Ok(content) = std::fs::read_to_string(&default_path) {
        match toml::from_str(&content) {
            Ok(config) => return config,
            Err(e) => {
                tracing::warn!("Failed to parse default memory config: {}", e);
            }
        }
    }
    
    // Fall back to defaults
    MemoryConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_default_config() {
        let config = MemoryConfig::default();
        assert!(matches!(config.backend, MemoryBackend::LocalFile));
        assert_eq!(config.max_knowledge, 10);
        assert_eq!(config.max_file_map, 20);
        assert_eq!(config.max_session_log, 30);
        assert_eq!(config.extraction_frequency, 5);
    }
    
    #[test]
    fn test_create_local_memory() {
        let temp_dir = tempdir().unwrap();
        let config = MemoryConfig::default();

        let memory = create_memory_provider(&config, temp_dir.path()).unwrap();
        // A freshly created provider with no recorded events is empty
        assert!(memory.is_empty());
        assert_eq!(memory.entry_count(), 0);
    }
    
    #[test]
    fn test_parse_config_toml() {
        let toml_content = r#"
backend = "Http"
max_knowledge = 15
max_file_map = 25

[http]
base_url = "http://localhost:8080"
api_key = "test-key"
space_id = "test-space"
timeout_secs = 60
async_events = true
batch_size = 20
cache_ttl = 600
"#;
        
        let config: MemoryConfig = toml::from_str(toml_content).unwrap();
        assert!(matches!(config.backend, MemoryBackend::Http));
        assert_eq!(config.max_knowledge, 15);
        assert_eq!(config.max_file_map, 25);
        assert_eq!(config.http.base_url, Some("http://localhost:8080".to_string()));
        assert_eq!(config.http.api_key, Some("test-key".to_string()));
        assert_eq!(config.http.timeout_secs, 60);
        assert_eq!(config.http.batch_size, 20);
    }
}