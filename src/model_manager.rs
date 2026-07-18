//! Model management: persistent multi-model configuration via `models.toml`.
//!
//! Configuration lives at `~/.config/rust_agent/models.toml` (user-level).
//!
//! Two styles are supported:
//! 1. **Flat** (legacy): each `[models.<alias>]` carries its own provider/base_url/api_key.
//! 2. **Endpoint-referenced** (new): `[endpoints.<name>]` defines shared connection
//!    parameters; model entries only need `endpoint = "<name>"` + `model`.
//!    Model-level fields (base_url, api_key, provider) override endpoint values.

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

    /// Named endpoint definitions (shared base_url + api_key).
    /// Models can reference these via `endpoint = "<name>"` to avoid repetition.
    #[serde(default)]
    pub endpoints: BTreeMap<String, EndpointEntry>,

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

/// A named endpoint that models can reference to avoid repeating
/// base_url / api_key / provider across many model entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointEntry {
    /// Provider: "anthropic", "openai", or "compatible".
    pub provider: String,
    /// Base URL of the API (e.g. "https://api.openai.com/v1").
    pub base_url: String,
    /// API key for this endpoint (optional — falls back to env vars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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

/// Multi-role pipeline configuration (in models.toml).
///
/// NOTE: With the DAG-based pipeline system, the pipeline stage definitions
/// have moved to `.agent/pipelines/*.toml`. This struct now only holds the
/// routing preferences and which pipeline to use by default.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    /// When true every user message is routed through the full pipeline.
    /// Deprecated: prefer using `router = "always_pipeline"`.
    #[serde(default)]
    pub enabled: bool,
    /// Routing strategy: "auto" (adaptive), "always_pipeline", "always_simple".
    /// When set to "auto", the router classifies each user message and
    /// picks the cheapest execution mode that fits the task complexity.
    /// Defaults to following the `enabled` flag for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<String>,
    /// Name of the default pipeline to load from `.agent/pipelines/`.
    /// Defaults to `"default"` (the built-in 3-stage pipeline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_pipeline: Option<String>,

    // ── Deprecated: moved to .agent/pipelines/*.toml ──────────────────────
    /// @deprecated Use stage definitions in `.agent/pipelines/*.toml` instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<String>,
    /// @deprecated Use `max_retries` in `.agent/pipelines/*.toml` stage definitions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checker_retries: Option<u32>,
    /// @deprecated Not supported in DAG pipeline yet.
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
    /// Provider name. Can be empty when `endpoint` is set (inherited from endpoint).
    #[serde(default)]
    pub provider: String,
    /// Model identifier (e.g. "gpt-4o", "deepseek-v4-flash").
    pub model: String,
    /// Reference to a named endpoint in `[endpoints]`. When set, provider / base_url
    /// / api_key are inherited from the endpoint, with model-level values taking
    /// precedence for per-field overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Enable extended thinking / reasoning mode (DeepSeek V4 / Claude 3.7+).
    /// OpenAI format: injects `{"thinking": {"type": "enabled"}}` into the request.
    /// Anthropic format: injects `{"thinking": {"type": "enabled", "budget_tokens": 8000}}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_enabled: Option<bool>,
    /// Reasoning effort level: "low" | "medium" | "high" | "max".
    /// OpenAI format: injects `{"reasoning_effort": "<value>"}` into the request.
    /// Anthropic format (DeepSeek endpoint): injects `{"output_config": {"effort": "<value>"}}` into the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Sampling temperature (0.0–2.0). Controls output randomness.
    /// 0.0 = deterministic, higher = more creative. Defaults to 0.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
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
    pub thinking_enabled: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f32>,
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
    /// Merges model-level fields with endpoint defaults when `endpoint` is set.
    pub fn resolve(&self, alias: &str) -> Option<ResolvedModel> {
        let entry = self.models.get(alias)?;
        let ep = entry.endpoint.as_deref().and_then(|n| self.endpoints.get(n));

        // provider: model > endpoint > fallback "compatible"
        let provider_str = if entry.provider.is_empty() {
            ep.map(|e| e.provider.as_str()).unwrap_or("compatible")
        } else {
            &entry.provider
        };

        // base_url: model > endpoint
        let base_url = entry
            .base_url
            .clone()
            .or_else(|| ep.map(|e| e.base_url.clone()));

        // api_key: model > endpoint
        let api_key = entry
            .api_key
            .clone()
            .or_else(|| ep.and_then(|e| e.api_key.clone()));

        Some(ResolvedModel {
            alias: alias.to_string(),
            provider: parse_provider(provider_str),
            model: entry.model.clone(),
            base_url,
            api_key,
            max_tokens: entry.max_tokens,
            thinking_enabled: entry.thinking_enabled,
            reasoning_effort: entry.reasoning_effort.clone(),
            temperature: entry.temperature,
        })
    }

    /// Return the resolved (effective) base_url for a model entry,
    /// merging with its endpoint if one is referenced.
    pub fn effective_base_url(&self, entry: &ModelEntry) -> Option<String> {
        if let Some(ref url) = entry.base_url {
            return Some(url.clone());
        }
        entry
            .endpoint
            .as_deref()
            .and_then(|n| self.endpoints.get(n))
            .map(|e| e.base_url.clone())
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

    /// Check whether an alias already exists.
    pub fn has_alias(&self, alias: &str) -> bool {
        self.models.contains_key(alias)
    }

    /// Find aliases that reference the same model at the same (resolved) base_url.
    pub fn find_duplicates(&self, model: &str, base_url: &str) -> Vec<String> {
        self.models
            .iter()
            .filter(|(_, e)| {
                e.model == model && self.effective_base_url(e).as_deref() == Some(base_url)
            })
            .map(|(a, _)| a.clone())
            .collect()
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

// ── Model fetching (OpenAI-compatible / Ollama) ─────────────────────

/// Result of fetching available models from a remote API.
#[derive(Debug, Clone)]
pub struct FetchedModels {
    pub models: Vec<String>,
    /// Which API format was detected: "openai-compatible" or "ollama".
    pub source: String,
}

/// Fetch the list of available models from an OpenAI-compatible or Ollama endpoint.
///
/// Tries `GET /v1/models` (OpenAI format) first; on failure falls back to
/// `GET /api/tags` (Ollama format).  Returns all discovered model IDs.
pub async fn fetch_models(base_url: &str, api_key: Option<&str>) -> Result<FetchedModels> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let base = base_url.trim_end_matches('/');

    // ── Attempt 1: OpenAI-compatible /v1/models ──────────────────────
    let mut req = client.get(format!("{}/v1/models", base));
    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let mut tried_endpoints: Vec<&str> = vec![];

    if let Ok(resp) = req.send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.text().await {
                match parse_openai_model_list(&body) {
                    Ok(models) if !models.is_empty() => {
                        return Ok(FetchedModels {
                            models,
                            source: "openai-compatible".into(),
                        });
                    }
                    Ok(_) => tried_endpoints.push("openai-compatible (empty list)"),
                    Err(_) => tried_endpoints.push("openai-compatible"),
                }
            }
        }
    }

    // ── Attempt 2: Ollama /api/tags ─────────────────────────────────
    if let Ok(resp) = client.get(format!("{}/api/tags", base)).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.text().await {
                match parse_ollama_model_list(&body) {
                    Ok(models) if !models.is_empty() => {
                        return Ok(FetchedModels {
                            models,
                            source: "ollama".into(),
                        });
                    }
                    Ok(_) => {
                        anyhow::bail!(
                            "Ollama endpoint at '{}' responded but has no models installed.\n\
                             Pull a model first, e.g.: ollama pull llama3.2",
                            base_url
                        );
                    }
                    Err(_) => tried_endpoints.push("ollama"),
                }
            }
        }
    }

    anyhow::bail!(
        "Unable to fetch model list from '{}'. Tried: {}.",
        base_url,
        if tried_endpoints.is_empty() { "(no successful response)".into() }
        else { tried_endpoints.join(", ") }
    )
}

fn parse_openai_model_list(body: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct OpenAIModelsResponse {
        data: Vec<OpenAIModelEntry>,
    }
    #[derive(Deserialize)]
    struct OpenAIModelEntry {
        id: String,
    }
    let resp: OpenAIModelsResponse = serde_json::from_str(body)?;
    Ok(resp.data.into_iter().map(|m| m.id).collect())
}

fn parse_ollama_model_list(body: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct OllamaModelsResponse {
        models: Vec<OllamaModelEntry>,
    }
    #[derive(Deserialize)]
    struct OllamaModelEntry {
        name: String,
    }
    let resp: OllamaModelsResponse = serde_json::from_str(body)?;
    Ok(resp.models.into_iter().map(|m| m.name).collect())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Sanitize a model name into a valid alias: lowercase, replace
/// non-alphanumeric chars with underscores, collapse runs.
pub fn sanitize_alias(model_name: &str) -> String {
    let mut out = String::with_capacity(model_name.len());
    let mut last_was_sep = false;
    for ch in model_name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn parse_provider(s: &str) -> Provider {
    match s.to_lowercase().as_str() {
        "anthropic" => Provider::Anthropic,
        "openai" => Provider::OpenAI,
        "compatible" => Provider::Compatible,
        _ => Provider::Compatible,
    }
}
