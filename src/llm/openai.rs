use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::conversation::{ContentBlock, Conversation};
use crate::tools::ToolDefinition;

use super::{LlmClient, LlmResponse, Usage};

pub struct OpenAIClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    max_tokens: u32,
    temperature: f32,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    /// DeepSeek reasoner thinking tokens (only present for reasoning models)
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    id: String,
    function: OpenAIFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAIError {
    error: OpenAIErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenAIErrorDetail {
    message: String,
}

impl OpenAIClient {
    pub fn new(config: &Config) -> Self {
        OpenAIClient {
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

    fn format_messages(&self, conversation: &Conversation) -> Vec<serde_json::Value> {
        let mut messages = vec![serde_json::json!({
            "role": "system",
            "content": conversation.system_prompt,
        })];

        for msg in &conversation.messages {
            match msg.role {
                crate::conversation::Role::User => {
                    // Check if message contains images
                    if msg.has_images() {
                        // For messages with images, we need to build a content array
                        let mut content = Vec::new();
                        
                        for block in &msg.content {
                            match block {
                                ContentBlock::Text { text } => {
                                    content.push(serde_json::json!({
                                        "type": "text",
                                        "text": text,
                                    }));
                                }
                                ContentBlock::Image { source, mime_type: _ } => {
                                    match source {
                                        crate::conversation::ImageSource::Base64 { media_type, data } => {
                                            content.push(serde_json::json!({
                                                "type": "image_url",
                                                "image_url": {
                                                    "url": format!("data:{};base64,{}", media_type, data),
                                                }
                                            }));
                                        }
                                    }
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content: tool_content,
                                    ..
                                } => {
                                    messages.push(serde_json::json!({
                                        "role": "tool",
                                        "tool_call_id": tool_use_id,
                                        "content": tool_content,
                                    }));
                                }
                                _ => {}
                            }
                        }
                        
                        if !content.is_empty() {
                            messages.push(serde_json::json!({
                                "role": "user",
                                "content": content,
                            }));
                        }
                    } else {
                        // For text-only messages, handle as before
                        for block in &msg.content {
                            match block {
                                ContentBlock::Text { text } => {
                                    messages.push(serde_json::json!({
                                        "role": "user",
                                        "content": text,
                                    }));
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    ..
                                } => {
                                    messages.push(serde_json::json!({
                                        "role": "tool",
                                        "tool_call_id": tool_use_id,
                                        "content": content,
                                    }));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                crate::conversation::Role::Assistant => {
                    let text = msg.text_content();
                    let reasoning = msg.content.iter().find_map(|b| {
                        if let ContentBlock::Thinking { thinking } = b {
                            Some(thinking.clone())
                        } else {
                            None
                        }
                    });
                    let tool_calls: Vec<serde_json::Value> = msg
                        .content
                        .iter()
                        .filter_map(|block| {
                            if let ContentBlock::ToolUse { id, name, input } = block {
                                Some(serde_json::json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                }))
                            } else {
                                None
                            }
                        })
                        .collect();

                    let mut msg_json = serde_json::json!({
                        "role": "assistant",
                    });

                    if let Some(r) = reasoning {
                        msg_json["reasoning_content"] = serde_json::json!(r);
                    }
                    if !text.is_empty() {
                        msg_json["content"] = serde_json::json!(text);
                    }
                    if !tool_calls.is_empty() {
                        msg_json["tool_calls"] = serde_json::json!(tool_calls);
                    }

                    messages.push(msg_json);
                }
                _ => {}
            }
        }

        messages
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect()
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn send_message(
        &self,
        conversation: &Conversation,
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let request = OpenAIRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            messages: self.format_messages(conversation),
            tools: self.format_tools(tools),
        };

        tracing::debug!("Sending request to OpenAI-compatible API");

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            if let Ok(error) = serde_json::from_str::<OpenAIError>(&body) {
                anyhow::bail!("OpenAI API error ({}): {}", status, error.error.message);
            }
            anyhow::bail!("OpenAI API error ({}): {}", status, body);
        }

        let api_response: OpenAIResponse =
            serde_json::from_str(&body).context("Failed to parse OpenAI response")?;

        let choice = api_response
            .choices
            .into_iter()
            .next()
            .context("No choices in OpenAI response")?;

        let mut content = Vec::new();

        // DeepSeek reasoner models return thinking tokens separately.
        // Store them first so they're echoed back in the next request.
        if let Some(reasoning) = choice.message.reasoning_content {
            if !reasoning.is_empty() {
                content.push(ContentBlock::Thinking { thinking: reasoning });
            }
        }

        if let Some(text) = choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlock::Text { text });
            }
        }

        if let Some(tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                content.push(ContentBlock::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        let usage = api_response.usage.map(|u| Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        });

        Ok(LlmResponse {
            content,
            stop_reason: choice.finish_reason,
            usage,
        })
    }
}
