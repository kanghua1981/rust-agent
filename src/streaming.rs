//! Streaming output support for Anthropic SSE API.
//!
//! Parses Server-Sent Events from Anthropic's streaming API and yields
//! text deltas in real-time, while accumulating the full response.

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use std::io::Write;

use crate::config::Config;
use crate::conversation::{ContentBlock, Conversation};
use crate::llm::{LlmResponse, Usage};
use crate::tools::ToolDefinition;

/// SSE event types from Anthropic streaming API
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartData },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockData,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: DeltaData },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { #[allow(dead_code)] index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDeltaData, usage: Option<DeltaUsage> },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: StreamErrorData },
}

#[derive(Debug, Deserialize)]
struct MessageStartData {
    usage: Option<StartUsage>,
}

#[derive(Debug, Deserialize)]
struct StartUsage {
    input_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct DeltaUsage {
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ContentBlockData {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum DeltaData {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct MessageDeltaData {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamErrorData {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
}

/// Track what we're accumulating for each content block
struct BlockAccumulator {
    block_type: String,        // "text" or "tool_use"
    text: String,              // accumulated text for text blocks
    tool_id: String,           // tool use id
    tool_name: String,         // tool name
    tool_input_json: String,   // accumulated JSON string for tool input
}

/// Send a streaming request to Anthropic and print text tokens in real-time.
/// Returns the complete LlmResponse when done.
pub async fn stream_anthropic_response(
    config: &Config,
    conversation: &Conversation,
    tools: &[ToolDefinition],
) -> Result<LlmResponse> {
    let client = Client::new();

    let formatted_tools: Vec<serde_json::Value> = tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            })
        })
        .collect();

    let mut request_body = serde_json::json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "system": conversation.system_prompt,
        "messages": conversation.api_messages(),
        "stream": true,
    });

    if !formatted_tools.is_empty() {
        request_body["tools"] = serde_json::json!(formatted_tools);
    }

    let response = client
        .post(format!("{}/v1/messages", config.base_url))
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send streaming request to Anthropic API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await?;
        anyhow::bail!("Anthropic API error ({}): {}", status, body);
    }

    // Parse SSE stream
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut blocks: Vec<BlockAccumulator> = Vec::new();
    let mut stop_reason: Option<String> = None;
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut is_printing_text = false;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading stream chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines
        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            // Parse event type and data
            let mut _event_type = String::new();
            let mut event_data = String::new();

            for line in event_text.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    _event_type = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    event_data = rest.to_string();
                }
            }

            if event_data.is_empty() {
                continue;
            }

            // Parse the JSON data
            let event: StreamEvent = match serde_json::from_str(&event_data) {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!("Failed to parse SSE event: {} (data: {})", e, event_data);
                    continue;
                }
            };

            match event {
                StreamEvent::MessageStart { message } => {
                    if let Some(usage) = message.usage {
                        input_tokens = usage.input_tokens;
                    }
                }
                StreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    // Ensure we have space
                    while blocks.len() <= index {
                        blocks.push(BlockAccumulator {
                            block_type: String::new(),
                            text: String::new(),
                            tool_id: String::new(),
                            tool_name: String::new(),
                            tool_input_json: String::new(),
                        });
                    }

                    blocks[index].block_type = content_block.block_type.clone();

                    if content_block.block_type == "text" {
                        if !is_printing_text {
                            // Start the response separator
                            println!(
                                "\n{}",
                                "─".repeat(60)
                            );
                            is_printing_text = true;
                        }
                        if let Some(text) = content_block.text {
                            print!("{}", text);
                            std::io::stdout().flush().ok();
                            blocks[index].text.push_str(&text);
                        }
                    } else if content_block.block_type == "tool_use" {
                        blocks[index].tool_id =
                            content_block.id.unwrap_or_default();
                        blocks[index].tool_name =
                            content_block.name.unwrap_or_default();
                    }
                }
                StreamEvent::ContentBlockDelta { index, delta } => {
                    if index < blocks.len() {
                        match delta {
                            DeltaData::TextDelta { text } => {
                                if !is_printing_text {
                                    println!(
                                        "\n{}",
                                        "─".repeat(60)
                                    );
                                    is_printing_text = true;
                                }
                                // Real-time streaming print
                                print!("{}", text);
                                std::io::stdout().flush().ok();
                                blocks[index].text.push_str(&text);
                            }
                            DeltaData::InputJsonDelta { partial_json } => {
                                blocks[index]
                                    .tool_input_json
                                    .push_str(&partial_json);
                            }
                        }
                    }
                }
                StreamEvent::ContentBlockStop { index: _ } => {
                    // Block finished
                }
                StreamEvent::MessageDelta { delta, usage } => {
                    if let Some(reason) = delta.stop_reason {
                        stop_reason = Some(reason);
                    }
                    if let Some(u) = usage {
                        output_tokens = u.output_tokens;
                    }
                }
                StreamEvent::MessageStop => {
                    // Close the text output separator if we were printing
                    if is_printing_text {
                        println!("\n{}", "─".repeat(60));
                        is_printing_text = false;
                    }
                }
                StreamEvent::Ping => {}
                StreamEvent::Error { error } => {
                    if is_printing_text {
                        println!();
                        #[allow(unused_assignments)]
                        { is_printing_text = false; }
                    }
                    anyhow::bail!(
                        "Anthropic streaming error ({}): {}",
                        error.error_type,
                        error.message
                    );
                }
            }
        }
    }

    // Close text output if stream ended without MessageStop
    if is_printing_text {
        println!("\n{}", "─".repeat(60));
    }

    // Build the final content blocks
    let content: Vec<ContentBlock> = blocks
        .into_iter()
        .filter_map(|block| match block.block_type.as_str() {
            "text" => {
                if block.text.is_empty() {
                    None
                } else {
                    Some(ContentBlock::Text { text: block.text })
                }
            }
            "tool_use" => {
                let input: serde_json::Value =
                    serde_json::from_str(&block.tool_input_json).unwrap_or_default();
                Some(ContentBlock::ToolUse {
                    id: block.tool_id,
                    name: block.tool_name,
                    input,
                })
            }
            _ => None,
        })
        .collect();

    let usage = Some(Usage {
        input_tokens,
        output_tokens,
    });

    Ok(LlmResponse {
        content,
        stop_reason,
        usage,
    })
}
