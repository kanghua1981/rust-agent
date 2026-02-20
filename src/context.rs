//! Context window management.
//!
//! Monitors the estimated token usage of the conversation and
//! automatically truncates or summarizes when approaching the
//! model's context window limit.

use crate::conversation::{ContentBlock, Conversation, Message};

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
            ContentBlock::ToolUse { name, input, .. } => {
                estimate_tokens(name) + estimate_tokens(&input.to_string())
            }
            ContentBlock::ToolResult { content, .. } => estimate_tokens(content),
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
    let max = max_context_tokens(model);
    let target = max * 60 / 100; // Target 60% usage after truncation

    let total = estimate_conversation_tokens(conversation);
    if total <= target {
        return; // No truncation needed
    }

    let msg_count = conversation.messages.len();
    if msg_count <= 4 {
        // Too few messages to truncate meaningfully
        // Just truncate large tool results
        truncate_large_blocks(conversation);
        return;
    }

    // Keep first 2 messages and as many recent messages as we can.
    // Adjust boundary to avoid splitting a tool_use/tool_result pair:
    // if the last kept-from-start message is an assistant with tool_use,
    // extend to also keep its tool_result.
    let mut first_keep = 2.min(msg_count);
    while first_keep < msg_count && message_has_tool_use(&conversation.messages[first_keep - 1]) {
        first_keep += 1;
    }

    let first_tokens: usize = conversation.messages[..first_keep]
        .iter()
        .map(estimate_message_tokens)
        .sum();

    let system_tokens = estimate_tokens(&conversation.system_prompt);
    let summary_overhead = 100; // tokens for the truncation notice
    let available = target.saturating_sub(system_tokens + first_tokens + summary_overhead);

    // Add recent messages from the end until we run out of budget.
    // Keep tool_use/tool_result pairs together as atomic units.
    let middle = &conversation.messages[first_keep..];
    let mut kept_end = Vec::new();
    let mut end_tokens = 0;
    let mut i = middle.len();

    while i > 0 {
        i -= 1;
        let msg = &middle[i];
        let msg_tokens = estimate_message_tokens(msg);

        // If this message contains tool_result blocks, it must be kept
        // together with the preceding message (which has the tool_use).
        if message_has_tool_result(msg) && i > 0 {
            let prev = &middle[i - 1];
            let pair_tokens = msg_tokens + estimate_message_tokens(prev);
            if end_tokens + pair_tokens > available {
                break; // Can't fit the pair
            }
            // Push both in reverse (will be reversed later)
            kept_end.push(msg.clone());
            kept_end.push(prev.clone());
            end_tokens += pair_tokens;
            i -= 1; // skip the tool_use message we already added
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

    if removed_count > 0 {
        // Generate a mechanical summary of the removed messages
        let removed_start = first_keep;
        let removed_end = msg_count - kept_end.len();
        let summary = summarize_removed_messages(
            &conversation.messages[removed_start..removed_end],
        );

        // Save summary to persistent memory
        {
            let mut mem = crate::memory::Memory::load(project_dir);
            mem.log_truncation_summary(&summary);
            if let Err(e) = mem.save() {
                tracing::warn!("Failed to save truncation summary to memory: {}", e);
            }
        }

        // Build new message list
        let mut new_messages: Vec<Message> = Vec::new();

        // Keep first messages
        new_messages.extend_from_slice(&conversation.messages[..first_keep]);

        // Add truncation notice with summary
        new_messages.push(Message::user(&format!(
            "[System: {} earlier messages were removed to fit the context window. \
             Summary of removed content: {}. \
             The conversation continues from the most recent messages below.]",
            removed_count, summary
        )));

        // Keep recent messages
        new_messages.extend(kept_end);

        conversation.messages = new_messages;

        tracing::info!(
            "Truncated conversation: removed {} messages, kept {} messages (~{} tokens)",
            removed_count,
            conversation.messages.len(),
            estimate_conversation_tokens(conversation)
        );
    }

    // Safety net: remove any orphaned tool_use/tool_result blocks
    // that slipped through despite the pair-aware logic above.
    ensure_tool_pair_integrity(&mut conversation.messages);

    // Also truncate very large individual blocks
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
fn summarize_removed_messages(messages: &[Message]) -> String {
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
/// Removes orphaned blocks to prevent Anthropic API errors like:
///   "tool_use ids were found without tool_result blocks immediately after"
fn ensure_tool_pair_integrity(messages: &mut Vec<Message>) {
    use std::collections::HashSet;

    // Collect all tool_use IDs and tool_result IDs
    let mut use_ids = HashSet::new();
    let mut result_ids = HashSet::new();

    for msg in messages.iter() {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    use_ids.insert(id.clone());
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    result_ids.insert(tool_use_id.clone());
                }
                _ => {}
            }
        }
    }

    // Find orphans: tool_use without tool_result, and vice versa
    let orphaned_uses: HashSet<_> = use_ids.difference(&result_ids).cloned().collect();
    let orphaned_results: HashSet<_> = result_ids.difference(&use_ids).cloned().collect();

    if orphaned_uses.is_empty() && orphaned_results.is_empty() {
        return; // All pairs are intact
    }

    // Remove orphaned blocks from messages
    for msg in messages.iter_mut() {
        msg.content.retain(|block| match block {
            ContentBlock::ToolUse { id, .. } => !orphaned_uses.contains(id),
            ContentBlock::ToolResult { tool_use_id, .. } => {
                !orphaned_results.contains(tool_use_id)
            }
            _ => true,
        });
    }

    // Remove any messages that became empty after block removal
    messages.retain(|msg| !msg.content.is_empty());

    tracing::info!(
        "Fixed tool pair integrity: removed {} orphaned tool_use, {} orphaned tool_result blocks",
        orphaned_uses.len(),
        orphaned_results.len()
    );
}
