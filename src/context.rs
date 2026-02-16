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

    // Keep first 2 messages and as many recent messages as we can
    let first_msgs = 2.min(msg_count);
    let first_tokens: usize = conversation.messages[..first_msgs]
        .iter()
        .map(estimate_message_tokens)
        .sum();

    let system_tokens = estimate_tokens(&conversation.system_prompt);
    let summary_overhead = 100; // tokens for the truncation notice
    let available = target.saturating_sub(system_tokens + first_tokens + summary_overhead);

    // Add recent messages from the end until we run out of budget
    let mut kept_end = Vec::new();
    let mut end_tokens = 0;

    for msg in conversation.messages[first_msgs..].iter().rev() {
        let msg_tokens = estimate_message_tokens(msg);
        if end_tokens + msg_tokens > available {
            break;
        }
        kept_end.push(msg.clone());
        end_tokens += msg_tokens;
    }

    kept_end.reverse();

    let removed_count = msg_count - first_msgs - kept_end.len();

    if removed_count > 0 {
        // Generate a mechanical summary of the removed messages
        let removed_start = first_msgs;
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
        new_messages.extend_from_slice(&conversation.messages[..first_msgs]);

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
                        let half = max_chars / 2;
                        if content.len() > max_chars {
                            let truncated = format!(
                                "{}\n\n... [{} characters truncated] ...\n\n{}",
                                &content[..half],
                                content.len() - max_chars,
                                &content[content.len() - half..]
                            );
                            *content = truncated;
                        }
                    }
                }
                ContentBlock::Text { text } => {
                    let tokens = estimate_tokens(text);
                    if tokens > max_block_tokens * 2 {
                        let max_chars = max_block_tokens * 8;
                        let half = max_chars / 2;
                        if text.len() > max_chars {
                            let truncated = format!(
                                "{}\n\n... [{} characters truncated] ...\n\n{}",
                                &text[..half],
                                text.len() - max_chars,
                                &text[text.len() - half..]
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
                            let short = if cmd.len() > 40 {
                                format!("{}...", &cmd[..37])
                            } else {
                                cmd.to_string()
                            };
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
