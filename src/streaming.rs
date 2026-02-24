//! Streaming output support for Anthropic SSE API.
//!
//! Parses Server-Sent Events from Anthropic's streaming API and yields
//! text deltas in real-time, while accumulating the full response.

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::conversation::{ContentBlock, Conversation};
use crate::llm::{LlmResponse, Usage};
use crate::output::AgentOutput;
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
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
    usage: Option<OpenAIStreamUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCall {
    index: usize,
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
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
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
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
    output: &dyn AgentOutput,
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
        // Anthropic's streaming endpoint returns errors in SSE format even for
        // pre-stream errors, e.g.:
        //   event:error
        //   data:{"code":"InvalidParameter","message":"...",...}
        // Extract the JSON from the data: line so the user sees a clean message.
        let message = body
            .lines()
            .find(|l| l.starts_with("data:"))
            .and_then(|l| serde_json::from_str::<serde_json::Value>(l.trim_start_matches("data:")).ok())
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| body.clone());
        anyhow::bail!("Anthropic API error ({}): {}", status, message);
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
        // Check for Ctrl-C interrupt; break out of the stream early so
        // the caller can stop cleanly rather than waiting for the full response.
        if crate::agent::is_interrupted() {
            break;
        }
        let chunk = chunk.context("Error reading stream chunk")?;
        // Normalize \r\n to \n so SSE parsing works with all servers
        let chunk_str = String::from_utf8_lossy(&chunk).replace("\r\n", "\n").replace('\r', "\n");
        buffer.push_str(&chunk_str);

        // Process complete SSE lines
        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            // Parse event type and data
            let mut _event_type = String::new();
            let mut event_data = String::new();

            for line in event_text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("event:") {
                    _event_type = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    event_data = rest.trim().to_string();
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
                            output.on_stream_start();
                            is_printing_text = true;
                        }
                        if let Some(text) = content_block.text {
                            output.on_streaming_text(&text);
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
                                    output.on_stream_start();
                                    is_printing_text = true;
                                }
                                // Real-time streaming output
                                output.on_streaming_text(&text);
                                blocks[index].text.push_str(&text);
                            }
                            DeltaData::InputJsonDelta { partial_json } => {
                                blocks[index]
                                    .tool_input_json
                                    .push_str(&partial_json);
                            }
                            DeltaData::ThinkingDelta { .. } | DeltaData::SignatureDelta { .. } => {
                                // Silently skip thinking/signature deltas (extended thinking feature)
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
                        output.on_stream_end();
                        is_printing_text = false;
                    }
                }
                StreamEvent::Ping => {}
                StreamEvent::Error { error } => {
                    if is_printing_text {
                        output.on_stream_end();
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
        output.on_stream_end();
    }

    // Build the final content blocks
    let mut content: Vec<ContentBlock> = blocks
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
                // unwrap_or_default() would produce Value::Null on parse failure.
                // Anthropic requires tool_use.input to be a JSON *object*;
                // sending null causes 400 "Request body format invalid".
                // Fall back to an empty object so the tool can return a proper
                // "missing parameter" error on the next iteration instead.
                let input: serde_json::Value =
                    serde_json::from_str(&block.tool_input_json)
                        .unwrap_or_else(|_| serde_json::json!({}));
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

    if content.is_empty() {
        // This can happen if the model only returned thinking blocks (extended thinking mode)
        // Return a placeholder response rather than an error
        content.push(ContentBlock::Text { text: String::new() });
    }

    Ok(LlmResponse {
        content,
        stop_reason,
        usage,
    })
}

/// Send a streaming request to an OpenAI-compatible API and print text tokens in real-time.
pub async fn stream_openai_response(
    config: &Config,
    conversation: &Conversation,
    tools: &[ToolDefinition],
    output: &dyn AgentOutput,
) -> Result<LlmResponse> {
    let client = Client::new();

    // Use OpenAI message formatting logic (already in openai.rs, but we'll inline it for streaming)
    // In a real refactor, we should move these formatters to a shared location.
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": conversation.system_prompt,
    })];

    for msg in &conversation.messages {
        match msg.role {
            crate::conversation::Role::User => {
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
            crate::conversation::Role::Assistant => {
                let text = msg.text_content();
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

    let mut request_body = serde_json::json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "messages": messages,
        "stream": true,
    });

    if !tools.is_empty() {
        request_body["tools"] = serde_json::json!(tools
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
            .collect::<Vec<serde_json::Value>>());
    }

    let response = client
        .post(format!("{}/v1/chat/completions", config.base_url))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send streaming request to OpenAI API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await?;
        anyhow::bail!("OpenAI API error ({}): {}", status, body);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut accumulated_text = String::new();
    let mut tool_accumulators: Vec<BlockAccumulator> = Vec::new();
    let mut stop_reason: Option<String> = None;
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut is_printing_text = false;

    while let Some(chunk) = stream.next().await {
        // Check for Ctrl-C interrupt; break out of the stream early.
        if crate::agent::is_interrupted() {
            break;
        }
        let chunk = chunk.context("Error reading stream chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let data = line[6..].trim();
            if data == "[DONE]" {
                break;
            }

            let chunk_response: OpenAIStreamResponse = match serde_json::from_str(data) {
                Ok(r) => r,
                Err(_) => continue, // Ignore parsing errors for individual chunks
            };

            if let Some(usage) = chunk_response.usage {
                input_tokens = usage.prompt_tokens;
                output_tokens = usage.completion_tokens;
            }

            for choice in chunk_response.choices {
                if let Some(reason) = choice.finish_reason {
                    stop_reason = Some(reason);
                }

                if let Some(content) = choice.delta.content {
                    if !content.is_empty() {
                        if !is_printing_text {
                            output.on_stream_start();
                            is_printing_text = true;
                        }
                        output.on_streaming_text(&content);
                        accumulated_text.push_str(&content);
                    }
                }

                if let Some(tool_calls) = choice.delta.tool_calls {
                    for tc in tool_calls {
                        let idx = tc.index;
                        while tool_accumulators.len() <= idx {
                            tool_accumulators.push(BlockAccumulator {
                                block_type: "tool_use".to_string(),
                                text: String::new(),
                                tool_id: String::new(),
                                tool_name: String::new(),
                                tool_input_json: String::new(),
                            });
                        }

                        if let Some(id) = tc.id {
                            tool_accumulators[idx].tool_id = id;
                        }
                        if let Some(func) = tc.function {
                            if let Some(name) = func.name {
                                tool_accumulators[idx].tool_name = name;
                            }
                            if let Some(args) = func.arguments {
                                tool_accumulators[idx].tool_input_json.push_str(&args);
                            }
                        }
                    }
                }
            }
        }
    }

    if is_printing_text {
        output.on_stream_end();
    }

    let mut final_content = Vec::new();
    if !accumulated_text.is_empty() {
        final_content.push(ContentBlock::Text { text: accumulated_text });
    }

    for acc in tool_accumulators {
        if !acc.tool_name.is_empty() {
            let input: serde_json::Value =
                serde_json::from_str(&acc.tool_input_json).unwrap_or_default();
            final_content.push(ContentBlock::ToolUse {
                id: acc.tool_id,
                name: acc.tool_name,
                input,
            });
        }
    }

    if final_content.is_empty() {
        anyhow::bail!("OpenAI-compatible LLM returned an empty response. Check if the model is valid and the API endpoint supports streaming.");
    }

    Ok(LlmResponse {
        content: final_content,
        stop_reason,
        usage: Some(Usage { input_tokens, output_tokens }),
    })
}
