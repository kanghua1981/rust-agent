use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::conversation::{ContentBlock, Conversation};
use crate::tools::ToolDefinition;

use super::{LlmClient, LlmResponse, Usage};

pub struct AnthropicClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    temperature: f32,
    system: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
}

impl AnthropicClient {
    pub fn new(config: &Config) -> Self {
        AnthropicClient {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
        }
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect()
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn send_message(
        &self,
        conversation: &Conversation,
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            system: conversation.system_prompt.clone(),
            messages: conversation.api_messages(),
            tools: self.format_tools(tools),
        };

        tracing::debug!("Sending request to Anthropic API");

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            if let Ok(error) = serde_json::from_str::<AnthropicError>(&body) {
                anyhow::bail!(
                    "Anthropic API error ({}): {} - {}",
                    status,
                    error.error.error_type,
                    error.error.message
                );
            }
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        let api_response: AnthropicResponse =
            serde_json::from_str(&body).context("Failed to parse Anthropic response")?;

        let content = api_response
            .content
            .into_iter()
            .map(|block| match block {
                AnthropicContentBlock::Text { text } => ContentBlock::Text { text },
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    ContentBlock::ToolUse { id, name, input }
                }
            })
            .collect();

        let usage = api_response.usage.map(|u| Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        });

        Ok(LlmResponse {
            content,
            stop_reason: api_response.stop_reason,
            usage,
        })
    }
}
