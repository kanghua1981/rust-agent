use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Role in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A content block in a message (text, image, or tool use/result)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image {
        source: ImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },

    /// Extended thinking content (Anthropic native + DeepSeek reasoner).
    /// Stored in conversation history so it can be echoed back in the next
    /// request.
    /// `signature` is an opaque token returned by Anthropic that MUST be
    /// echoed back verbatim — omitting it causes a 400 error:
    ///   "The `content[].thinking` in the thinking mode must be passed back to the API."
    /// For the DeepSeek OpenAI-compatible path this field is serialized as
    /// `reasoning_content` instead.
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        /// Anthropic-specific verification token; absent for DeepSeek/OpenAI paths.
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

/// Source of an image content block
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ImageSource {
    #[serde(rename = "base64")]
    Base64 {
        media_type: String,
        data: String,
    },
}

/// A single message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: &str) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content,
        }
    }

    pub fn tool_result(tool_use_id: &str, result: &str, is_error: bool) -> Self {
        Message {
            id: Uuid::new_v4().to_string(),
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: result.to_string(),
                is_error: if is_error { Some(true) } else { None },
            }],
        }
    }

    /// Extract text content from this message
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if this message contains tool use requests
    #[allow(dead_code)]
    pub fn has_tool_use(&self) -> bool {
        self.content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
    }

    /// Extract all tool use blocks
    #[allow(dead_code)]
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some((id.as_str(), name.as_str(), input))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// The typed vocabulary of the session log.
///
/// This is the append-only, replayable recording of a session — the source of
/// truth. Message events carry the full message (any role), so
/// [Conversation::derive_messages] reconstructs the model history from them;
/// turn/tool/chunk/completion events are supplementary fidelity for replay and
/// observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionEventKind {
    TurnStart { turn: u64 },
    TurnEnd { turn: u64 },
    StepStart { turn: u64, step: u64 },
    StepEnd { turn: u64, step: u64 },
    UserMessage { message: Message },
    AssistantMessage { message: Message },
    SystemMessage { message: Message },
    ToolCall { id: String, name: String, input: serde_json::Value },
    ToolResult { id: String, is_error: bool },
    AssistantChunk { text: String },
    Compaction { surface_start: usize, surface_end: usize, summary: String },
    SessionEndSeed,
}

impl SessionEventKind {
    /// Classify a message into its matching message-level event variant.
    pub fn from_message(message: &Message) -> Self {
        match message.role {
            Role::User => SessionEventKind::UserMessage { message: message.clone() },
            Role::Assistant => SessionEventKind::AssistantMessage { message: message.clone() },
            Role::System => SessionEventKind::SystemMessage { message: message.clone() },
        }
    }

    /// The message carried by a message-level event, if any.
    pub fn message(&self) -> Option<&Message> {
        match self {
            SessionEventKind::UserMessage { message }
            | SessionEventKind::AssistantMessage { message }
            | SessionEventKind::SystemMessage { message } => Some(message),
            _ => None,
        }
    }
}

/// One append-only session log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Monotonic append position within the session (0-based).
    pub seq: u64,
    /// Epoch milliseconds when the event was appended.
    pub time: u64,
    /// The typed event payload.
    pub kind: SessionEventKind,
}

/// Incremental token-estimate cache. Stored behind interior mutability so the
/// read-only estimate functions can populate it. `model` is the model the
/// counts belong to; `counted_len`/`tail_tokens` hold the running message count
/// and the token total of messages [0..counted_len).
#[derive(Debug, Default)]
struct TokenEstimateCache {
    model: String,
    counted_len: usize,
    tail_tokens: usize,
}

/// Manages the conversation history
#[derive(Debug)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub system_prompt: String,
    /// Append-only typed session log (the authoritative recording).
    pub log: Vec<SessionEvent>,
    /// Delegation depth of the agent this conversation belongs to (0 = top).
    /// Persisted with the session so a resumed sub-agent keeps its depth cap.
    pub delegation_depth: usize,
    /// Incremental token-estimate cache; see [Conversation::token_estimate].
    token_estimate_cache: std::sync::Mutex<TokenEstimateCache>,
}

impl Conversation {
    /// Create a minimal conversation with just a system prompt (no project loading).
    pub fn with_system_prompt(system_prompt: String) -> Self {
        Conversation {
            messages: Vec::new(),
            system_prompt,
            log: Vec::new(),
            delegation_depth: 0,
            token_estimate_cache: std::sync::Mutex::new(TokenEstimateCache::default()),
        }
    }

    pub fn new(project_dir: &Path) -> Self {
        let mut system_prompt = Self::build_system_prompt(project_dir);

        // Load project summary (from .agent/summary.md)
        if let Some(summary) = crate::summary::load(project_dir) {
            system_prompt.push_str(&crate::summary::to_system_prompt_section(&summary));
            tracing::info!("Loaded project summary into system prompt");
        }

        // Load project skills
        let loaded = crate::skills::load_skills(project_dir);
        if !loaded.is_empty() {
            system_prompt.push_str(&loaded.to_system_prompt_section());
            tracing::info!("Loaded {} skill(s) into system prompt", loaded.len());
        }

        // Load persistent memory — only knowledge facts go into the system prompt.
        // File-map and session-log are injected per-turn via recall_relevant() in agent.rs.
        let mem = crate::memory::Memory::load(project_dir);
        let knowledge_section = mem.to_system_prompt_knowledge();
        if !knowledge_section.is_empty() {
            system_prompt.push_str(&knowledge_section);
            tracing::info!("Loaded {} knowledge entries into system prompt", mem.knowledge.len());
        }

        // Add sub-agents information to system prompt
        let models_cfg = crate::model_manager::load();
        if !models_cfg.sub_agents.is_empty() {
            system_prompt.push_str("\n\n## Available Sub-Agents\n");
            system_prompt.push_str("You can delegate specialized tasks to the following agents running in server mode. Use the `call_node` tool with the node `target` name or direct WebSocket URL.\n\n");
            for (name, sa) in &models_cfg.sub_agents {
                let role_info = if let Some(role) = &sa.role {
                    format!(" (Role: {})", role)
                } else {
                    String::new()
                };
                system_prompt.push_str(&format!("- **{}**: ws://localhost:{}{}\n", name, sa.port, role_info));
            }
            system_prompt.push_str("\nWhen delegating, prefer using the `target_dir` parameter to isolate the sub-agent's work to a specific directory.\n");
        }

        Conversation {
            messages: Vec::new(),
            system_prompt,
            log: Vec::new(),
            delegation_depth: 0,
            token_estimate_cache: std::sync::Mutex::new(TokenEstimateCache::default()),
        }
    }

    /// Build the system prompt with support for user customization.
    ///
    /// Loading order (later sources append to or override earlier ones):
    ///   1. Built-in default prompt
    ///   2. Global custom prompt: `~/.config/rust_agent/system_prompt.md`
    ///   3. Project custom prompt: `<project>/.agent/system_prompt.md`
    ///
    /// If a custom prompt file starts with `# OVERRIDE`, it completely replaces
    /// all previous prompt content. Otherwise it appends.
    fn build_system_prompt(project_dir: &Path) -> String {
        let mut prompt = Self::default_system_prompt(project_dir);

        // Global custom system prompt
        if let Some(config_dir) = dirs::config_dir() {
            let global_path = config_dir.join("rust_agent").join("system_prompt.md");
            if let Ok(content) = std::fs::read_to_string(&global_path) {
                let content = content.trim();
                if !content.is_empty() {
                    if content.starts_with("# OVERRIDE") {
                        // Strip the marker line and use the rest as full replacement
                        let body = content.strip_prefix("# OVERRIDE").unwrap_or(content).trim();
                        prompt = body.to_string();
                        tracing::info!("Global system_prompt.md OVERRIDES default prompt");
                    } else {
                        prompt.push_str("\n\n");
                        prompt.push_str(content);
                        tracing::info!("Appended global system_prompt.md");
                    }
                }
            }
        }

        // Project-level custom system prompt (takes highest priority)
        let project_path = project_dir.join(".agent").join("system_prompt.md");
        if let Ok(content) = std::fs::read_to_string(&project_path) {
            let content = content.trim();
            if !content.is_empty() {
                if content.starts_with("# OVERRIDE") {
                    let body = content.strip_prefix("# OVERRIDE").unwrap_or(content).trim();
                    prompt = body.to_string();
                    tracing::info!("Project system_prompt.md OVERRIDES all previous prompts");
                } else {
                    prompt.push_str("\n\n");
                    prompt.push_str(content);
                    tracing::info!("Appended project system_prompt.md");
                }
            }
        }

        prompt
    }

    fn default_system_prompt(project_dir: &Path) -> String {
        format!(
            r#"You are an expert AI coding assistant running in a terminal environment.
You have access to tools that let you read files, write files, run commands, search code, and more.

Current working directory: {}
Operating system: {}

Guidelines:
- Use tools to explore and understand the codebase before making changes
- Always read relevant files before editing them
- Make minimal, targeted changes
- Run tests after making changes when possible
- Explain what you're doing and why
- If you're unsure, ask for clarification
- Use the appropriate tool for each task

When writing or editing code:
- Follow existing code style and conventions
- Add appropriate error handling
- Write clean, idiomatic code
- Consider edge cases

Skills management:
- ALWAYS use the `create_skill` tool to create or update skills. NEVER use `write_file`
  or `edit_file` to directly create/modify files in `.agent/skills/`. The `create_skill`
  tool automatically generates the required YAML frontmatter format.
- Before creating a skill, check the Available Skills list in this prompt to avoid duplicates.
- To read the full content of an existing skill, use the `load_skill` tool."#,
            project_dir.display(),
            std::env::consts::OS
        )
    }

    pub fn add_message(&mut self, message: Message) {
        self.record(SessionEventKind::from_message(&message));
        self.messages.push(message);
    }

    /// Get messages formatted for the API (excluding system prompt).
    ///
    /// This method merges consecutive messages with the same role into a
    /// single message.  The Anthropic API requires strict user/assistant
    /// alternation and that **all** `tool_result` blocks for a given
    /// assistant response appear in the single next user message.  Without
    /// merging, truncation notices or per-tool-result messages can violate
    /// these constraints and trigger 400 errors such as:
    ///   "tool_use ids were found without tool_result blocks immediately after"
    pub fn api_messages(&self) -> Vec<serde_json::Value> {
        self.api_messages_inner(false)
    }

    /// Returns true if the conversation contains any assistant message with
    /// thinking blocks (with or without a valid signature).
    ///
    /// Anthropic REQUIRES `content[].thinking` blocks to be echoed back
    /// verbatim in ALL subsequent requests when extended thinking is enabled —
    /// regardless of whether the same message also contains tool_use blocks.
    /// Failing to do so results in a 400 error:
    ///   "The `content[].thinking` in the thinking mode must be passed back to the API."
    ///
    /// We detect ANY thinking block (even without a signature) to avoid
    /// the situation where unsigned blocks exist but are not recognized,
    /// causing `api_messages()` to filter them out while the API still
    /// expects them — producing a 400 error.
    ///
    /// Note: only blocks with a valid signature are actually echoed back
    /// in `api_messages_with_thinking()`. Unsigned blocks trigger
    /// auto-detection but are skipped during serialization.
    pub fn has_thinking_blocks(&self) -> bool {
        self.messages.iter().any(|m| {
            m.content.iter().any(|b| matches!(
                b,
                ContentBlock::Thinking { .. }
            ))
        })
    }

    /// Like `api_messages()` but preserves ALL `thinking` blocks that carry a
    /// valid signature, regardless of whether the message also contains tool_use
    /// blocks.  This is required by the Anthropic API: omitting any signed
    /// thinking block from a subsequent request results in a 400 error.
    pub fn api_messages_with_thinking(&self) -> Vec<serde_json::Value> {
        self.api_messages_inner(true)
    }

    fn api_messages_inner(&self, selective_thinking: bool) -> Vec<serde_json::Value> {
        if self.messages.is_empty() {
            return Vec::new();
        }

        let mut merged: Vec<serde_json::Value> = Vec::new();

        for msg in &self.messages {
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };

            // Include thinking blocks that carry a valid signature:
            // 1. When selective_thinking is true (api_messages_with_thinking) — always
            // 2. When selective_thinking is false but the message HAS signed
            //    thinking blocks — as a safety net to prevent 400 errors if
            //    api_messages() is ever called on a conversation that still
            //    contains thinking blocks (e.g. after context truncation
            //    removed the signed blocks from other messages).
            //
            // Anthropic REQUIRES thinking blocks to be echoed back in ALL
            // subsequent requests when extended thinking is enabled.
            //
            // DeepSeek's Anthropic-compatible endpoint (reasoning_effort)
            // does NOT return signatures — its thinking blocks must also be
            // echoed back verbatim.  We therefore include ALL thinking blocks
            // when the message qualifies for echo-back, with or without
            // a signature.
            let msg_has_any_thinking = msg.content.iter().any(|b| matches!(
                b,
                ContentBlock::Thinking { .. }
            ));
            let include_thinking_for_msg = selective_thinking || msg_has_any_thinking;

            let blocks: Vec<serde_json::Value> = msg
                .content
                .iter()
                .filter(|b| {
                    match b {
                        ContentBlock::Thinking { .. } => {
                            include_thinking_for_msg
                        }
                        _ => true,
                    }
                })
                .filter_map(|b| serde_json::to_value(b).ok())
                .collect();

            // Skip messages whose content blocks all failed to serialize.
            if blocks.is_empty() {
                continue;
            }

            // Try to merge with the previous message if roles match
            if let Some(last) = merged.last_mut() {
                if last.get("role").and_then(|r| r.as_str()) == Some(role_str) {
                    // Same role — append content blocks to existing message
                    if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                        arr.extend(blocks);
                        continue;
                    }
                }
            }

            // Different role or first message — create a new entry
            merged.push(serde_json::json!({
                "role": role_str,
                "content": blocks,
            }));
        }

        merged
    }

    /// Append a typed event to the session log, stamping seq/time.
    pub fn record(&mut self, kind: SessionEventKind) {
        let seq = self.log.len() as u64;
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.log.push(SessionEvent { seq, time, kind });
    }

    /// Record a turn boundary (log-only).
    pub fn append_turn_start(&mut self, turn: u64) {
        self.record(SessionEventKind::TurnStart { turn });
    }
    pub fn append_turn_end(&mut self, turn: u64) {
        self.record(SessionEventKind::TurnEnd { turn });
    }
    pub fn append_step_start(&mut self, turn: u64, step: u64) {
        self.record(SessionEventKind::StepStart { turn, step });
    }
    pub fn append_step_end(&mut self, turn: u64, step: u64) {
        self.record(SessionEventKind::StepEnd { turn, step });
    }

    /// Record a tool call / result pair (log-only; the message blocks already
    /// carry the content, these are supplementary replay fidelity).
    pub fn append_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value) {
        self.record(SessionEventKind::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input: input.clone(),
        });
    }
    pub fn append_tool_result(&mut self, id: &str, is_error: bool) {
        self.record(SessionEventKind::ToolResult {
            id: id.to_string(),
            is_error,
        });
    }

    /// Record a raw assistant text chunk (log-only, token-level replay fidelity).
    /// Reserved for the streaming hook in a later phase.
    #[allow(dead_code)]
    pub fn append_chunk(&mut self, text: &str) {
        self.record(SessionEventKind::AssistantChunk {
            text: text.to_string(),
        });
    }

    /// Record a compaction that shadows message surface positions [surface_start,
    /// surface_end] (inclusive, at compaction time) with one summary message.
    /// Append-only: the log keeps the original messages; derive_messages applies
    /// the shadow when projecting.
    pub fn append_compaction(&mut self, surface_start: usize, surface_end: usize, summary: &str) {
        self.record(SessionEventKind::Compaction {
            surface_start,
            surface_end,
            summary: summary.to_string(),
        });
    }

    /// Build the summary message a compaction substitutes for the shadowed range.
    pub fn compaction_summary_message(summary: &str, count: usize) -> Message {
        Message {
            id: String::new(),
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: format!(
                    "[System: {} earlier messages were removed to fit the context window. Summary of removed conversation:
{}
The conversation continues from the most recent messages below.]",
                    count, summary
                ),
            }],
        }
    }

    /// Re-derive the message surface from the log (applying compaction shadows).
    pub fn rebuild_from_log(&mut self) {
        self.messages = self.derive_messages();
        self.invalidate_token_estimate();
    }

    /// Fork a child conversation from the log prefix ending at `boundary_seq`
    /// (exclusive).
    ///
    /// The child keeps a fresh log (sequenced from 0) and derives its message
    /// surface from that prefix — nothing is shared mutably. Pass a
    /// between-turn `boundary_seq` for a stable branch; use
    /// [Conversation::validate_log] to confirm such a boundary exists.
    pub fn fork(&self, boundary_seq: u64) -> Conversation {
        let boundary = (boundary_seq as usize).min(self.log.len());
        let mut log = self.log[..boundary].to_vec();
        for (i, ev) in log.iter_mut().enumerate() {
            ev.seq = i as u64;
        }
        let mut conv = Conversation::from_log(log);
        conv.system_prompt = self.system_prompt.clone();
        conv
    }

    /// Validate the structural invariants of the session log (replay
    /// correctness): contiguous sequence numbers, turn/step nesting, and
    /// tool-call/result pairing. Returns a list of problems; empty means the
    /// log is structurally healthy and can be replayed/forked safely.
    #[cfg(test)]
    pub fn validate_log(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut problems = Vec::new();

        for (i, ev) in self.log.iter().enumerate() {
            if ev.seq != i as u64 {
                problems.push(format!(
                    "seq gap at log position {} (expected {}, got {})",
                    i, i, ev.seq
                ));
            }
        }

        let mut open_turn: Option<u64> = None;
        let mut open_step: Option<(u64, u64)> = None;
        let mut tool_calls: HashSet<String> = HashSet::new();
        let mut tool_results: HashSet<String> = HashSet::new();

        for ev in &self.log {
            match &ev.kind {
                SessionEventKind::TurnStart { turn } => {
                    if open_turn.is_some() {
                        problems.push(format!("TurnStart {turn} before a TurnEnd"));
                    }
                    open_turn = Some(*turn);
                }
                SessionEventKind::TurnEnd { turn } => {
                    if open_step.is_some() {
                        problems.push(format!("TurnEnd {turn} with an open step"));
                    }
                    if open_turn != Some(*turn) {
                        problems.push(format!("TurnEnd {turn} mismatched with open turn"));
                    }
                    open_turn = None;
                    open_step = None;
                }
                SessionEventKind::StepStart { turn, step } => {
                    if open_turn != Some(*turn) {
                        problems.push(format!("StepStart {turn}/{step} outside its turn"));
                    }
                    if open_step.is_some() {
                        problems.push(format!("StepStart {turn}/{step} with an open step"));
                    }
                    open_step = Some((*turn, *step));
                }
                SessionEventKind::StepEnd { turn, step } => {
                    if open_step != Some((*turn, *step)) {
                        problems.push(format!("StepEnd {turn}/{step} mismatched"));
                    }
                    open_step = None;
                }
                SessionEventKind::ToolCall { id, .. } => {
                    tool_calls.insert(id.clone());
                }
                SessionEventKind::ToolResult { id, .. } => {
                    tool_results.insert(id.clone());
                }
                _ => {}
            }
        }
        if open_turn.is_some() {
            problems.push("log ends with an open turn".to_string());
        }
        if open_step.is_some() {
            problems.push("log ends with an open step".to_string());
        }
        for id in &tool_calls {
            if !tool_results.contains(id) {
                problems.push(format!("ToolCall {id} without a ToolResult"));
            }
        }
        for id in &tool_results {
            if !tool_calls.contains(id) {
                problems.push(format!("ToolResult {id} without a ToolCall"));
            }
        }
        problems
    }

    /// Re-sync `log` from the current `messages` after an in-place structural
    /// rewrite (truncation or tool-pair repair) that changed content or count,
    /// so both agree. Lossy for turn/tool/chunk event continuity over the
    /// rewritten region; a Phase 3 compaction-marker model replaces this.
    pub fn sync_log_from_messages(&mut self) {
        self.log.clear();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for (i, m) in self.messages.iter().enumerate() {
            self.log.push(SessionEvent {
                seq: i as u64,
                time: now,
                kind: SessionEventKind::from_message(m),
            });
        }
    }

    /// The live, append-only session log (the durable, replayable record).
    pub fn to_log(&self) -> Vec<SessionEvent> {
        self.log.clone()
    }

    /// Rebuild a conversation from a persisted session log.
    ///
    /// Message events are replayed in `seq` order to reconstruct the message
    /// history; the system prompt is not part of the log (callers set it
    /// separately, see the persistence restore path).
    pub fn from_log(log: Vec<SessionEvent>) -> Self {
        let messages = log
            .iter()
            .filter_map(|e| e.kind.message().cloned())
            .collect::<Vec<_>>();
        Conversation {
            messages,
            system_prompt: String::new(),
            log,
            delegation_depth: 0,
            token_estimate_cache: std::sync::Mutex::new(TokenEstimateCache::default()),
        }
    }

    /// Project the model-visible message history from the session log.
    ///
    /// This is the "model-visible equals logged" boundary: it reconstructs
    /// exactly the messages recorded in the log, so a caller can assert that
    /// what it sends to the model is what it logged.
    pub fn derive_messages(&self) -> Vec<Message> {
        // Fold the log into the message surface, applying each Compaction
        // shadow (positional replace) in log order. The surface is a pure
        // function of the log — the "model-visible equals logged" boundary.
        let mut surface: Vec<Message> = Vec::new();
        for ev in &self.log {
            if let Some(msg) = ev.kind.message() {
                surface.push(msg.clone());
            } else if let SessionEventKind::Compaction { surface_start, surface_end, summary } = &ev.kind {
                let start = *surface_start;
                let end = (*surface_end).min(surface.len().saturating_sub(1));
                if end >= start && start < surface.len() {
                    let count = end - start + 1;
                    let summary_msg = Self::compaction_summary_message(summary, count);
                    surface.splice(start..=end, [summary_msg]);
                }
            }
        }
        surface
    }

    /// Whether the session log reconstructs exactly the current message history.
    ///
    /// The "model-visible equals logged" invariant: every message here must be
    /// recorded in the log, and the log must not invent messages. Call at a
    /// persistence/observability boundary to catch a code path that mutated the
    /// message list without going through the log-aware path.
    pub fn log_is_consistent(&self) -> bool {
        let derived = self.derive_messages();
        if derived.len() != self.messages.len() {
            return false;
        }
        // Compare surface structure (role sequence). Content may differ because
        // in-session content pruning (truncate_large_blocks) is a surface-only
        // transform not yet reflected in the log; the log keeps full fidelity.
        derived
            .iter()
            .zip(self.messages.iter())
            .all(|(a, b)| a.role == b.role)
    }

    /// Incrementally estimate the model token count of the whole conversation.
    ///
    /// The estimate cache is populated lazily: only messages not yet counted are
    /// tokenized, so a steadily-appended conversation costs O(delta) per call
    /// rather than O(total). `count_message` supplies the per-message estimator
    /// so this module owns only the cache, not the tokenizer choice.
    ///
    /// The cache is keyed by (model, message count). A structural change
    /// (truncation, clear, replacement) must call
    /// [Conversation::invalidate_token_estimate], which makes the next call
    /// recompute from scratch. An in-place content edit that keeps the message
    /// count (rare tool-pair repair) may leave a slightly stale figure, which is
    /// acceptable for the truncation-pressure heuristic this estimate feeds.
    pub(crate) fn token_estimate(
        &self,
        model: &str,
        count_message: impl Fn(&Message, &str) -> usize,
    ) -> usize {
        let mut cache = self.token_estimate_cache.lock().unwrap();
        if cache.model != model || cache.counted_len > self.messages.len() {
            *cache = TokenEstimateCache::default();
            cache.model = model.to_string();
        }
        // The system prompt can be appended at runtime (guidance injection), so
        // recompute it fresh rather than caching (it is bounded and cheap).
        let system = crate::context::estimate_tokens_for_model(&self.system_prompt, model);
        let mut tail = cache.tail_tokens;
        for msg in &self.messages[cache.counted_len.min(self.messages.len())..] {
            tail += count_message(msg, model);
        }
        cache.counted_len = self.messages.len();
        cache.tail_tokens = tail;
        system + tail
    }

    /// Drop the incremental token-estimate cache. Call after any structural
    /// change to `messages` (truncation, clear, replacement) so the next
    /// [Conversation::token_estimate] recomputes from scratch.
    pub(crate) fn invalidate_token_estimate(&mut self) {
        *self.token_estimate_cache.lock().unwrap() = TokenEstimateCache::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_round_trip_preserves_surface_and_validates() {
        let mut c = Conversation::with_system_prompt("sys".to_string());
        c.append_turn_start(1);
        c.add_message(Message::user("hello"));
        c.add_message(Message::assistant(vec![ContentBlock::Text {
            text: "hi".to_string(),
        }]));
        c.append_turn_end(1);

        assert!(c.validate_log().is_empty(), "log problems: {:?}", c.validate_log());
        assert!(c.log_is_consistent());

        let restored = Conversation::from_log(c.to_log());
        assert!(restored.validate_log().is_empty());
        assert!(restored.log_is_consistent());

        let before: Vec<Role> = c.messages.iter().map(|m| m.role.clone()).collect();
        let after: Vec<Role> = restored.derive_messages().iter().map(|m| m.role.clone()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn validate_log_flags_tool_call_without_result() {
        let mut c = Conversation::with_system_prompt("sys".to_string());
        c.record(SessionEventKind::ToolCall {
            id: "t1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({}),
        });
        let problems = c.validate_log();
        assert!(
            problems.iter().any(|p| p.contains("without a ToolResult")),
            "expected an unpaired-tool-call problem, got: {problems:?}"
        );
    }

    #[test]
    fn validate_log_flags_seq_gap() {
        let mut c = Conversation::with_system_prompt("sys".to_string());
        c.record(SessionEventKind::TurnStart { turn: 1 });
        c.record(SessionEventKind::TurnEnd { turn: 1 });
        // Corrupt: drop an event so the sequence numbers are no longer contiguous.
        c.log.remove(0);
        let problems = c.validate_log();
        assert!(
            problems.iter().any(|p| p.contains("seq gap")),
            "expected a seq-gap problem, got: {problems:?}"
        );
    }
}
