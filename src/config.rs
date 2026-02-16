use anyhow::Result;
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAI,
    Compatible,
}

impl Config {
    pub fn load(args: &Args) -> Result<Self> {
        let provider = match args.provider.to_lowercase().as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "compatible" => Provider::Compatible,
            _ => Provider::Anthropic,
        };

        let api_key_env = match provider {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Compatible => "LLM_API_KEY",
        };

        let api_key = std::env::var(api_key_env).unwrap_or_else(|_| {
            eprintln!(
                "⚠️  Warning: {} not set. Please set it or create a .env file.",
                api_key_env
            );
            String::new()
        });

        let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| match provider {
            Provider::Anthropic => "https://api.anthropic.com".to_string(),
            Provider::OpenAI => "https://api.openai.com".to_string(),
            Provider::Compatible => "http://localhost:8080".to_string(),
        });

        Ok(Config {
            api_key,
            model: args.model.clone(),
            provider,
            base_url,
            max_tokens: 8192,
            temperature: 0.0,
            max_conversation_turns: 100,
            max_tool_iterations: 25,
        })
    }
}
