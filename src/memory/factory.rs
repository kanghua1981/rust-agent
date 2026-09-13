//! Memory provider factory and configuration.
//!
//! Backend selection and the section caps applied to `.agent/memory.md`.

use std::path::Path;
use std::sync::Arc;

use super::provider::{LocalFileMemory, MemoryProvider, NullMemory};
use super::MemoryLimits;

/// Memory backend type.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum MemoryBackend {
    /// Project knowledge in `.agent/memory.md`.
    LocalFile,
    /// No-op memory for tests and ephemeral runs.
    Null,
}

/// Configuration for the memory system.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MemoryConfig {
    /// Memory backend type.
    pub backend: MemoryBackend,

    /// Maximum knowledge entries.
    #[serde(default = "default_max_knowledge")]
    pub max_knowledge: usize,

    /// Character budget for the knowledge section (all entries combined).
    #[serde(default = "default_max_knowledge_chars")]
    pub max_knowledge_chars: usize,

    /// Knowledge extraction frequency (every N turns).
    #[serde(default = "default_extraction_frequency")]
    pub extraction_frequency: usize,
}

fn default_max_knowledge() -> usize { 10 }
fn default_max_knowledge_chars() -> usize { 2200 }
fn default_extraction_frequency() -> usize { 5 }

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: MemoryBackend::LocalFile,
            max_knowledge: default_max_knowledge(),
            max_knowledge_chars: default_max_knowledge_chars(),
            extraction_frequency: default_extraction_frequency(),
        }
    }
}

impl MemoryConfig {
    /// Section caps handed to the markdown store.
    pub fn limits(&self) -> MemoryLimits {
        MemoryLimits {
            knowledge: self.max_knowledge,
            knowledge_chars: self.max_knowledge_chars,
        }
    }
}

/// Create the memory provider selected by `config`.
pub fn create_memory_provider(config: &MemoryConfig, project_dir: &Path) -> Arc<dyn MemoryProvider> {
    match config.backend {
        MemoryBackend::LocalFile => {
            Arc::new(LocalFileMemory::load_with_limits(project_dir, config.limits()))
        }
        MemoryBackend::Null => Arc::new(NullMemory),
    }
}

/// Load memory configuration from `config_path`, or from `.agent/memory.toml`
/// when no explicit path is given. Missing or unparsable files yield defaults.
pub fn load_memory_config(config_path: Option<&Path>) -> MemoryConfig {
    // An explicit path comes from the resolved project directory. It must not
    // fall through to the working directory: that would load the configuration
    // of whichever project happens to be the current directory.
    let path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(".agent").join("memory.toml"));

    let Ok(content) = std::fs::read_to_string(&path) else {
        return MemoryConfig::default();
    };
    match toml::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("Failed to parse memory config {}: {}; using defaults", path.display(), e);
            MemoryConfig::default()
        }
    }
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
        assert_eq!(config.max_knowledge_chars, 2200);
        assert_eq!(config.extraction_frequency, 5);
    }

    #[test]
    fn test_create_local_memory() {
        let temp_dir = tempdir().unwrap();
        let memory = create_memory_provider(&MemoryConfig::default(), temp_dir.path());
        assert!(memory.is_empty());
        assert_eq!(memory.entry_count(), 0);
    }

    #[test]
    fn test_configured_caps_reach_the_provider() {
        let temp_dir = tempdir().unwrap();
        // These caps used to be dead config: the store pruned with hard-coded
        // constants instead of the values parsed from `.agent/memory.toml`.
        let config: MemoryConfig = toml::from_str(
            "backend = \"LocalFile\"\nmax_knowledge = 2\n",
        ).unwrap();

        let memory = create_memory_provider(&config, temp_dir.path());
        for i in 0..3 {
            memory.add_knowledge(&format!("fact number {}", i));
        }
        let knowledge = memory.knowledge();
        assert_eq!(knowledge.len(), 2);
        assert_eq!(knowledge[1], "fact number 2");
    }

    #[test]
    fn explicit_config_path_does_not_fall_back_to_the_working_directory() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("absent.toml");
        let config = load_memory_config(Some(&missing));
        assert_eq!(config.max_knowledge, MemoryConfig::default().max_knowledge);
    }

    #[test]
    fn unparsable_config_falls_back_to_defaults() {
        let dir = tempdir().unwrap();
        let broken = dir.path().join("memory.toml");
        std::fs::write(&broken, "backend = 12\n").unwrap();
        let config = load_memory_config(Some(&broken));
        assert!(matches!(config.backend, MemoryBackend::LocalFile));
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_content = r#"
backend = "Null"
max_knowledge = 15
max_knowledge_chars = 4096
extraction_frequency = 2
"#;

        let config: MemoryConfig = toml::from_str(toml_content).unwrap();
        assert!(matches!(config.backend, MemoryBackend::Null));
        assert_eq!(config.max_knowledge, 15);
        assert_eq!(config.max_knowledge_chars, 4096);
        assert_eq!(config.extraction_frequency, 2);
    }
}
