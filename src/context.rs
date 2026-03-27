//! Context window management.
//!
//! Monitors the estimated token usage of the conversation and
//! automatically truncates or summarizes when approaching the
//! model's context window limit.

use crate::conversation::{ContentBlock, Conversation, ImageSource, Message};

/// Estimated max context tokens for different models
pub fn max_context_tokens(model: &str) -> usize {
    if model.contains("claude") {
        200_000
    } else if model.contains("gpt-4o") {
        128_000
    } else if model.contains("gpt-4") {
        128_000
    } else if model.contains("gpt-3.5") {
        16_000
    } else {
        // Conservative default
        100_000
    }
}

/// Rough estimate of tokens in a string (~4 chars per token for English,
/// ~2 chars per token for CJK).
/// This is a heuristic, not exact.
pub fn estimate_tokens(text: &str) -> usize {
    // Count CJK characters vs ASCII
    let cjk_chars = text
        .chars()
        .filter(|c| {
            let c = *c as u32;
            (0x4E00..=0x9FFF).contains(&c)    // CJK Unified
                || (0x3000..=0x303F).contains(&c)  // CJK Punctuation
                || (0x3040..=0x30FF).contains(&c)  // Hiragana/Katakana
                || (0xAC00..=0xD7AF).contains(&c)  // Hangul
        })
        .count();

    let ascii_chars = text.len().saturating_sub(cjk_chars);

    // CJK: ~1.5 tokens per character, ASCII: ~0.25 tokens per character
    (cjk_chars * 3 / 2) + (ascii_chars / 4) + 1
}

/// Estimate total tokens for a conversation
pub fn estimate_conversation_tokens(conversation: &Conversation) -> usize {
    let system_tokens = estimate_tokens(&conversation.system_prompt);

    let message_tokens: usize = conversation
        .messages
        .iter()
        .map(|msg| estimate_message_tokens(msg))
        .sum();

    system_tokens + message_tokens
}

/// Estimate tokens for a single message
fn estimate_message_tokens(msg: &Message) -> usize {
    let overhead = 4; // role + formatting tokens

    let content_tokens: usize = msg
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => estimate_tokens(text),
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
                estimate_tokens(name) + estimate_tokens(&input.to_string())
            }
            ContentBlock::ToolResult { content, .. } => estimate_tokens(content),
            ContentBlock::Thinking { thinking } => estimate_tokens(thinking),
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
    let estimated = estimate_conversation_tokens(conversation);
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
pub fn truncate_conversation(conversation: &mut Conversation, model: &str, project_dir: &std::path::Path) {
    // Use the plan + apply pipeline with a mechanical summary fallback.
    if let Some(plan) = plan_truncation(conversation, model) {
        let summary = summarize_removed_messages(
            &conversation.messages[plan.remove_start..plan.remove_end],
        );
        apply_truncation(conversation, &plan, &summary, project_dir);
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

    let total = estimate_conversation_tokens(conversation);
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
        } else {
            break;
        }
    }

    let first_tokens: usize = conversation.messages[..first_keep]
        .iter()
        .map(estimate_message_tokens)
        .sum();

    let system_tokens = estimate_tokens(&conversation.system_prompt);
    let summary_overhead = 200; // tokens for the truncation notice (larger for LLM summary)
    let available = target.saturating_sub(system_tokens + first_tokens + summary_overhead);

    let middle = &conversation.messages[first_keep..];
    let mut kept_end = Vec::new();
    let mut end_tokens = 0;
    let mut i = middle.len();

    while i > 0 {
        i -= 1;
        let msg = &middle[i];
        let msg_tokens = estimate_message_tokens(msg);

        if message_has_tool_result(msg) && i > 0 {
            let prev = &middle[i - 1];
            let pair_tokens = msg_tokens + estimate_message_tokens(prev);
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
    project_dir: &std::path::Path,
) {
    // Save summary to persistent memory
    {
        let mut mem = crate::memory::Memory::load(project_dir);
        mem.log_truncation_summary(summary);
        if let Err(e) = mem.save() {
            tracing::warn!("Failed to save truncation summary to memory: {}", e);
        }
    }

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
