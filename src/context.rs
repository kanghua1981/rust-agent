//! Context window management.
//!
//! Monitors the estimated token usage of the conversation and
//! automatically truncates or summarizes when approaching the
//! model's context window limit.

use crate::conversation::{ContentBlock, Conversation, ImageSource, Message};

// ── Precise token counting (optional tiktoken-rs support) ─────────────────

/// Token counter that uses `tiktoken-rs` when the feature is enabled,
/// falling back to a tuned heuristic for each model family.
pub struct TokenCounter {
    /// Whether tiktoken is available (set at startup)
    tiktoken_available: bool,
}

impl TokenCounter {
    /// Create a new token counter. Automatically detects tiktoken availability.
    pub fn new() -> Self {
        Self {
            tiktoken_available: cfg!(feature = "tiktoken"),
        }
    }

    /// Count tokens in `text` using the most appropriate method for `model`.
    pub fn count(&self, text: &str, model: &str) -> usize {
        if self.tiktoken_available {
            match Self::count_tiktoken(text, model) {
                Ok(n) => return n,
                Err(_) => { /* fall through to heuristic */ }
            }
        }
        Self::count_heuristic(text, model)
    }

    #[cfg(feature = "tiktoken")]
    fn count_tiktoken(text: &str, _model: &str) -> Result<usize, String> {
        use tiktoken_rs::cl100k_base;
        // cl100k_base is used by GPT-4 and is close enough for Claude too
        let bpe = cl100k_base().map_err(|e| format!("tiktoken init: {}", e))?;
        let tokens = bpe.encode_with_special_tokens(text);
        Ok(tokens.len())
    }

    #[cfg(not(feature = "tiktoken"))]
    fn count_tiktoken(_text: &str, _model: &str) -> Result<usize, String> {
        Err("tiktoken not compiled in".to_string())
    }

    /// Heuristic token count with per-model-family tuning.
    ///
    /// These ratios are empirically derived and more accurate than the old
    /// fixed CJK=1.5/ASCII=0.25 heuristic.
    fn count_heuristic(text: &str, model: &str) -> usize {
        let model_lower = model.to_lowercase();

        // Per-model-family character-per-token ratios
        let (cjk_ratio, ascii_ratio): (f64, f64) = if model_lower.contains("claude") {
            // Claude 3/4 tokenizer is ~3.5 chars/token for English, ~1.2 for CJK
            (1.2, 3.5)
        } else if model_lower.contains("gpt-4") {
            (1.1, 3.8)
        } else if model_lower.contains("gpt-3.5") {
            (1.0, 3.5)
        } else if model_lower.contains("deepseek") {
            // DeepSeek uses a similar tokenizer to OpenAI
            (1.2, 3.5)
        } else {
            // Conservative defaults
            (1.5, 3.2)
        };

        let cjk_count = text.chars().filter(|c| is_cjk(*c)).count();
        let ascii_count = text.len().saturating_sub(cjk_count);

        let cjk_tokens = (cjk_count as f64 / cjk_ratio).ceil() as usize;
        let ascii_tokens = (ascii_count as f64 / ascii_ratio).ceil() as usize;

        (cjk_tokens + ascii_tokens).max(1)
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a character is CJK (Chinese/Japanese/Korean).
fn is_cjk(c: char) -> bool {
    let c = c as u32;
    (0x4E00..=0x9FFF).contains(&c)    // CJK Unified Ideographs
        || (0x3000..=0x303F).contains(&c)  // CJK Symbols & Punctuation
        || (0x3040..=0x30FF).contains(&c)  // Hiragana & Katakana
        || (0xAC00..=0xD7AF).contains(&c)  // Hangul Syllables
        || (0xFF00..=0xFFEF).contains(&c)  // Fullwidth forms
        || (0x3400..=0x4DBF).contains(&c)  // CJK Extension A
        || (0x20000..=0x2A6DF).contains(&c) // CJK Extension B
        || (0xF900..=0xFAFF).contains(&c)  // CJK Compatibility
}

/// Estimated max context tokens for different models
pub fn max_context_tokens(model: &str) -> usize {
    let model_lower = model.to_lowercase();
    if model_lower.contains("claude") {
        200_000
    } else if model_lower.contains("gpt-4o") {
        128_000
    } else if model_lower.contains("gpt-4") {
        128_000
    } else if model_lower.contains("gpt-3.5") {
        16_000
    } else if model_lower.contains("deepseek") {
        // DeepSeek models support up to 1M tokens
        1_000_000
    } else {
        // Conservative default
        100_000
    }
}

/// Lazy-initialized global token counter.
static TOKEN_COUNTER: once_cell::sync::Lazy<TokenCounter> =
    once_cell::sync::Lazy::new(TokenCounter::new);

/// Rough estimate of tokens in a string using the improved heuristic.
/// For precise counting, the `TokenCounter` with tiktoken-rs is preferred.
/// Kept for backward compatibility — prefers using model-aware `estimate_tokens_for_model`.
pub fn estimate_tokens(text: &str) -> usize {
    TokenCounter::count_heuristic(text, "default")
}

/// Estimate tokens in `text` using the best available counter for `model`.
/// Uses tiktoken-rs when the feature is enabled, otherwise a per-model heuristic.
pub fn estimate_tokens_for_model(text: &str, model: &str) -> usize {
    TOKEN_COUNTER.count(text, model)
}

/// Estimate total tokens for a conversation (using default heuristic).
/// Prefer `estimate_conversation_tokens_for_model` when the model is known.
pub fn estimate_conversation_tokens(conversation: &Conversation) -> usize {
    let system_tokens = estimate_tokens(&conversation.system_prompt);

    let message_tokens: usize = conversation
        .messages
        .iter()
        .map(|msg| estimate_message_tokens(msg))
        .sum();

    system_tokens + message_tokens
}

/// Estimate total tokens for a conversation using model-specific tokenizer.
pub fn estimate_conversation_tokens_for_model(conversation: &Conversation, model: &str) -> usize {
    let system_tokens = estimate_tokens_for_model(&conversation.system_prompt, model);

    let message_tokens: usize = conversation
        .messages
        .iter()
        .map(|msg| estimate_message_tokens_for_model(msg, model))
        .sum();

    system_tokens + message_tokens
}

/// Estimate tokens for a single message (using default heuristic).
fn estimate_message_tokens(msg: &Message) -> usize {
    estimate_message_tokens_for_model(msg, "default")
}

/// Estimate tokens for a single message with model-specific tokenizer.
fn estimate_message_tokens_for_model(msg: &Message, model: &str) -> usize {
    let overhead = 4; // role + formatting tokens

    let content_tokens: usize = msg
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => estimate_tokens_for_model(text, model),
            ContentBlock::Image { source, .. } => {
                // Estimate tokens for image based on base64 data size
                // For OpenAI vision models, each image token represents a 512x512 tile
                // We'll use a rough estimate: 85 tokens per 1000 base64 characters
                match source {
                    ImageSource::Base64 { data, .. } => {
                        // Rough estimate: 85 tokens per 1000 base64 chars
                        (data.len() * 85 / 1000).max(1)
                    }
                }
            }
            ContentBlock::ToolUse { name, input, .. } => {
                estimate_tokens_for_model(name, model) + estimate_tokens_for_model(&input.to_string(), model)
            }
            ContentBlock::ToolResult { content, .. } => estimate_tokens_for_model(content, model),
            ContentBlock::Thinking { thinking, .. } => estimate_tokens_for_model(thinking, model),
        })
        .sum();

    overhead + content_tokens
}

/// Information about context window status
pub struct ContextStatus {
    pub estimated_tokens: usize,
    pub max_tokens: usize,
    pub usage_percent: f32,
    pub needs_truncation: bool,
}

/// Check context window status
pub fn check_context(conversation: &Conversation, model: &str) -> ContextStatus {
    let max = max_context_tokens(model);
    let estimated = estimate_conversation_tokens_for_model(conversation, model);
    let usage = estimated as f32 / max as f32 * 100.0;

    ContextStatus {
        estimated_tokens: estimated,
        max_tokens: max,
        usage_percent: usage,
        // Start truncating at 80% to leave room for the response
        needs_truncation: usage > 80.0,
    }
}

/// Truncate conversation to fit within context window.
/// Strategy:
/// 1. Keep the system prompt (always)
/// 2. Keep the first user message (provides session context)
/// 3. Keep the most recent N messages
/// 4. Remove middle messages, replacing with a summary
///
/// IMPORTANT: tool_use / tool_result messages are always kept as atomic pairs
/// to satisfy the Anthropic API constraint that every tool_use must be followed
/// by a tool_result in the very next message.
pub fn truncate_conversation(conversation: &mut Conversation, model: &str, memory: &dyn crate::memory::MemoryProvider) {
    // Use the plan + apply pipeline with a mechanical summary fallback.
    if let Some(plan) = plan_truncation(conversation, model) {
        let summary = summarize_removed_messages(
            &conversation.messages[plan.remove_start..plan.remove_end],
        );
        apply_truncation(conversation, &plan, &summary, memory);
    } else {
        // Too few messages — just truncate oversized blocks
        truncate_large_blocks(conversation);
    }
}

/// A planned truncation: describes what to keep and what to remove.
pub struct TruncationPlan {
    /// Number of messages to keep from the start of the conversation.
    pub keep_start: usize,
    /// Index (inclusive) of the first message to remove.
    pub remove_start: usize,
    /// Index (exclusive) of the last message to remove.
    pub remove_end: usize,
    /// Messages to keep from the end of the conversation.
    pub kept_end: Vec<Message>,
    /// Total count of messages being removed.
    pub removed_count: usize,
}

/// Determine what to truncate without actually modifying the conversation.
///
/// Returns `None` if truncation is not needed or not feasible (too few messages).
pub fn plan_truncation(conversation: &Conversation, model: &str) -> Option<TruncationPlan> {
    let max = max_context_tokens(model);
    let target = max * 60 / 100;

    let total = estimate_conversation_tokens_for_model(conversation, model);
    if total <= target {
        return None;
    }

    let msg_count = conversation.messages.len();
    if msg_count <= 4 {
        return None;
    }

    let mut first_keep = 2.min(msg_count);
    while first_keep < msg_count {
        let last = &conversation.messages[first_keep - 1];
        if message_has_tool_use(last) || message_has_tool_result(last) {
            first_keep += 1;
        } else if message_has_large_thinking(last) {
            // Thinking blocks are model scratchpad — they're verbose (up to
            // 8000 tokens each) and don't provide lasting value for future
            // turns.  Prefer removing them over actual conversation content.
            // Signed thinking blocks from more recent messages (in kept_end)
            // are still preserved for API echo-back requirements.
            first_keep += 1;
        } else {
            break;
        }
    }

    let first_tokens: usize = conversation.messages[..first_keep]
        .iter()
        .map(|m| estimate_message_tokens_for_model(m, model))
        .sum();

    let system_tokens = estimate_tokens_for_model(&conversation.system_prompt, model);
    let summary_overhead = 200; // tokens for the truncation notice (larger for LLM summary)
    let available = target.saturating_sub(system_tokens + first_tokens + summary_overhead);

    let middle = &conversation.messages[first_keep..];
    let mut kept_end = Vec::new();
    let mut end_tokens = 0;
    let mut i = middle.len();

    while i > 0 {
        i -= 1;
        let msg = &middle[i];
        let msg_tokens = estimate_message_tokens_for_model(msg, model);

        if message_has_tool_result(msg) && i > 0 {
            let prev = &middle[i - 1];
            let pair_tokens = msg_tokens + estimate_message_tokens_for_model(prev, model);
            if end_tokens + pair_tokens > available {
                break;
            }
            kept_end.push(msg.clone());
            kept_end.push(prev.clone());
            end_tokens += pair_tokens;
            i -= 1;
        } else {
            if end_tokens + msg_tokens > available {
                break;
            }
            kept_end.push(msg.clone());
            end_tokens += msg_tokens;
        }
    }

    kept_end.reverse();

    let removed_count = msg_count - first_keep - kept_end.len();
    if removed_count == 0 {
        return None;
    }

    let remove_start = first_keep;
    let remove_end = msg_count - kept_end.len();

    Some(TruncationPlan {
        keep_start: first_keep,
        remove_start,
        remove_end,
        kept_end,
        removed_count,
    })
}

/// Build a condensed text representation of messages about to be removed.
///
/// This is designed to be fed to an LLM for narrative summarization.
/// It captures the high-level flow: what was discussed, what tools were
/// used, what files were touched, and any key conclusions.
pub fn build_truncation_context(messages: &[Message]) -> String {
    /// Truncate `s` to at most `max_bytes` bytes, always landing on a valid
    /// UTF-8 char boundary so we never panic on multi-byte characters.
    fn safe_truncate(s: &str, max_bytes: usize) -> &str {
        if s.len() <= max_bytes {
            return s;
        }
        let mut boundary = max_bytes;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &s[..boundary]
    }
    let mut parts: Vec<String> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        let role = match msg.role {
            crate::conversation::Role::User => "User",
            crate::conversation::Role::Assistant => "Assistant",
            crate::conversation::Role::System => "System",
        };

        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    // Truncate long text blocks to keep the context prompt small
                    let truncated = if text.len() > 300 {
                        format!("{}... [truncated, {} chars total]", safe_truncate(text, 300), text.len())
                    } else {
                        text.clone()
                    };
                    parts.push(format!("[Msg {}] {}: {}", i + 1, role, truncated));
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let extra = match name.as_str() {
                        "run_command" => input.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        "grep_search" | "file_search" => input.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        _ => path.to_string(),
                    };
                    parts.push(format!("[Msg {}] Tool call: {} ({})", i + 1, name, extra));
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let status = if is_error.unwrap_or(false) { "ERROR" } else { "OK" };
                    let preview = if content.len() > 150 {
                        format!("{}...", safe_truncate(content, 150))
                    } else {
                        content.clone()
                    };
                    parts.push(format!("[Msg {}] Tool result ({}): {}", i + 1, status, preview));
                }
                ContentBlock::Thinking { .. } => {
                    // Skip thinking blocks in context summary (they can be very long)
                }
                ContentBlock::Image { .. } => {
                    parts.push(format!("[Msg {}] {}: [Image content]", i + 1, role));
                }
            }
        }
    }

    // Cap the total context to ~3000 chars to keep the LLM summarization prompt cheap
    let joined = parts.join("\n");
    if joined.len() > 3000 {
        format!("{}...\n[{} more entries omitted]", safe_truncate(&joined, 3000), parts.len())
    } else {
        joined
    }
}

/// Apply a truncation plan to the conversation with the given summary text.
///
/// This is the second phase of the truncation pipeline. The summary can be
/// either a mechanical summary (from `summarize_removed_messages`) or an
/// LLM-generated narrative.
pub fn apply_truncation(
    conversation: &mut Conversation,
    plan: &TruncationPlan,
    summary: &str,
    memory: &dyn crate::memory::MemoryProvider,
) {
    // Delegate to the memory provider — backend decides how to persist.
    memory.log_truncation(summary);

    // Build new message list
    let mut new_messages: Vec<Message> = Vec::new();
    new_messages.extend_from_slice(&conversation.messages[..plan.keep_start]);

    new_messages.push(Message::user(&format!(
        "[System: {} earlier messages were removed to fit the context window. \
         Summary of removed conversation:\n{}\n\
         The conversation continues from the most recent messages below.]",
        plan.removed_count, summary
    )));

    new_messages.extend(plan.kept_end.clone());
    conversation.messages = new_messages;

    tracing::info!(
        "Truncated conversation: removed {} messages, kept {} messages (~{} tokens)",
        plan.removed_count,
        conversation.messages.len(),
        estimate_conversation_tokens(conversation)
    );

    ensure_tool_pair_integrity(&mut conversation.messages);
    truncate_large_blocks(conversation);
}

/// Summarize removed messages using an LLM for narrative quality.
///
/// Builds a condensed context from the messages, sends it to the LLM with a
/// summarization prompt, and returns the LLM's response. Falls back to the
/// mechanical `summarize_removed_messages()` if the LLM call fails or the
/// response is empty.
pub async fn summarize_with_llm(
    messages: &[Message],
    client: &dyn crate::llm::LlmClient,
) -> String {
    const SUMMARY_SYSTEM_PROMPT: &str = "\
You are a conversation summarizer. Your task is to summarize the key actions, \
decisions, and outcomes from the conversation segment below. \
Focus on: what files were modified, what commands were run, what was discussed, \
and any conclusions reached. \
Keep the summary concise (2-5 sentences). Do not add commentary or analysis.";

    let context = build_truncation_context(messages);
    let mut conv = crate::conversation::Conversation::with_system_prompt(
        SUMMARY_SYSTEM_PROMPT.to_string()
    );
    conv.messages.push(crate::conversation::Message::user(&format!(
        "Summarize this conversation segment:\n\n{}", context
    )));

    match client.send_message(&conv, &[]).await {
        Ok(response) => {
            let text: String = response.content.iter()
                .filter_map(|b| {
                    if let crate::conversation::ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                tracing::debug!("LLM summary was empty, using mechanical fallback");
                summarize_removed_messages(messages)
            } else {
                tracing::debug!("LLM generated summary: {} chars", text.len());
                text
            }
        }
        Err(e) => {
            tracing::warn!("LLM summarization failed: {}, using mechanical fallback", e);
            summarize_removed_messages(messages)
        }
    }
}

/// Async variant of `truncate_conversation` that uses LLM for summarization.
///
/// When `client` is Some, removed messages are summarized by the LLM for a
/// richer, more contextual narrative. Falls back to mechanical summary if the
/// LLM call fails. When `client` is None, uses the mechanical summarizer directly.
pub async fn truncate_conversation_llm(
    conversation: &mut Conversation,
    model: &str,
    memory: &dyn crate::memory::MemoryProvider,
    client: Option<&dyn crate::llm::LlmClient>,
) {
    if let Some(plan) = plan_truncation(conversation, model) {
        let summary = if let Some(client) = client {
            summarize_with_llm(
                &conversation.messages[plan.remove_start..plan.remove_end],
                client,
            ).await
        } else {
            summarize_removed_messages(
                &conversation.messages[plan.remove_start..plan.remove_end],
            )
        };
        apply_truncation(conversation, &plan, &summary, memory);
    } else {
        truncate_large_blocks(conversation);
    }
}

/// Truncate individual large content blocks (e.g., huge file contents or command outputs)
fn truncate_large_blocks(conversation: &mut Conversation) {
    let max_block_tokens = 8000;

    for msg in &mut conversation.messages {
        for block in &mut msg.content {
            match block {
                ContentBlock::ToolResult { content, .. } => {
                    let tokens = estimate_tokens(content);
                    if tokens > max_block_tokens {
                        let max_chars = max_block_tokens * 4;
                        let mut half = max_chars / 2;
                        if content.len() > max_chars {
                            // Find safe char boundaries
                            while half > 0 && !content.is_char_boundary(half) {
                                half -= 1;
                            }
                            let mut end_start = content.len() - (max_chars / 2);
                            while end_start < content.len() && !content.is_char_boundary(end_start) {
                                end_start += 1;
                            }

                            let truncated = format!(
                                "{}\n\n... [{} characters truncated] ...\n\n{}",
                                &content[..half],
                                content.len() - (half + (content.len() - end_start)),
                                &content[end_start..]
                            );
                            *content = truncated;
                        }
                    }
                }
                ContentBlock::Text { text } => {
                    let tokens = estimate_tokens(text);
                    if tokens > max_block_tokens * 2 {
                        let max_chars = max_block_tokens * 8;
                        let mut half = max_chars / 2;
                        if text.len() > max_chars {
                            // Find safe char boundaries
                            while half > 0 && !text.is_char_boundary(half) {
                                half -= 1;
                            }
                            let mut end_start = text.len() - (max_chars / 2);
                            while end_start < text.len() && !text.is_char_boundary(end_start) {
                                end_start += 1;
                            }

                            let truncated = format!(
                                "{}\n\n... [{} characters truncated] ...\n\n{}",
                                &text[..half],
                                text.len() - (half + (text.len() - end_start)),
                                &text[end_start..]
                            );
                            *text = truncated;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Generate a compact mechanical summary of removed messages.
/// Extracts tool names and file paths to produce something like:
/// "Read main.c, edited gpio.dts, ran 'make dtbs'"
pub fn summarize_removed_messages(messages: &[Message]) -> String {
    let mut actions: Vec<String> = Vec::new();

    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { name, input, .. } => {
                    let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let action = match name.as_str() {
                        "read_file" => format!("read {}", path),
                        "write_file" => format!("wrote {}", path),
                        "edit_file" => format!("edited {}", path),
                        "run_command" => {
                            let cmd = input
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            let short = crate::ui::truncate_str(cmd, 40);
                            format!("ran `{}`", short)
                        }
                        "grep_search" | "file_search" => {
                            let pattern = input
                                .get("pattern")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("searched '{}'", pattern)
                        }
                        "list_directory" => format!("listed {}", path),
                        _ => format!("{}", name),
                    };
                    actions.push(action);
                }
                _ => {}
            }
        }
    }

    if actions.is_empty() {
        "general discussion".to_string()
    } else {
        // Deduplicate consecutive identical actions
        actions.dedup();
        // Keep at most 10 actions to stay compact
        if actions.len() > 10 {
            let kept = &actions[actions.len() - 10..];
            format!("...and then: {}", kept.join(", "))
        } else {
            actions.join(", ")
        }
    }
}

/// Check if a message contains any ToolUse blocks.
fn message_has_tool_use(msg: &Message) -> bool {
    msg.content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }))
}

/// Check if a message contains any ToolResult blocks.
fn message_has_tool_result(msg: &Message) -> bool {
    msg.content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
}

/// Check if a message contains thinking blocks that are large enough
/// to justify removal during truncation (≥ 500 estimated tokens).
/// Small thinking blocks are kept — they don't bloat context significantly.
fn message_has_large_thinking(msg: &Message) -> bool {
    msg.content.iter().any(|b| {
        if let ContentBlock::Thinking { thinking, .. } = b {
            estimate_tokens(thinking) >= 500
        } else {
            false
        }
    })
}

/// Safety net: ensure every tool_use has a matching tool_result and vice versa.
/// Also verify ordering: the tool_result must appear in the message immediately
/// following the one containing the tool_use.
/// Removes orphaned or misordered blocks to prevent Anthropic API errors like:
///   "tool_use ids were found without tool_result blocks immediately after"
pub fn ensure_tool_pair_integrity(messages: &mut Vec<Message>) {
    use std::collections::{HashMap, HashSet};

    // Phase 1: collect all tool_use IDs with their message index,
    // and all tool_result IDs with their message index.
    let mut use_id_to_msg: HashMap<String, usize> = HashMap::new();
    let mut result_id_to_msg: HashMap<String, usize> = HashMap::new();

    for (idx, msg) in messages.iter().enumerate() {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    use_id_to_msg.insert(id.clone(), idx);
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    result_id_to_msg.insert(tool_use_id.clone(), idx);
                }
                _ => {}
            }
        }
    }

    // Phase 2: find IDs to remove.
    // A pair is valid when:
    //   - Both tool_use and tool_result exist
    //   - The tool_result message index == tool_use message index + 1
    //     (they must be in adjacent messages)
    //
    // NOTE: After api_messages() merges consecutive same-role messages
    // the adjacency might shift, so we also accept tool_result in a
    // later user message as long as no assistant message intervenes.
    // However, the safest approach is to just require use_idx < result_idx.
    let mut bad_ids: HashSet<String> = HashSet::new();

    // Orphaned tool_uses (no matching result)
    for id in use_id_to_msg.keys() {
        if !result_id_to_msg.contains_key(id) {
            bad_ids.insert(id.clone());
        }
    }

    // Orphaned tool_results (no matching use)
    for id in result_id_to_msg.keys() {
        if !use_id_to_msg.contains_key(id) {
            bad_ids.insert(id.clone());
        }
    }

    // Misordered pairs (result appears before or at the same index as use)
    for (id, use_idx) in &use_id_to_msg {
        if let Some(result_idx) = result_id_to_msg.get(id) {
            if *result_idx <= *use_idx {
                bad_ids.insert(id.clone());
                continue;
            }
            // Anthropic requires tool_result in the IMMEDIATELY NEXT user
            // message after the assistant message with tool_use.  After
            // api_messages() merges consecutive same-role messages, this
            // means there must be no assistant message between the
            // tool_use message and the tool_result message.
            let has_intervening_assistant = messages[*use_idx + 1..*result_idx]
                .iter()
                .any(|m| m.role == crate::conversation::Role::Assistant);
            if has_intervening_assistant {
                bad_ids.insert(id.clone());
            }
        }
    }

    if bad_ids.is_empty() {
        return; // All pairs are intact and correctly ordered
    }

    tracing::info!(
        "Fixing tool pair integrity: removing {} broken tool_use/tool_result ID(s)",
        bad_ids.len()
    );

    // Remove bad blocks from messages
    for msg in messages.iter_mut() {
        msg.content.retain(|block| match block {
            ContentBlock::ToolUse { id, .. } => !bad_ids.contains(id),
            ContentBlock::ToolResult { tool_use_id, .. } => !bad_ids.contains(tool_use_id),
            _ => true,
        });
    }

    // Remove any messages that became empty after block removal
    messages.retain(|msg| !msg.content.is_empty());
}
