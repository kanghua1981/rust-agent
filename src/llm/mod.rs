pub mod anthropic;
pub mod openai;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::{Config, Provider};
use crate::conversation::Conversation;
use crate::tools::ToolDefinition;

/// Response from the LLM
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Vec<crate::conversation::ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Trait for LLM clients
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn send_message(
        &self,
        conversation: &Conversation,
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse>;
}

/// Create an LLM client based on config
pub fn create_client(config: &Config) -> Box<dyn LlmClient> {
    match config.provider {
        Provider::Anthropic => Box::new(anthropic::AnthropicClient::new(config)),
        Provider::OpenAI | Provider::Compatible => Box::new(openai::OpenAIClient::new(config)),
    }
}
