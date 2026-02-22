use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::model_manager;
use crate::Args;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub provider: Provider,
    pub base_url: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_conversation_turns: usize,
    pub max_tool_iterations: usize,
    /// The alias of the active model (if loaded from models.toml).
    pub model_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAI,
    Compatible,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Anthropic => write!(f, "anthropic"),
            Provider::OpenAI => write!(f, "openai"),
            Provider::Compatible => write!(f, "compatible"),
        }
    }
}

impl Config {
    /// Load configuration with priority:
    /// CLI args > models.toml default > environment variables > hard-coded defaults
    pub fn load(args: &Args) -> Result<Self> {
        // Check if the user explicitly passed --model / --provider on the CLI.
        // clap fills defaults from env or default_value, so we compare against
        // those to detect an explicit override.
        let cli_model_explicit = std::env::args().any(|a| a == "--model" || a == "-m");
        let cli_provider_explicit = std::env::args().any(|a| a == "--provider");

        // Try to load a model from models.toml (default entry) when the user
        // did NOT pass explicit CLI flags.
        let models_cfg = model_manager::load();
        let resolved = if !cli_model_explicit && !cli_provider_explicit {
            models_cfg.resolve_default()
        } else if cli_model_explicit {
            // If --model matches an alias in models.toml, resolve it
            models_cfg.resolve(&args.model).or(None)
        } else {
            None
        };

        let (provider, model, model_alias, toml_base_url, toml_api_key, toml_max_tokens) =
            if let Some(ref r) = resolved {
                (
                    r.provider.clone(),
                    r.model.clone(),
                    Some(r.alias.clone()),
                    r.base_url.clone(),
                    r.api_key.clone(),
                    r.max_tokens,
                )
            } else {
                let provider = match args.provider.to_lowercase().as_str() {
                    "anthropic" => Provider::Anthropic,
                    "openai" => Provider::OpenAI,
                    "compatible" => Provider::Compatible,
                    _ => Provider::Anthropic,
                };
                (provider, args.model.clone(), None, None, None, None)
            };

        let api_key_env = match provider {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Compatible => "LLM_API_KEY",
        };

        // api_key priority: models.toml entry > env var
        let api_key = toml_api_key.unwrap_or_else(|| {
            std::env::var(api_key_env).unwrap_or_else(|_| {
                eprintln!(
                    "⚠️  Warning: {} not set. Please set it or create a .env file.",
                    api_key_env
                );
                String::new()
            })
        });

        // base_url priority: models.toml entry > env var > provider default
        let base_url = toml_base_url.unwrap_or_else(|| {
            std::env::var("LLM_BASE_URL").unwrap_or_else(|_| match provider {
                Provider::Anthropic => "https://api.anthropic.com".to_string(),
                Provider::OpenAI => "https://api.openai.com".to_string(),
                Provider::Compatible => "http://localhost:8080".to_string(),
            })
        });

        let max_tokens = toml_max_tokens.unwrap_or(8192);

        Ok(Config {
            api_key,
            model,
            provider,
            base_url,
            max_tokens,
            temperature: 0.0,
            max_conversation_turns: 100,
            max_tool_iterations: args.max_iterations,
            model_alias,
        })
    }

    /// Build a new `Config` by applying a resolved model on top of the current config.
    pub fn with_resolved_model(&self, resolved: &model_manager::ResolvedModel) -> Config {
        let api_key_env = match resolved.provider {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Compatible => "LLM_API_KEY",
        };

        let api_key = resolved.api_key.clone().unwrap_or_else(|| {
            std::env::var(api_key_env).unwrap_or_else(|_| self.api_key.clone())
        });

        let base_url = resolved.base_url.clone().unwrap_or_else(|| {
            std::env::var("LLM_BASE_URL").unwrap_or_else(|_| match resolved.provider {
                Provider::Anthropic => "https://api.anthropic.com".to_string(),
                Provider::OpenAI => "https://api.openai.com".to_string(),
                Provider::Compatible => "http://localhost:8080".to_string(),
            })
        });

        Config {
            api_key,
            model: resolved.model.clone(),
            provider: resolved.provider.clone(),
            base_url,
            max_tokens: resolved.max_tokens.unwrap_or(self.max_tokens),
            temperature: self.temperature,
            max_conversation_turns: self.max_conversation_turns,
            max_tool_iterations: self.max_tool_iterations,
            model_alias: Some(resolved.alias.clone()),
        }
    }
}
