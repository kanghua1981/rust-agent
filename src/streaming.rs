//! Streaming output support for Anthropic SSE API.
//!
//! Parses Server-Sent Events from Anthropic's streaming API and yields
//! text deltas in real-time, while accumulating the full response.

use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::conversation::{ContentBlock, Conversation};
use crate::llm::{LlmResponse, Usage};
use crate::output::AgentOutput;
use crate::tools::ToolDefinition;

/// Normalize a provider base URL: trim trailing slashes and any trailing `/v1`
/// so the endpoint append below never produces a doubled `/v1/v1/...`.
/// Users often configure the endpoint with the version segment already
/// included (e.g. `https://api.openai.com/v1`), which would otherwise be
/// appended twice.
fn api_base(base: &str) -> String {
    let mut s = base.trim_end_matches('/');
    let lower = s.to_ascii_lowercase();
    if lower.ends_with("/v1") {
        s = &s[..s.len() - 3];
        s = s.trim_end_matches('/');
    }
    s.to_string()
}

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
    /// DeepSeek reasoner thinking tokens
    reasoning_content: Option<String>,
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
    signature: String,         // Anthropic thinking signature (must be echoed back)
}

/// Maximum time to wait for the next SSE chunk before declaring the stream dead.
/// Long thinking-model responses can take 60–90 s between tokens, so 120 s is
/// generous enough to not false-fire on a slow model while still catching real hangs.
const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);
/// TCP connect timeout for LLM API requests.
// api.deepseek.com can take ~15s+ to establish a TCP+TLS connection from some
// routes; give the connect phase generous headroom so a slow handshake isn't
// misreported as a hard failure.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Send a streaming request to Anthropic and print text tokens in real-time.
/// Returns the complete LlmResponse when done.
pub async fn stream_anthropic_response(
    config: &Config,
    conversation: &Conversation,
    tools: &[ToolDefinition],
    output: &dyn AgentOutput,
) -> Result<LlmResponse> {
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_default();

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

    let thinking_on = config.use_extended_thinking(conversation);
    let messages = if thinking_on {
        conversation.api_messages_with_thinking()
    } else {
        conversation.api_messages()
    };

    let mut request_body = serde_json::json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "system": conversation.system_prompt,
        "messages": messages,
        "stream": true,
    });

    if !formatted_tools.is_empty() {
        request_body["tools"] = serde_json::json!(formatted_tools);
    }

    // Extended thinking / reasoning effort (DeepSeek V4 / Claude 3.7+)
    if thinking_on {
        request_body["thinking"] = serde_json::json!({ "type": "enabled", "budget_tokens": 8000 });
    }
    if let Some(ref effort) = config.reasoning_effort {
        request_body["output_config"] = serde_json::json!({ "effort": effort });
    }

    tracing::debug!(
        "Anthropic request body: {}",
        serde_json::to_string_pretty(&request_body).unwrap_or_default()
    );

    let response = client
        .post(format!("{}/v1/messages", api_base(&config.base_url)))
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

    while let Some(chunk) = {
        match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(item) => item,
            Err(_) => {
                anyhow::bail!(
                    "LLM stream timed out: no data received for {}s. \
                     The API server may be overloaded or the connection stalled.",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                );
            }
        }
    } {
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
                            signature: String::new(),
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
                    } else if content_block.block_type == "thinking" {
                        output.on_thinking_start();
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
                            DeltaData::ThinkingDelta { thinking } => {
                                // Stream thinking tokens to output and accumulate for echo-back.
                                output.on_thinking_token(&thinking);
                                blocks[index].text.push_str(&thinking);
                            }
                            DeltaData::SignatureDelta { signature } => {
                                // Accumulate the Anthropic thinking signature — it MUST
                                // be echoed back verbatim in the next request turn.
                                blocks[index].signature.push_str(&signature);
                            }
                        }
                    }
                }
                StreamEvent::ContentBlockStop { index } => {
                    if index < blocks.len() && blocks[index].block_type == "thinking" {
                        output.on_thinking_end();
                    }
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
            "thinking" => {
                if block.text.is_empty() {
                    None
                } else {
                    Some(ContentBlock::Thinking {
                        thinking: block.text,
                        signature: if block.signature.is_empty() { None } else { Some(block.signature) },
                    })
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
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_default();

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
                let reasoning = msg.content.iter().find_map(|b| {
                    if let ContentBlock::Thinking { thinking, .. } = b {
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

    // Detect if extended thinking should be active — mirrors the Anthropic path logic.
    let thinking_on = config.use_extended_thinking(conversation);

    let mut request_body = serde_json::json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "messages": messages,
        "stream": true,
        // dsh sends this too; without it OpenAI-compatible servers won't return
        // a usage chunk, so token accounting stays at 0.
        "stream_options": { "include_usage": true },
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

    // Extended thinking / reasoning effort for DeepSeek reasoner & OpenAI reasoner modes.
    if thinking_on {
        request_body["thinking"] = serde_json::json!({ "type": "enabled" });
    }
    if let Some(ref effort) = config.reasoning_effort {
        request_body["reasoning_effort"] = serde_json::json!(effort);
    }

    let response = client
        .post(format!("{}/v1/chat/completions", api_base(&config.base_url)))
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
    let mut accumulated_reasoning = String::new();
    let mut tool_accumulators: Vec<BlockAccumulator> = Vec::new();
    let mut stop_reason: Option<String> = None;
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut is_printing_text = false;
    let mut is_printing_thinking = false;

    while let Some(chunk) = {
        match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(item) => item,
            Err(_) => {
                anyhow::bail!(
                    "LLM stream timed out: no data received for {}s. \
                     The API server may be overloaded or the connection stalled.",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                );
            }
        }
    } {
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
                    // End thinking block if still open when finish_reason arrives.
                    if is_printing_thinking {
                        output.on_thinking_end();
                        is_printing_thinking = false;
                    }
                    stop_reason = Some(reason);
                }

                if let Some(reasoning) = choice.delta.reasoning_content {
                    if !reasoning.is_empty() {
                        if !is_printing_thinking {
                            output.on_thinking_start();
                            is_printing_thinking = true;
                        }
                        output.on_thinking_token(&reasoning);
                        accumulated_reasoning.push_str(&reasoning);
                    }
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
                                signature: String::new(),
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

    // End thinking block if stream loop ended without finish_reason closing it.
    if is_printing_thinking {
        output.on_thinking_end();
    }
    if is_printing_text {
        output.on_stream_end();
    }

    let mut final_content = Vec::new();
    // Store reasoning tokens first so they appear before the answer in history.
    // The next request will echo them back as `reasoning_content`.
    if !accumulated_reasoning.is_empty() {
        final_content.push(ContentBlock::Thinking { thinking: accumulated_reasoning, signature: None });
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Provider;
    use crate::conversation::Message;
    use crate::memory::MemoryConfig;
    use crate::output::SilentOutput;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn test_config(
        base_url: String,
        provider: Provider,
        thinking: Option<bool>,
        effort: Option<String>,
    ) -> Config {
        Config {
            api_key: "test-key".to_string(),
            model: "test-model".to_string(),
            provider,
            base_url,
            max_tokens: 128,
            temperature: 0.0,
            max_conversation_turns: 100,
            max_tool_iterations: 5,
            model_alias: None,
            sub_agents: BTreeMap::new(),
            extra_binds: Vec::new(),
            memory: MemoryConfig::default(),
            thinking_enabled: thinking,
            reasoning_effort: effort,
        }
    }

    fn test_conversation() -> Conversation {
        let mut c = Conversation::with_system_prompt("you are a test".to_string());
        c.add_message(Message::user("hi"));
        c
    }

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// One-shot HTTP/1.1 server that answers with `body` as an SSE stream and
    /// returns the raw request it received (for request-format assertions).
    async fn spawn_mock(body: &'static str) -> (String, Arc<tokio::sync::Mutex<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(tokio::sync::Mutex::new(String::new()));
        let cap = captured.clone();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let n = match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = find(&buf, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let cl: usize = head
                            .split("content-length:")
                            .nth(1)
                            .and_then(|s| s.split("\r\n").next())
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + cl {
                            break;
                        }
                    }
                }
                *cap.lock().await = String::from_utf8_lossy(&buf).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.flush().await;
            }
        });
        (format!("http://{}", addr), captured)
    }

    const ANTHROPIC_SSE: &str = r#"event: message_start
data: {"type":"message_start","message":{"usage":{"input_tokens":12}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"pondering"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-abc"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"read_file"}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"a.txt\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":2}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}

event: message_stop
data: {"type":"message_stop"}

"#;

    const OPENAI_SSE: &str = r#"data: {"choices":[{"delta":{"reasoning_content":"think"},"finish_reason":null}]}

data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}

data: {"choices":[{"delta":{"content":" there"},"finish_reason":null}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"path\":"}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"b.txt\"}"}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":9,"completion_tokens":5}}

data: [DONE]

"#;

    #[test]
    fn api_base_strips_trailing_v1_and_slashes() {
        assert_eq!(api_base("https://api.openai.com"), "https://api.openai.com");
        assert_eq!(api_base("https://api.openai.com/v1"), "https://api.openai.com");
        assert_eq!(api_base("https://api.openai.com/v1/"), "https://api.openai.com");
        assert_eq!(api_base("http://localhost:8080/"), "http://localhost:8080");
        assert_eq!(
            api_base("https://api.deepseek.com/anthropic"),
            "https://api.deepseek.com/anthropic"
        );
    }

    #[tokio::test]
    async fn anthropic_stream_parses_thinking_text_tool_and_usage() {
        let (base, captured) = spawn_mock(ANTHROPIC_SSE).await;
        let cfg = test_config(base, Provider::Anthropic, Some(true), None);
        let conv = test_conversation();

        let resp = stream_anthropic_response(&cfg, &conv, &[], &SilentOutput)
            .await
            .expect("stream should parse");

        let thinking = resp.content.iter().find_map(|b| match b {
            ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
            _ => None,
        });
        assert_eq!(thinking.as_deref(), Some("pondering"));

        let text = resp.content.iter().find_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(text.as_deref(), Some("Hello world"));

        let (name, input) = resp
            .content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolUse { name, input, .. } => Some((name.clone(), input.clone())),
                _ => None,
            })
            .expect("tool_use block");
        assert_eq!(name, "read_file");
        assert_eq!(input["path"], "a.txt");

        assert_eq!(resp.stop_reason.as_deref(), Some("tool_use"));
        let usage = resp.usage.expect("usage");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 7);

        let req = captured.lock().await.clone();
        assert!(
            req.starts_with("POST /v1/messages "),
            "request line: {:?}",
            req.lines().next()
        );
        let lower = req.to_lowercase();
        assert!(lower.contains("x-api-key: test-key"));
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        assert!(req.contains("\"model\":\"test-model\""));
        assert!(req.contains("\"stream\":true"));
        assert!(req.contains("\"thinking\""));
    }

    #[tokio::test]
    async fn openai_stream_parses_reasoning_text_tool_and_usage() {
        let (base, captured) = spawn_mock(OPENAI_SSE).await;
        let cfg = test_config(base, Provider::Compatible, None, None);
        let conv = test_conversation();

        let resp = stream_openai_response(&cfg, &conv, &[], &SilentOutput)
            .await
            .expect("stream should parse");

        let thinking = resp.content.iter().find_map(|b| match b {
            ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
            _ => None,
        });
        assert_eq!(thinking.as_deref(), Some("think"));

        let text = resp.content.iter().find_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(text.as_deref(), Some("Hi there"));

        let (name, input) = resp
            .content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolUse { name, input, .. } => Some((name.clone(), input.clone())),
                _ => None,
            })
            .expect("tool_use block");
        assert_eq!(name, "read_file");
        assert_eq!(input["path"], "b.txt");

        assert_eq!(resp.stop_reason.as_deref(), Some("tool_calls"));
        let usage = resp.usage.expect("usage");
        assert_eq!(usage.input_tokens, 9);
        assert_eq!(usage.output_tokens, 5);

        let req = captured.lock().await.clone();
        assert!(req.starts_with("POST /v1/chat/completions "));
        let lower = req.to_lowercase();
        assert!(lower.contains("authorization: bearer test-key"));
        assert!(req.contains("\"stream_options\":{\"include_usage\":true}"));
    }

    #[tokio::test]
    async fn base_url_ending_in_v1_is_not_doubled() {
        let (base, captured) = spawn_mock(OPENAI_SSE).await;
        let cfg = test_config(format!("{base}/v1"), Provider::Compatible, None, None);
        let conv = test_conversation();

        let _ = stream_openai_response(&cfg, &conv, &[], &SilentOutput)
            .await
            .expect("stream should parse");

        let req = captured.lock().await.clone();
        assert_eq!(
            req.lines().next().unwrap(),
            "POST /v1/chat/completions HTTP/1.1"
        );
    }
}
