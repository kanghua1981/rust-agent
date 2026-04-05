//! Time-based memory system with multiple timelines.
//!
//! Inspired by human memory systems:
//! - Working memory: seconds to minutes
//! - Short-term memory: minutes to hours  
//! - Long-term memory: days to years
//! - Semantic memory: facts without time context
//! - Episodic memory: events with time context

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Time-Based Memory Types ─────────────────────────────────────────────

/// Different types of memories based on time scale
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryTimescale {
    /// Working memory: current task, lasts seconds to minutes
    /// (like human working memory, 7±2 items)
    Working,
    
    /// ShortTerm memory: recent events, lasts minutes to hours
    /// (like human short-term memory, detailed but fades)
    ShortTerm,
    
    /// LongTerm memory: important events, lasts days to weeks
    /// (consolidated from short-term, less detail)
    LongTerm,
    
    /// Semantic memory: facts and knowledge, timeless
    /// (organized by topic, not time)
    Semantic,
    
    /// Episodic memory: specific events with time context
    /// ("I remember when...", includes temporal details)
    Episodic,
    
    /// Procedural memory: skills and how-to, improves with practice
    /// (muscle memory for agents)
    Procedural,
}

/// How memory decays over time
#[derive(Debug, Clone, Copy)]
pub enum DecayFunction {
    /// Linear decay: strength = max(0, initial - decay_rate * time)
    Linear { decay_rate: f32 },
    
    /// Exponential decay: strength = initial * exp(-decay_rate * time)
    Exponential { decay_rate: f32 },
    
    /// Step decay: full strength for period, then drops
    Step { period_ms: u64, drop_to: f32 },
    
    /// No decay (for semantic/procedural memory)
    None,
}

/// Memory strength over time
#[derive(Debug, Clone)]
pub struct MemoryStrength {
    pub initial: f32,          // Initial strength (0.0 to 1.0)
    pub current: f32,          // Current strength
    pub decay_function: DecayFunction,
    pub last_accessed: u64,    // Last time this memory was accessed
    pub access_count: u32,     // How many times accessed
}

impl MemoryStrength {
    pub fn new(initial: f32, decay_function: DecayFunction) -> Self {
        Self {
            initial,
            current: initial,
            decay_function,
            last_accessed: current_timestamp(),
            access_count: 1,
        }
    }
    
    /// Update strength based on time elapsed
    pub fn update(&mut self) {
        let now = current_timestamp();
        let elapsed_ms = now.saturating_sub(self.last_accessed);
        let elapsed_seconds = elapsed_ms as f32 / 1000.0;
        
        self.current = match self.decay_function {
            DecayFunction::Linear { decay_rate } => {
                (self.initial - decay_rate * elapsed_seconds).max(0.0)
            }
            DecayFunction::Exponential { decay_rate } => {
                self.initial * (-decay_rate * elapsed_seconds).exp()
            }
            DecayFunction::Step { period_ms, drop_to } => {
                if elapsed_ms > period_ms {
                    drop_to
                } else {
                    self.initial
                }
            }
            DecayFunction::None => self.initial,
        };
        
        self.last_accessed = now;
    }
    
    /// Strengthen memory (when accessed or reinforced)
    pub fn strengthen(&mut self, amount: f32) {
        self.current = (self.current + amount).min(1.0);
        self.access_count += 1;
        self.last_accessed = current_timestamp();
    }
    
    /// Weaken memory (when contradicted or obsolete)
    pub fn weaken(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }
}

// ── Time-Stamped Memory ─────────────────────────────────────────────────

/// A memory with temporal information
#[derive(Debug, Clone)]
pub struct TimedMemory {
    pub id: String,
    pub timescale: MemoryTimescale,
    
    // Temporal information
    pub created_at: u64,           // When memory was created
    pub effective_from: u64,       // When memory becomes relevant
    pub effective_to: Option<u64>, // When memory expires (None = forever)
    pub duration: Option<u64>,     // How long the event lasted
    
    // Content
    pub title: String,
    pub content: String,
    pub category: MemoryCategory,
    
    // Relationships
    pub precedes: Vec<String>,     // IDs of memories this precedes
    pub follows: Vec<String>,      // IDs of memories this follows
    pub related: Vec<String>,      // IDs of related memories
    
    // Metadata
    pub importance: Importance,
    pub confidence: f32,           // 0.0 to 1.0
    pub source: MemorySource,
    pub tags: Vec<String>,
    
    // State
    pub verified: bool,
    pub strength: MemoryStrength,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryCategory {
    UserRequest,      // User asked something
    AgentResponse,    // Agent responded
    ToolExecution,    // Tool was used
    FileOperation,    // File was read/written
    CodeChange,       // Code was modified
    CommandRun,       // Command was executed
    DecisionMade,     // Decision was made
    Learning,         // Something was learned
    Error,            // Error occurred
    Success,          // Success achieved
    Plan,             // Planning activity
    Review,           // Review activity
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Importance {
    Critical,   // Must remember
    High,       // Important
    Medium,     // Good to remember
    Low,        // Can forget if needed
}

#[derive(Debug, Clone)]
pub enum MemorySource {
    User,
    Agent,
    Tool,
    System,
    External,
}

// ── Time-Based Memory Store ─────────────────────────────────────────────

/// Stores memories organized by timescale and time
pub struct TimeMemoryStore {
    // Memories organized by timescale
    working_memories: VecDeque<TimedMemory>,   // Most recent first
    short_term_memories: VecDeque<TimedMemory>, // Time-ordered
    long_term_memories: Vec<TimedMemory>,      // Importance-ordered
    semantic_memories: Vec<TimedMemory>,       // Topic-organized
    episodic_memories: Vec<TimedMemory>,       // Time-ordered with details
    procedural_memories: Vec<TimedMemory>,     // Skill-organized
    
    // Limits for each timescale
    limits: HashMap<MemoryTimescale, usize>,
    
    // Time windows for each timescale
    time_windows: HashMap<MemoryTimescale, u64>,
}

impl TimeMemoryStore {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        limits.insert(MemoryTimescale::Working, 10);    // 7±2 items
        limits.insert(MemoryTimescale::ShortTerm, 100); // Recent events
        limits.insert(MemoryTimescale::LongTerm, 1000); // Important events
        limits.insert(MemoryTimescale::Semantic, 5000); // Facts
        limits.insert(MemoryTimescale::Episodic, 2000); // Detailed events
        limits.insert(MemoryTimescale::Procedural, 1000); // Skills
        
        let mut time_windows = HashMap::new();
        time_windows.insert(MemoryTimescale::Working, 5 * 60 * 1000);    // 5 minutes
        time_windows.insert(MemoryTimescale::ShortTerm, 2 * 60 * 60 * 1000); // 2 hours
        time_windows.insert(MemoryTimescale::LongTerm, 30 * 24 * 60 * 60 * 1000); // 30 days
        
        Self {
            working_memories: VecDeque::new(),
            short_term_memories: VecDeque::new(),
            long_term_memories: Vec::new(),
            semantic_memories: Vec::new(),
            episodic_memories: Vec::new(),
            procedural_memories: Vec::new(),
            limits,
            time_windows,
        }
    }
    
    /// Add a memory to the appropriate timescale(s)
    pub fn add_memory(&mut self, memory: TimedMemory) {
        // Determine which timescales this memory belongs to
        let timescales = self.determine_timescales(&memory);
        
        for timescale in timescales {
            self.add_to_timescale(timescale, memory.clone());
        }
        
        // Apply retention policies
        self.apply_retention();
    }
    
    /// Determine which timescales a memory belongs to
    fn determine_timescales(&self, memory: &TimedMemory) -> Vec<MemoryTimescale> {
        let mut timescales = Vec::new();
        
        // All memories start in working memory
        timescales.push(MemoryTimescale::Working);
        
        // Based on category and importance
        match memory.category {
            MemoryCategory::UserRequest | MemoryCategory::AgentResponse => {
                timescales.push(MemoryTimescale::ShortTerm);
                timescales.push(MemoryTimescale::Episodic);
                
                if memory.importance >= Importance::High {
                    timescales.push(MemoryTimescale::LongTerm);
                }
            }
            MemoryCategory::Learning => {
                timescales.push(MemoryTimescale::Semantic);
                timescales.push(MemoryTimescale::LongTerm);
            }
            MemoryCategory::ToolExecution | MemoryCategory::CodeChange => {
                timescales.push(MemoryTimescale::Procedural);
                timescales.push(MemoryTimescale::ShortTerm);
                
                if memory.verified && memory.confidence > 0.8 {
                    timescales.push(MemoryTimescale::LongTerm);
                }
            }
            MemoryCategory::Success if memory.importance >= Importance::High => {
                timescales.push(MemoryTimescale::LongTerm);
                timescales.push(MemoryTimescale::Episodic);
            }
            MemoryCategory::Error if memory.importance >= Importance::Medium => {
                timescales.push(MemoryTimescale::Procedural); // Learn from errors
                timescales.push(MemoryTimescale::LongTerm);
            }
            _ => {}
        }
        
        // Remove duplicates
        timescales.sort();
        timescales.dedup();
        
        timescales
    }
    
    /// Add memory to a specific timescale
    fn add_to_timescale(&mut self, timescale: MemoryTimescale, memory: TimedMemory) {
        match timescale {
            MemoryTimescale::Working => {
                // Working memory: most recent first, limited capacity
                self.working_memories.push_front(memory);
                if self.working_memories.len() > self.limits[&MemoryTimescale::Working] {
                    self.working_memories.pop_back();
                }
            }
            MemoryTimescale::ShortTerm => {
                // Short-term: time-ordered
                self.short_term_memories.push_back(memory);
            }
            MemoryTimescale::LongTerm => {
                // Long-term: importance-ordered
                self.long_term_memories.push(memory);
                self.long_term_memories.sort_by(|a, b| {
                    b.importance.cmp(&a.importance)
                        .then(b.confidence.partial_cmp(&a.confidence).unwrap())
                });
            }
            MemoryTimescale::Semantic => {
                // Semantic: topic-organized (simplified)
                self.semantic_memories.push(memory);
            }
            MemoryTimescale::Episodic => {
                // Episodic: time-ordered with details
                self.episodic_memories.push(memory);
                self.episodic_memories.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            }
            MemoryTimescale::Procedural => {
                // Procedural: skill-organized
                self.procedural_memories.push(memory);
            }
        }
    }
    
    /// Apply retention policies (remove old/weak memories)
    fn apply_retention(&mut self) {
        let now = current_timestamp();
        
        // Working memory: remove based on time window
        if let Some(window) = self.time_windows.get(&MemoryTimescale::Working) {
            self.working_memories.retain(|m| now - m.created_at <= *window);
        }
        
        // Short-term memory: remove based on time window and limit
        if let Some(window) = self.time_windows.get(&MemoryTimescale::ShortTerm) {
            self.short_term_memories.retain(|m| now - m.created_at <= *window);
        }
        let short_term_limit = self.limits[&MemoryTimescale::ShortTerm];
        while self.short_term_memories.len() > short_term_limit {
            self.short_term_memories.pop_front(); // Remove oldest
        }
        
        // Long-term memory: remove weak memories
        let long_term_limit = self.limits[&MemoryTimescale::LongTerm];
        if self.long_term_memories.len() > long_term_limit {
            // Keep only the strongest memories
            self.long_term_memories.sort_by(|a, b| {
                b.strength.current.partial_cmp(&a.strength.current).unwrap()
                    .then(b.importance.cmp(&a.importance))
            });
            self.long_term_memories.truncate(long_term_limit);
        }
        
        // Update memory strengths
        for memory in self.all_memories_mut() {
            memory.strength.update();
        }
    }
    
    /// Get memories relevant to current context
    pub fn get_context_memories(&self, query: &str, time_context: TimeContext) -> ContextMemories {
        let mut context = ContextMemories::new();
        let now = current_timestamp();
        
        // 1. Working memory (always included)
        context.working = self.working_memories.iter().cloned().collect();
        
        // 2. Recent memories (last N minutes)
        let recent_cutoff = now.saturating_sub(time_context.recent_minutes * 60 * 1000);
        context.recent = self.get_memories_since(MemoryTimescale::ShortTerm, recent_cutoff);
        
        // 3. Relevant semantic memories
        context.semantic = self.get_relevant_semantic(query);
        
        // 4. Relevant procedural memories
        context.procedural = self.get_relevant_procedural(query);
        
        // 5. Important long-term memories
        context.important = self.get_important_memories(time_context.importance_threshold);
        
        context
    }
    
    /// Get memories since a timestamp
    fn get_memories_since(&self, timescale: MemoryTimescale, since: u64) -> Vec<TimedMemory> {
        let filter_fn = |m: &TimedMemory| m.created_at >= since;
        match timescale {
            MemoryTimescale::ShortTerm => self.short_term_memories.iter().filter(|m| filter_fn(m)).cloned().collect(),
            MemoryTimescale::Episodic => self.episodic_memories.iter().filter(|m| filter_fn(m)).cloned().collect(),
            _ => Vec::new(),
        }
    }
    
    /// Get relevant semantic memories
    fn get_relevant_semantic(&self, query: &str) -> Vec<TimedMemory> {
        let query_lower = query.to_lowercase();
        
        self.semantic_memories.iter()
            .filter(|m| {
                m.title.to_lowercase().contains(&query_lower) ||
                m.content.to_lowercase().contains(&query_lower) ||
                m.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .take(5) // Limit results
            .cloned()
            .collect()
    }
    
    /// Get relevant procedural memories
    fn get_relevant_procedural(&self, query: &str) -> Vec<TimedMemory> {
        let query_lower = query.to_lowercase();
        
        self.procedural_memories.iter()
            .filter(|m| m.category == MemoryCategory::ToolExecution || 
                       m.category == MemoryCategory::CodeChange)
            .filter(|m| m.verified && m.confidence > 0.7)
            .filter(|m| {
                m.title.to_lowercase().contains(&query_lower) ||
                m.content.to_lowercase().contains(&query_lower)
            })
            .take(3) // Limit results
            .cloned()
            .collect()
    }
    
    /// Get important memories
    fn get_important_memories(&self, threshold: f32) -> Vec<TimedMemory> {
        self.long_term_memories.iter()
            .filter(|m| m.strength.current >= threshold)
            .take(5) // Limit results
            .cloned()
            .collect()
    }
    
    /// Get all memories (for iteration)
    fn all_memories_mut(&mut self) -> impl Iterator<Item = &mut TimedMemory> {
        self.working_memories.iter_mut()
            .chain(self.short_term_memories.iter_mut())
            .chain(self.long_term_memories.iter_mut())
            .chain(self.semantic_memories.iter_mut())
            .chain(self.episodic_memories.iter_mut())
            .chain(self.procedural_memories.iter_mut())
    }
    
    /// Consolidate memories (move from short-term to long-term)
    pub fn consolidate(&mut self) {
        let now = current_timestamp();
        
        // Find short-term memories ready for consolidation
        let to_consolidate: Vec<_> = self.short_term_memories.iter()
            .filter(|m| {
                // Consolidate if: important, verified, and old enough
                m.importance >= Importance::Medium &&
                m.verified &&
                (now - m.created_at) > 30 * 60 * 1000 // 30 minutes
            })
            .cloned()
            .collect();
        
        for memory in to_consolidate {
            // Remove from short-term
            self.short_term_memories.retain(|m| m.id != memory.id);
            
            // Add to long-term with adjusted strength
            let mut long_term_memory = memory;
            long_term_memory.strength.strengthen(0.1); // Consolidation strengthens
            self.long_term_memories.push(long_term_memory);
        }
        
        // Sort long-term memories
        self.long_term_memories.sort_by(|a, b| {
            b.strength.current.partial_cmp(&a.strength.current).unwrap()
                .then(b.importance.cmp(&a.importance))
        });
    }
}

// ── Context Structures ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TimeContext {
    pub current_time: u64,
    pub recent_minutes: u64,      // How many minutes back is "recent"
    pub importance_threshold: f32, // Minimum importance to include
    pub max_results_per_type: usize,
}

impl Default for TimeContext {
    fn default() -> Self {
        Self {
            current_time: current_timestamp(),
            recent_minutes: 30,    // Last 30 minutes
            importance_threshold: 0.6,
            max_results_per_type: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextMemories {
    pub working: Vec<TimedMemory>,   // Current working memory
    pub recent: Vec<TimedMemory>,    // Recent events
    pub semantic: Vec<TimedMemory>,  // Relevant facts
    pub procedural: Vec<TimedMemory>, // Relevant how-to
    pub important: Vec<TimedMemory>, // Important long-term
}

impl ContextMemories {
    pub fn new() -> Self {
        Self {
            working: Vec::new(),
            recent: Vec::new(),
            semantic: Vec::new(),
            procedural: Vec::new(),
            important: Vec::new(),
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.working.is_empty() &&
        self.recent.is_empty() &&
        self.semantic.is_empty() &&
        self.procedural.is_empty() &&
        self.important.is_empty()
    }
    
    pub fn to_context_string(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        
        let mut context = String::from("--- Time-Based Memory Context ---\n\n");
        
        if !self.working.is_empty() {
            context.push_str("## Current Working Memory\n");
            for memory in &self.working {
                context.push_str(&format!("• {}: {}\n", memory.title, memory.content));
            }
            context.push_str("\n");
        }
        
        if !self.recent.is_empty() {
            context.push_str("## Recent Activity\n");
            for memory in &self.recent {
                let time_ago = format_duration(current_timestamp() - memory.created_at);
                context.push_str(&format!("• [{} ago] {}: {}\n", 
                    time_ago, memory.title, memory.content));
            }
            context.push_str("\n");
        }
        
        if !self.semantic.is_empty() {
            context.push_str("## Relevant Knowledge\n");
            for memory in &self.semantic {
                context.push_str(&format!("• {}: {}\n", memory.title, memory.content));
                if memory.verified {
                    context.push_str("  (Verified)\n");
                }
            }
            context.push_str("\n");
        }
        
        if !self.procedural.is_empty() {
            context.push_str("## Relevant Procedures\n");
            for memory in &self.procedural {
                context.push_str(&format!("• {}: {}\n", memory.title, memory.content));
                context.push_str(&format!("  Confidence: {:.0}%, Used {} times\n", 
                    memory.confidence * 100.0, memory.strength.access_count));
            }
            context.push_str("\n");
        }
        
        if !self.important.is_empty() {
            context.push_str("## Important Memories\n");
            for memory in &self.important {
                context.push_str(&format!("• {}: {}\n", memory.title, memory.content));
                context.push_str(&format!("  Strength: {:.0}%, Importance: {:?}\n", 
                    memory.strength.current * 100.0, memory.importance));
            }
        }
        
        context
    }
}

// ── Utility Functions ───────────────────────────────────────────────────

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60 * 1000 {
        format!("{:.1}s", ms as f32 / 1000.0)
    } else if ms < 60 * 60 * 1000 {
        format!("{:.1}m", ms as f32 / (60.0 * 1000.0))
    } else if ms < 24 * 60 * 60 * 1000 {
        format!("{:.1}h", ms as f32 / (60.0 * 60.0 * 1000.0))
    } else {
        format!("{:.1}d", ms as f32 / (24.0 * 60.0 * 60.0 * 1000.0))
    }
}

// ── Integration Helper ──────────────────────────────────────────────────

/// Helper for integrating time-based memory with agent
pub struct TimeMemoryHelper {
    store: Arc<RwLock<TimeMemoryStore>>,
}

impl TimeMemoryHelper {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(TimeMemoryStore::new())),
        }
    }
    
    /// Record a user interaction
    pub async fn record_interaction(&self, user_prompt: &str, response: &str) {
        let memory = TimedMemory {
            id: uuid::Uuid::new_v4().to_string(),
            timescale: MemoryTimescale::Working, // Will be adjusted
            created_at: current_timestamp(),
            effective_from: current_timestamp(),
            effective_to: None,
            duration: None,
            title: format!("User: {}", user_prompt.chars().take(50).collect::<String>()),
            content: format!("User asked: {}\nResponse: {}", user_prompt, response),
            category: MemoryCategory::UserRequest,
            precedes: Vec::new(),
            follows: Vec::new(),
            related: Vec::new(),
            importance: Importance::Medium,
            confidence: 1.0,
            source: MemorySource::User,
            tags: vec!["interaction".to_string()],
            verified: true,
            strength: MemoryStrength::new(0.7, DecayFunction::Exponential { decay_rate: 0.001 }),
        };
        
        let mut store = self.store.write().await;
        store.add_memory(memory);
    }
    
    /// Record a tool execution
    pub async fn record_tool(&self, tool: &str, target: &str, success: bool) {
        let memory = TimedMemory {
            id: uuid::Uuid::new_v4().to_string(),
            timescale: MemoryTimescale::Working,
            created_at: current_timestamp(),
            effective_from: current_timestamp(),
            effective_to: None,
            duration: None,
            title: format!("Tool: {} {}", tool, target),
            content: format!("Executed {} on {}", tool, target),
            category: MemoryCategory::ToolExecution,
            precedes: Vec::new(),
            follows: Vec::new(),
            related: Vec::new(),
            importance: if success { Importance::Medium } else { Importance::High },
            confidence: 1.0,
            source: MemorySource::Tool,
            tags: vec![tool.to_string()],
            verified: true,
            strength: MemoryStrength::new(
                if success { 0.6 } else { 0.8 }, // Remember errors more strongly
                DecayFunction::Exponential { decay_rate: 0.0005 }
            ),
        };
        
        let mut store = self.store.write().await;
        store.add_memory(memory);
    }
    
    /// Record a learning point
    pub async fn record_learning(&self, topic: &str, fact: &str, importance: Importance) {
        let memory = TimedMemory {
            id: uuid::Uuid::new_v4().to_string(),
            timescale: MemoryTimescale::Working,
            created_at: current_timestamp(),
            effective_from: current_timestamp(),
            effective_to: None,
            duration: None,
            title: format!("Learned: {}", topic),
            content: fact.to_string(),
            category: MemoryCategory::Learning,
            precedes: Vec::new(),
            follows: Vec::new(),
            related: Vec::new(),
            importance,
            confidence: 0.8,
            source: MemorySource::Agent,
            tags: vec!["knowledge".to_string(), "learning".to_string()],
            verified: false, // Needs verification
            strength: MemoryStrength::new(0.5, DecayFunction::None), // Semantic memory doesn't decay
        };
        
        let mut store = self.store.write().await;
        store.add_memory(memory);
    }
    
    /// Get context for current task
    pub async fn get_context(&self, query: &str) -> String {
        let context = TimeContext::default();
        let store = self.store.read().await;
        
        let memories = store.get_context_memories(query, context);
        memories.to_context_string()
    }
    
    /// Consolidate memories (call periodically)
    pub async fn consolidate(&self) {
        let mut store = self.store.write().await;
        store.consolidate();
    }
    
    /// Get memory statistics
    pub async fn get_stats(&self) -> HashMap<String, usize> {
        let store = self.store.read().await;
        
        let mut stats = HashMap::new();
        stats.insert("working".to_string(), store.working_memories.len());
        stats.insert("short_term".to_string(), store.short_term_memories.len());
        stats.insert("long_term".to_string(), store.long_term_memories.len());
        stats.insert("semantic".to_string(), store.semantic_memories.len());
        stats.insert("episodic".to_string(), store.episodic_memories.len());
        stats.insert("procedural".to_string(), store.procedural_memories.len());
        
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_strength() {
        let mut strength = MemoryStrength::new(1.0, DecayFunction::Linear { decay_rate: 0.1 });
        
        // Simulate 5 seconds passing
        std::thread::sleep(std::time::Duration::from_millis(100));
        strength.update();
        
        assert!(strength.current < 1.0);
    }
    
    #[tokio::test]
    async fn test_time_memory() {
        let helper = TimeMemoryHelper::new();
        
        // Record some interactions
        helper.record_interaction("Hello", "Hi there!").await;
        helper.record_tool("read_file", "main.rs", true).await;
        helper.record_learning("Rust", "Rust has ownership system", Importance::High).await;
        
        // Get context
        let context = helper.get_context("Rust").await;
        assert!(!context.is_empty());
        assert!(context.contains("Time-Based Memory Context"));
        
        // Get stats
        let stats = helper.get_stats().await;
        assert!(stats["working"] > 0);
    }
}