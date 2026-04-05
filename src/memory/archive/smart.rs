//! Smart memory system that focuses on recording what actually improves LLM performance.
//!
//! Records: User prompts, LLM reasoning, outcomes, and learned patterns.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Core Smart Memory Types ──────────────────────────────────────────────

/// A smart memory entry that captures what matters
#[derive(Debug, Clone)]
pub struct SmartMemory {
    pub id: String,
    pub timestamp: u64,
    
    // What the user asked
    pub user_prompt: String,
    
    // What we understood
    pub intent_summary: String,
    
    // What we did (tools used, files touched)
    pub actions_taken: Vec<Action>,
    
    // What happened (success/failure)
    pub outcome: Outcome,
    
    // What we learned
    pub lessons: Vec<String>,
    
    // User feedback if any
    pub user_feedback: Option<String>,
    
    // Category for organization
    pub category: MemoryCategory,
    
    // Relevance score (calculated on recall)
    pub relevance_score: f32,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub tool: String,
    pub target: String, // file path, command, etc.
    pub purpose: String, // why this action was taken
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Success,
    PartialSuccess { achieved: String, missed: String },
    Failure { reason: String },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryCategory {
    CodeWriting,
    CodeModification,
    CodeReview,
    FileOperation,
    CommandExecution,
    InformationQuery,
    Debugging,
    Planning,
    Learning,
}

// ── Smart Memory Store ───────────────────────────────────────────────────

pub struct SmartMemoryStore {
    memories: VecDeque<SmartMemory>, // Most recent first
    max_memories: usize,
    
    // Indexes for fast lookup
    by_category: HashMap<MemoryCategory, Vec<usize>>,
    by_keyword: HashMap<String, Vec<usize>>,
}

impl SmartMemoryStore {
    pub fn new(max_memories: usize) -> Self {
        Self {
            memories: VecDeque::with_capacity(max_memories),
            max_memories,
            by_category: HashMap::new(),
            by_keyword: HashMap::new(),
        }
    }
    
    /// Add a new smart memory
    pub fn add_memory(&mut self, memory: SmartMemory) {
        // Remove oldest if at capacity
        if self.memories.len() >= self.max_memories {
            if let Some(removed) = self.memories.pop_back() {
                self.remove_from_indexes(&removed, self.memories.len());
            }
        }
        
        let index = self.memories.len();
        self.memories.push_front(memory.clone());
        
        // Add to indexes
        self.add_to_indexes(&memory, 0);
    }
    
    /// Find relevant memories for a query
    pub fn find_relevant(&self, query: &str, category: Option<MemoryCategory>) -> Vec<SmartMemory> {
        let mut candidates = Vec::new();
        
        // First, check by category if specified
        if let Some(cat) = category {
            if let Some(indices) = self.by_category.get(&cat) {
                for &idx in indices {
                    if idx < self.memories.len() {
                        candidates.push((idx, &self.memories[idx]));
                    }
                }
            }
        }
        
        // If no category or not enough results, check by keyword
        if candidates.len() < 3 {
            let keywords = extract_keywords(query);
            for keyword in keywords {
                if let Some(indices) = self.by_keyword.get(&keyword) {
                    for &idx in indices {
                        if idx < self.memories.len() {
                            candidates.push((idx, &self.memories[idx]));
                        }
                    }
                }
            }
        }
        
        // Calculate relevance scores
        let mut scored: Vec<(f32, SmartMemory)> = candidates
            .into_iter()
            .map(|(idx, memory)| {
                let score = calculate_relevance(memory, query, idx);
                (score, memory.clone())
            })
            .collect();
        
        // Sort by relevance (highest first)
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        
        // Return top 5 most relevant
        scored.into_iter()
            .take(5)
            .map(|(_, memory)| memory)
            .collect()
    }
    
    /// Extract lessons learned from memories
    pub fn extract_lessons(&self, min_confidence: f32) -> Vec<String> {
        let mut lessons = HashMap::new();
        
        for memory in &self.memories {
            for lesson in &memory.lessons {
                if memory.outcome == Outcome::Success {
                    let entry = lessons.entry(lesson.clone()).or_insert((0, 0));
                    entry.0 += 1; // Success count
                    entry.1 += 1; // Total count
                } else {
                    let entry = lessons.entry(lesson.clone()).or_insert((0, 0));
                    entry.1 += 1; // Total count only
                }
            }
        }
        
        lessons.into_iter()
            .filter(|(_, (success, total))| {
                *total >= 2 && (*success as f32 / *total as f32) >= min_confidence
            })
            .map(|(lesson, _)| lesson)
            .collect()
    }
    
    /// Get user preferences based on past interactions
    pub fn get_user_preferences(&self) -> Vec<String> {
        let mut preferences = HashMap::new();
        
        for memory in &self.memories {
            // Look for patterns in user feedback
            if let Some(feedback) = &memory.user_feedback {
                if feedback.contains("like") || feedback.contains("prefer") || feedback.contains("good") {
                    // Extract preference from context
                    if let Some(pref) = extract_preference(&memory.user_prompt, feedback) {
                        *preferences.entry(pref).or_insert(0) += 1;
                    }
                }
            }
            
            // Look for patterns in what worked well
            if memory.outcome == Outcome::Success && !memory.lessons.is_empty() {
                for lesson in &memory.lessons {
                    if lesson.contains("prefer") || lesson.contains("works better") {
                        *preferences.entry(lesson.clone()).or_insert(0) += 2;
                    }
                }
            }
        }
        
        // Sort by frequency and return
        let mut pref_list: Vec<_> = preferences.into_iter().collect();
        pref_list.sort_by(|a, b| b.1.cmp(&a.1));
        
        pref_list.into_iter()
            .take(5)
            .map(|(pref, _)| pref)
            .collect()
    }
    
    // ── Private Helper Methods ───────────────────────────────────────────
    
    fn add_to_indexes(&mut self, memory: &SmartMemory, index: usize) {
        // Add to category index
        self.by_category
            .entry(memory.category.clone())
            .or_insert_with(Vec::new)
            .push(index);
        
        // Add to keyword index
        let text = format!("{} {}", memory.user_prompt, memory.intent_summary);
        let keywords = extract_keywords(&text);
        
        for keyword in keywords {
            self.by_keyword
                .entry(keyword)
                .or_insert_with(Vec::new)
                .push(index);
        }
    }
    
    fn remove_from_indexes(&mut self, memory: &SmartMemory, old_index: usize) {
        // Note: This is simplified - in production would need proper index management
        // For now, we'll just rebuild indexes occasionally or use a more robust approach
    }
}

// ── Utility Functions ────────────────────────────────────────────────────

fn extract_keywords(text: &str) -> Vec<String> {
    let stop_words: [&str; 49] = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to",
        "for", "of", "with", "by", "as", "is", "are", "was", "were",
        "be", "been", "being", "have", "has", "had", "do", "does",
        "did", "will", "would", "should", "could", "can", "may",
        "might", "must", "shall", "this", "that", "these", "those",
        "i", "you", "he", "she", "it", "we", "they", "me",
    ];
    
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 3 && !stop_words.contains(word))
        .map(|s| s.to_string())
        .collect()
}

fn calculate_relevance(memory: &SmartMemory, query: &str, recency_index: usize) -> f32 {
    let mut score = 0.0;
    
    // 1. Keyword overlap (40%)
    let query_keywords: std::collections::HashSet<_> = extract_keywords(query).into_iter().collect();
    let memory_keywords: std::collections::HashSet<_> = 
        extract_keywords(&format!("{} {}", memory.user_prompt, memory.intent_summary))
        .into_iter().collect();
    
    if !query_keywords.is_empty() {
        let overlap = query_keywords.intersection(&memory_keywords).count() as f32;
        score += (overlap / query_keywords.len() as f32) * 0.4;
    }
    
    // 2. Outcome success (30%)
    match memory.outcome {
        Outcome::Success => score += 0.3,
        Outcome::PartialSuccess { .. } => score += 0.15,
        Outcome::Failure { .. } => score += 0.05,
        Outcome::Unknown => score += 0.1,
    }
    
    // 3. Recency (20%)
    let recency_factor = 1.0 - (recency_index as f32 / 100.0).min(1.0);
    score += recency_factor * 0.2;
    
    // 4. User feedback positive (10%)
    if let Some(feedback) = &memory.user_feedback {
        if feedback.contains("thanks") || feedback.contains("good") || feedback.contains("perfect") {
            score += 0.1;
        } else if feedback.contains("not") || feedback.contains("wrong") || feedback.contains("bad") {
            score -= 0.05;
        }
    }
    
    score.min(1.0).max(0.0)
}

fn extract_preference(prompt: &str, feedback: &str) -> Option<String> {
    // Simple preference extraction
    if feedback.contains("detailed") || feedback.contains("explain more") {
        Some("Prefers detailed explanations".to_string())
    } else if feedback.contains("concise") || feedback.contains("brief") {
        Some("Prefers concise responses".to_string())
    } else if feedback.contains("example") || feedback.contains("show me") {
        Some("Prefers examples in responses".to_string())
    } else if feedback.contains("test") || feedback.contains("verify") {
        Some("Prefers including tests".to_string())
    } else if feedback.contains("comment") || feedback.contains("document") {
        Some("Prefers well-documented code".to_string())
    } else {
        None
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Integration Helper ───────────────────────────────────────────────────

/// Helper to create smart memories from agent interactions
pub struct SmartMemoryRecorder {
    store: Arc<RwLock<SmartMemoryStore>>,
}

impl SmartMemoryRecorder {
    pub fn new(max_memories: usize) -> Self {
        Self {
            store: Arc::new(RwLock::new(SmartMemoryStore::new(max_memories))),
        }
    }
    
    /// Record a complete interaction
    pub async fn record_interaction(
        &self,
        user_prompt: &str,
        intent_summary: &str,
        actions: Vec<Action>,
        outcome: Outcome,
        lessons: Vec<String>,
        user_feedback: Option<String>,
    ) {
        let category = Self::categorize_prompt(user_prompt);
        
        let memory = SmartMemory {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: current_timestamp(),
            user_prompt: user_prompt.to_string(),
            intent_summary: intent_summary.to_string(),
            actions_taken: actions,
            outcome,
            lessons,
            user_feedback,
            category,
            relevance_score: 0.0,
        };
        
        let mut store = self.store.write().await;
        store.add_memory(memory);
    }
    
    /// Get relevant context for a new query
    pub async fn get_context(&self, query: &str) -> String {
        let category = Self::categorize_prompt(query);
        let store = self.store.read().await;
        
        let relevant = store.find_relevant(query, Some(category));
        
        if relevant.is_empty() {
            return String::new();
        }
        
        let mut context = String::from("--- Smart Memory Context ---\n\n");
        
        for (i, memory) in relevant.iter().enumerate() {
            context.push_str(&format!("{}. User asked: \"{}\"\n", i + 1, memory.user_prompt));
            context.push_str(&format!("   We understood: {}\n", memory.intent_summary));
            
            if !memory.actions_taken.is_empty() {
                context.push_str(&format!("   Actions: {}\n", 
                    memory.actions_taken.iter()
                        .map(|a| format!("{} {}", a.tool, a.target))
                        .collect::<Vec<_>>()
                        .join(", ")));
            }
            
            match &memory.outcome {
                Outcome::Success => context.push_str("   Result: Success\n"),
                Outcome::PartialSuccess { achieved, missed } => 
                    context.push_str(&format!("   Result: Partial (achieved: {}, missed: {})\n", achieved, missed)),
                Outcome::Failure { reason } => 
                    context.push_str(&format!("   Result: Failed ({})\n", reason)),
                Outcome::Unknown => context.push_str("   Result: Unknown\n"),
            }
            
            if let Some(feedback) = &memory.user_feedback {
                context.push_str(&format!("   User feedback: {}\n", feedback));
            }
            
            if !memory.lessons.is_empty() {
                context.push_str(&format!("   Lessons: {}\n", memory.lessons.join(", ")));
            }
            
            context.push_str("\n");
        }
        
        // Add learned lessons if any
        let lessons = store.extract_lessons(0.7);
        if !lessons.is_empty() {
            context.push_str("## Learned Lessons\n");
            for lesson in lessons.iter().take(3) {
                context.push_str(&format!("• {}\n", lesson));
            }
            context.push_str("\n");
        }
        
        // Add user preferences if any
        let preferences = store.get_user_preferences();
        if !preferences.is_empty() {
            context.push_str("## User Preferences\n");
            for pref in preferences.iter().take(2) {
                context.push_str(&format!("• {}\n", pref));
            }
        }
        
        context
    }
    
    fn categorize_prompt(prompt: &str) -> MemoryCategory {
        let prompt_lower = prompt.to_lowercase();
        
        if prompt_lower.contains("write") || prompt_lower.contains("create") || prompt_lower.contains("implement") {
            MemoryCategory::CodeWriting
        } else if prompt_lower.contains("modify") || prompt_lower.contains("edit") || prompt_lower.contains("change") {
            MemoryCategory::CodeModification
        } else if prompt_lower.contains("review") || prompt_lower.contains("analyze") {
            MemoryCategory::CodeReview
        } else if prompt_lower.contains("file") || prompt_lower.contains("read") || prompt_lower.contains("write") {
            MemoryCategory::FileOperation
        } else if prompt_lower.contains("run") || prompt_lower.contains("execute") || prompt_lower.contains("command") {
            MemoryCategory::CommandExecution
        } else if prompt_lower.contains("explain") || prompt_lower.contains("what") || prompt_lower.contains("how") {
            MemoryCategory::InformationQuery
        } else if prompt_lower.contains("debug") || prompt_lower.contains("fix") || prompt_lower.contains("error") {
            MemoryCategory::Debugging
        } else if prompt_lower.contains("plan") || prompt_lower.contains("design") {
            MemoryCategory::Planning
        } else {
            MemoryCategory::Learning
        }
    }
}

// ── Example Usage ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_smart_memory() {
        let recorder = SmartMemoryRecorder::new(100);
        
        // Record a successful interaction
        recorder.record_interaction(
            "Write a function to calculate factorial in Rust",
            "Create factorial calculation function with tests",
            vec![
                Action {
                    tool: "write_file".to_string(),
                    target: "factorial.rs".to_string(),
                    purpose: "Implement factorial function".to_string(),
                    success: true,
                }
            ],
            Outcome::Success,
            vec![
                "User prefers recursive implementations".to_string(),
                "Include tests for edge cases".to_string(),
            ],
            Some("Perfect, thanks!".to_string()),
        ).await;
        
        // Get context for similar query
        let context = recorder.get_context("How to write factorial function?").await;
        assert!(!context.is_empty());
        assert!(context.contains("factorial"));
    }
    
    #[test]
    fn test_keyword_extraction() {
        let text = "Write a function to calculate factorial in Rust programming language";
        let keywords = extract_keywords(text);
        
        assert!(keywords.contains(&"write".to_string()));
        assert!(keywords.contains(&"function".to_string()));
        assert!(keywords.contains(&"calculate".to_string()));
        assert!(keywords.contains(&"factorial".to_string()));
        assert!(keywords.contains(&"rust".to_string()));
        assert!(keywords.contains(&"programming".to_string()));
        assert!(keywords.contains(&"language".to_string()));
    }
    
    #[test]
    fn test_relevance_calculation() {
        let memory = SmartMemory {
            id: "test".to_string(),
            timestamp: current_timestamp(),
            user_prompt: "Write factorial function".to_string(),
            intent_summary: "Create factorial calculation".to_string(),
            actions_taken: Vec::new(),
            outcome: Outcome::Success,
            lessons: Vec::new(),
            user_feedback: Some("Good job!".to_string()),
            category: MemoryCategory::CodeWriting,
            relevance_score: 0.0,
        };
        
        let score = calculate_relevance(&memory, "How to write factorial?", 0);
        assert!(score > 0.5);
    }
}