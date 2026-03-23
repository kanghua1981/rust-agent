//! Model management: persistent multi-model configuration via `models.toml`.
//!
//! Configuration lives at `~/.config/rust_agent/models.toml` (user-level).
//! Each model entry has an alias, provider, model name, and optional overrides
//! for `base_url` and `api_key`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Provider;

// ── Data types ───────────────────────────────────────────────────────

/// Top-level structure of `models.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsConfig {
    /// Alias of the default model (e.g. "sonnet").
    #[serde(default)]
    pub default: Option<String>,

    /// Named model entries keyed by alias.
    #[serde(default)]
    pub models: BTreeMap<String, ModelEntry>,

    /// Extra bind-mounts injected into every worker container.
    /// Example in models.toml:
    ///   [[extra_binds]]
    ///   host   = "/home/user/.rustup"
    ///   target = "/.rustup"
    ///   [[extra_binds]]
    ///   host   = "/home/user/.cargo"
    ///   target = "/.cargo"
    #[serde(default)]
    pub extra_binds: Vec<crate::container::ExtraBindMount>,

    /// Role definitions (planner, executor, checker, or any custom name).
    #[serde(default)]
    pub roles: BTreeMap<String, RoleConfig>,

    /// Multi-role pipeline configuration.
    #[serde(default)]
    pub pipeline: Option<PipelineConfig>,

    /// Configured sub-agents to start (alias, port, role).
    #[serde(default)]
    pub sub_agents: BTreeMap<String, crate::config::SubAgentConfig>,
}

/// Configuration for a single named role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    /// Model alias to use for this role (references a key in `[models]`).
    pub model: String,
    /// Fully custom system prompt. If set, replaces the built-in default for
    /// this role (equivalent to `# OVERRIDE` in a prompt file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Extra instructions appended to the final system prompt (highest priority).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_instructions: Option<String>,
}

/// Multi-role pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    /// When true every user message is routed through the full pipeline.
    #[serde(default)]
    pub enabled: bool,
    /// Routing strategy: "auto" (adaptive), "always_pipeline", "always_simple".
    /// When set to "auto", the router classifies each user message and
    /// picks the cheapest execution mode that fits the task complexity.
    /// Defaults to following the `enabled` flag for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<String>,
    /// Ordered list of role names that form the pipeline stages.
    /// Must all have entries in `[roles]`. Defaults to
    /// `["planner", "executor", "checker"]` when empty.
    #[serde(default)]
    pub stages: Vec<String>,
    /// How many times the executor may retry after a checker FAIL.
    /// Defaults to 2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checker_retries: Option<u32>,
    /// If true the pipeline pauses after planning to show the plan and
    /// ask the user for confirmation before executing. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_plan_confirm: Option<bool>,
}

impl PipelineConfig {
    pub fn effective_stages(&self) -> Vec<&str> {
        if self.stages.is_empty() {
            vec!["planner", "executor", "checker"]
        } else {
            self.stages.iter().map(|s| s.as_str()).collect()
        }
    }

    pub fn max_retries(&self) -> u32 {
        self.max_checker_retries.unwrap_or(2)
    }

    pub fn confirm_plan(&self) -> bool {
        self.require_plan_confirm.unwrap_or(true)
    }

    /// Resolve the effective router mode.
    ///
    /// Priority: explicit `router` field > `enabled` flag.
    /// - `router = "auto"` → adaptive routing regardless of `enabled`.
    /// - `router` absent + `enabled = true` → AlwaysPipeline (backward compat).
    /// - `router` absent + `enabled = false` → AlwaysSimple.
    pub fn router_mode(&self) -> crate::router::RouterMode {
        if let Some(ref r) = self.router {
            r.parse().unwrap_or_else(|_| {
                tracing::warn!("Unknown router mode '{}', falling back to auto", r);
                crate::router::RouterMode::Auto
            })
        } else if self.enabled {
            crate::router::RouterMode::AlwaysPipeline
        } else {
            crate::router::RouterMode::AlwaysSimple
        }
    }
}

/// A single model entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Resolved information returned after looking up a model alias.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub alias: String,
    pub provider: Provider,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub max_tokens: Option<u32>,
}

// ── File path helper ─────────────────────────────────────────────────

/// Return the path to `~/.config/rust_agent/models.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rust_agent").join("models.toml"))
}

// ── Read / Write ─────────────────────────────────────────────────────

/// Load `models.toml` from the standard location.
/// Returns a default (empty) config if the file does not exist.
pub fn load() -> ModelsConfig {
    let Some(path) = config_path() else {
        return ModelsConfig::default();
    };
    if !path.exists() {
        return ModelsConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("⚠️  Failed to parse {}: {}", path.display(), e);
            ModelsConfig::default()
        }),
        Err(e) => {
            eprintln!("⚠️  Failed to read {}: {}", path.display(), e);
            ModelsConfig::default()
        }
    }
}

/// Persist the current `ModelsConfig` to disk.
pub fn save(cfg: &ModelsConfig) -> Result<()> {
    let path = config_path().context("Cannot determine config directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, content)?;
    Ok(())
}

// ── Query helpers ────────────────────────────────────────────────────

impl ModelsConfig {
    /// Resolve an alias to a full `ResolvedModel`.
    pub fn resolve(&self, alias: &str) -> Option<ResolvedModel> {
        let entry = self.models.get(alias)?;
        let provider = parse_provider(&entry.provider);
        Some(ResolvedModel {
            alias: alias.to_string(),
            provider,
            model: entry.model.clone(),
            base_url: entry.base_url.clone(),
            api_key: entry.api_key.clone(),
            max_tokens: entry.max_tokens,
        })
    }

    /// Resolve the default model, if one is configured.
    pub fn resolve_default(&self) -> Option<ResolvedModel> {
        let alias = self.default.as_deref()?;
        self.resolve(alias)
    }

    /// List all configured aliases (sorted).
    pub fn aliases(&self) -> Vec<&str> {
        self.models.keys().map(|s| s.as_str()).collect()
    }

    /// Add or overwrite a model entry.
    pub fn add(&mut self, alias: String, entry: ModelEntry) {
        self.models.insert(alias, entry);
    }

    /// Remove a model entry. Returns `true` if it existed.
    pub fn remove(&mut self, alias: &str) -> bool {
        let existed = self.models.remove(alias).is_some();
        // Clear default if it pointed to the removed alias
        if self.default.as_deref() == Some(alias) {
            self.default = None;
        }
        existed
    }

    /// Set the default alias.
    pub fn set_default(&mut self, alias: String) {
        self.default = Some(alias);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn parse_provider(s: &str) -> Provider {
    match s.to_lowercase().as_str() {
        "anthropic" => Provider::Anthropic,
        "openai" => Provider::OpenAI,
        "compatible" => Provider::Compatible,
        _ => Provider::Compatible,
    }
}
