//! Timeline-based memory system.
//!
//! Organizes memories along multiple timelines with different granularities
//! and retention policies, inspired by human memory systems.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Timeline Types ──────────────────────────────────────────────────────

/// Different timelines for different types of memories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimelineType {
    /// Episodic timeline: specific events in chronological order
    /// (lasts hours to days, detailed but fades quickly)
    Episodic,
    
    /// Semantic timeline: facts and knowledge without temporal context
    /// (long-term, organized by topic not time)
    Semantic,
    
    /// Procedural timeline: how-to knowledge and skills
    /// (long-term, improves with practice)
    Procedural,
    
    /// Working timeline: current task context
    /// (short-term, constantly updated)
    Working,
    
    /// Project timeline: project-specific events
    /// (medium-term, organized by project phase)
    Project,
    
    /// User timeline: user interaction patterns
    /// (long-term, learns user preferences)
    User,
}

/// Time granularity for different timelines
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeGranularity {
    Milliseconds,  // For precise event ordering
    Seconds,       // For real-time interactions
    Minutes,       // For task-level events
    Hours,         // For session-level events
    Days,          // For daily patterns
    Weeks,         // For weekly patterns
    Months,        // For monthly patterns
}

/// Memory retention policy
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub max_age: Option<u64>,           // Maximum age in milliseconds (None = keep forever)
    pub max_count: Option<usize>,       // Maximum number of items
    pub compression_after: Option<u64>, // Compress/summarize after this age
    pub importance_threshold: f32,      // Minimum importance to keep long-term
}

// ── Timeline Event ──────────────────────────────────────────────────────

/// An event on a timeline
#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub id: String,
    pub timeline_type: TimelineType,
    
    // Temporal information
    pub timestamp: u64,                 // Absolute timestamp (ms since epoch)
    pub duration: Option<u64>,          // Duration in ms (None for instantaneous)
    pub relative_time: Option<f64>,     // Relative time within context (0.0 to 1.0)
    
    // Content
    pub title: String,
    pub description: String,
    pub category: EventCategory,
    
    // Relationships
    pub parent_id: Option<String>,      // Parent event (e.g., task contains subtasks)
    pub related_ids: Vec<String>,       // Related events
    pub precedes_ids: Vec<String>,      // Events that this precedes
    pub follows_ids: Vec<String>,       // Events that this follows
    
    // Metadata
    pub importance: EventImportance,
    pub confidence: f32,                // 0.0 to 1.0
    pub source: EventSource,
    pub tags: Vec<String>,
    
    // State
    pub completed: bool,
    pub success: Option<bool>,          // None = not applicable/unknown
    pub user_feedback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventCategory {
    UserInteraction,    // User asked something
    ToolExecution,      // Tool was used
    FileOperation,      // File was read/written
    CodeChange,         // Code was modified
    CommandExecution,   // Command was run
    DecisionPoint,      // Decision was made
    LearningPoint,      // Something was learned
    ErrorOccurrence,    // Error happened
    SuccessAchievement, // Success was achieved
    Planning,           // Planning activity
    Review,             // Review activity
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventImportance {
    Critical,   // Must remember, affects core functionality
    High,       // Important for efficiency/quality
    Medium,     // Good to remember
    Low,        // Can forget if needed
    Trivial,    // Can safely forget
}

#[derive(Debug, Clone)]
pub enum EventSource {
    UserInput,
    AgentAction,
    ToolOutput,
    SystemEvent,
    External,
}

// ── Timeline Structure ──────────────────────────────────────────────────

/// A single timeline
pub struct Timeline {
    pub timeline_type: TimelineType,
    pub granularity: TimeGranularity,
    pub retention: RetentionPolicy,
    
    // Events sorted by timestamp
    events: BTreeMap<u64, Vec<TimelineEvent>>,
    
    // Indexes for fast lookup
    by_id: HashMap<String, (u64, usize)>, // timestamp -> index in events[timestamp]
    by_category: HashMap<EventCategory, Vec<String>>,
    by_tag: HashMap<String, Vec<String>>,
}

impl Timeline {
    pub fn new(timeline_type: TimelineType, granularity: TimeGranularity, retention: RetentionPolicy) -> Self {
        Self {
            timeline_type,
            granularity,
            retention,
            events: BTreeMap::new(),
            by_id: HashMap::new(),
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
        }
    }
    
    /// Add an event to the timeline
    pub fn add_event(&mut self, event: TimelineEvent) {
        let timestamp = self.normalize_timestamp(event.timestamp);
        
        // Store event
        let event_list = self.events.entry(timestamp).or_insert_with(Vec::new);
        let index = event_list.len();
        event_list.push(event.clone());
        
        // Update indexes
        self.by_id.insert(event.id.clone(), (timestamp, index));
        
        self.by_category
            .entry(event.category.clone())
            .or_insert_with(Vec::new)
            .push(event.id.clone());
        
        for tag in &event.tags {
            self.by_tag
                .entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(event.id.clone());
        }
        
        // Apply retention policy
        self.apply_retention();
    }
    
    /// Get events in a time range
    pub fn get_events_in_range(&self, start: u64, end: u64) -> Vec<&TimelineEvent> {
        let normalized_start = self.normalize_timestamp(start);
        let normalized_end = self.normalize_timestamp(end);
        
        self.events
            .range(normalized_start..=normalized_end)
            .flat_map(|(_, events)| events.iter())
            .collect()
    }
    
    /// Get events by category
    pub fn get_events_by_category(&self, category: &EventCategory) -> Vec<&TimelineEvent> {
        self.by_category
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.get_event_by_id(id))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get event by ID
    pub fn get_event_by_id(&self, id: &str) -> Option<&TimelineEvent> {
        self.by_id.get(id)
            .and_then(|(timestamp, index)| {
                self.events.get(timestamp)
                    .and_then(|events| events.get(*index))
            })
    }
    
    /// Get timeline summary for a period
    pub fn get_summary(&self, start: u64, end: u64, max_events: usize) -> TimelineSummary {
        let events = self.get_events_in_range(start, end);
        
        // Group by category for summary
        let mut by_category: HashMap<EventCategory, usize> = HashMap::new();
        let mut important_events = Vec::new();
        
        for event in &events {
            *by_category.entry(event.category.clone()).or_insert(0) += 1;
            
            if event.importance >= EventImportance::High {
                important_events.push((*event).clone());
            }
        }
        
        // Sort important events by importance and recency
        important_events.sort_by(|a, b| {
            b.importance.cmp(&a.importance)
                .then(b.timestamp.cmp(&a.timestamp))
        });
        
        // Take top N
        let top_events = important_events
            .into_iter()
            .take(max_events)
            .collect();
        
        TimelineSummary {
            start,
            end,
            total_events: events.len(),
            events_by_category: by_category,
            top_events,
        }
    }
    
    /// Normalize timestamp based on granularity
    fn normalize_timestamp(&self, timestamp: u64) -> u64 {
        match self.granularity {
            TimeGranularity::Milliseconds => timestamp,
            TimeGranularity::Seconds => timestamp / 1000 * 1000,
            TimeGranularity::Minutes => timestamp / (60 * 1000) * (60 * 1000),
            TimeGranularity::Hours => timestamp / (60 * 60 * 1000) * (60 * 60 * 1000),
            TimeGranularity::Days => timestamp / (24 * 60 * 60 * 1000) * (24 * 60 * 60 * 1000),
            TimeGranularity::Weeks => timestamp / (7 * 24 * 60 * 60 * 1000) * (7 * 24 * 60 * 60 * 1000),
            TimeGranularity::Months => {
                // Simplified: 30 days per month
                timestamp / (30 * 24 * 60 * 60 * 1000) * (30 * 24 * 60 * 60 * 1000)
            }
        }
    }
    
    /// Apply retention policy (remove old/unimportant events)
    fn apply_retention(&mut self) {
        let now = current_timestamp();
        
        // Apply max age
        if let Some(max_age) = self.retention.max_age {
            let cutoff = now.saturating_sub(max_age);
            self.events.retain(|&timestamp, _| timestamp >= cutoff);
        }
        
        // Apply max count
        if let Some(max_count) = self.retention.max_count {
            let total: usize = self.events.values().map(|v| v.len()).sum();
            if total > max_count {
                self.remove_oldest_events(total - max_count);
            }
        }
        
        // Rebuild indexes after removal
        self.rebuild_indexes();
    }
    
    fn remove_oldest_events(&mut self, count: usize) {
        let mut removed = 0;
        let mut to_remove = Vec::new();
        
        for (&timestamp, events) in &self.events {
            for (index, event) in events.iter().enumerate() {
                if removed >= count {
                    break;
                }
                
                // Remove low importance events first
                if event.importance <= EventImportance::Low {
                    to_remove.push((timestamp, index, event.id.clone()));
                    removed += 1;
                }
            }
        }
        
        // If still need to remove more, remove medium importance
        if removed < count {
            for (&timestamp, events) in &self.events {
                for (index, event) in events.iter().enumerate() {
                    if removed >= count {
                        break;
                    }
                    
                    if event.importance <= EventImportance::Medium {
                        // Check if not already marked for removal
                        if !to_remove.iter().any(|(t, i, _)| *t == timestamp && *i == index) {
                            to_remove.push((timestamp, index, event.id.clone()));
                            removed += 1;
                        }
                    }
                }
            }
        }
        
        // Actually remove the events
        for (timestamp, index, id) in to_remove {
            if let Some(events) = self.events.get_mut(&timestamp) {
                if index < events.len() {
                    events.remove(index);
                }
            }
            self.by_id.remove(&id);
        }
        
        // Remove empty timestamps
        self.events.retain(|_, events| !events.is_empty());
    }
    
    fn rebuild_indexes(&mut self) {
        self.by_id.clear();
        self.by_category.clear();
        self.by_tag.clear();
        
        for (timestamp, events) in &self.events {
            for (index, event) in events.iter().enumerate() {
                self.by_id.insert(event.id.clone(), (*timestamp, index));
                
                self.by_category
                    .entry(event.category.clone())
                    .or_insert_with(Vec::new)
                    .push(event.id.clone());
                
                for tag in &event.tags {
                    self.by_tag
                        .entry(tag.clone())
                        .or_insert_with(Vec::new)
                        .push(event.id.clone());
                }
            }
        }
    }
}

// ── Timeline Summary ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TimelineSummary {
    pub start: u64,
    pub end: u64,
    pub total_events: usize,
    pub events_by_category: HashMap<EventCategory, usize>,
    pub top_events: Vec<TimelineEvent>,
}

impl TimelineSummary {
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        
        md.push_str(&format!("## Timeline Summary ({} events)\n\n", self.total_events));
        
        // Events by category
        if !self.events_by_category.is_empty() {
            md.push_str("### Events by Category\n");
            let mut categories: Vec<_> = self.events_by_category.iter().collect();
            categories.sort_by(|a, b| b.1.cmp(a.1));
            
            for (category, count) in categories {
                md.push_str(&format!("- **{:?}**: {} events\n", category, count));
            }
            md.push_str("\n");
        }
        
        // Top events
        if !self.top_events.is_empty() {
            md.push_str("### Key Events\n");
            for event in &self.top_events {
                let time_str = format_timestamp(event.timestamp);
                md.push_str(&format!("- **{}** ({})\n", event.title, time_str));
                md.push_str(&format!("  {}\n", event.description));
                
                if let Some(success) = event.success {
                    md.push_str(&format!("  Outcome: {}\n", if success { "Success" } else { "Failure" }));
                }
                
                if !event.tags.is_empty() {
                    md.push_str(&format!("  Tags: {}\n", event.tags.join(", ")));
                }
                md.push_str("\n");
            }
        }
        
        md
    }
}

// ── Multi-Timeline Manager ──────────────────────────────────────────────

/// Manages multiple timelines
pub struct TimelineManager {
    timelines: HashMap<TimelineType, Timeline>,
    
    // Cross-timeline indexes
    event_to_timeline: HashMap<String, TimelineType>,
    
    // Current context
    current_session_id: String,
    session_start_time: u64,
}

impl TimelineManager {
    pub fn new() -> Self {
        let mut manager = Self {
            timelines: HashMap::new(),
            event_to_timeline: HashMap::new(),
            current_session_id: uuid::Uuid::new_v4().to_string(),
            session_start_time: current_timestamp(),
        };
        
        // Initialize default timelines
        manager.initialize_default_timelines();
        
        manager
    }
    
    fn initialize_default_timelines(&mut self) {
        // Episodic timeline: detailed events, short retention
        self.timelines.insert(TimelineType::Episodic, Timeline::new(
            TimelineType::Episodic,
            TimeGranularity::Seconds,
            RetentionPolicy {
                max_age: Some(7 * 24 * 60 * 60 * 1000), // 7 days
                max_count: Some(1000),
                compression_after: Some(24 * 60 * 60 * 1000), // Compress after 1 day
                importance_threshold: 0.3,
            }
        ));
        
        // Semantic timeline: facts and knowledge
        self.timelines.insert(TimelineType::Semantic, Timeline::new(
            TimelineType::Semantic,
            TimeGranularity::Days,
            RetentionPolicy {
                max_age: None, // Keep forever
                max_count: Some(5000),
                compression_after: None, // Don't compress
                importance_threshold: 0.5,
            }
        ));
        
        // Procedural timeline: skills and how-to
        self.timelines.insert(TimelineType::Procedural, Timeline::new(
            TimelineType::Procedural,
            TimeGranularity::Days,
            RetentionPolicy {
                max_age: None, // Keep forever
                max_count: Some(2000),
                compression_after: None,
                importance_threshold: 0.4,
            }
        ));
        
        // Working timeline: current task
        self.timelines.insert(TimelineType::Working, Timeline::new(
            TimelineType::Working,
            TimeGranularity::Milliseconds,
            RetentionPolicy {
                max_age: Some(2 * 60 * 60 * 1000), // 2 hours
                max_count: Some(100),
                compression_after: Some(30 * 60 * 1000), // Compress after 30 minutes
                importance_threshold: 0.2,
            }
        ));
        
        // Project timeline: project events
        self.timelines.insert(TimelineType::Project, Timeline::new(
            TimelineType::Project,
            TimeGranularity::Hours,
            RetentionPolicy {
                max_age: Some(90 * 24 * 60 * 60 * 1000), // 90 days
                max_count: Some(2000),
                compression_after: Some(7 * 24 * 60 * 60 * 1000), // Compress after 1 week
                importance_threshold: 0.3,
            }
        ));
        
        // User timeline: user interactions
        self.timelines.insert(TimelineType::User, Timeline::new(
            TimelineType::User,
            TimeGranularity::Days,
            RetentionPolicy {
                max_age: None, // Keep forever
                max_count: Some(10000),
                compression_after: None,
                importance_threshold: 0.6,
            }
        ));
    }
    
    /// Record an event on the appropriate timeline(s)
    pub fn record_event(&mut self, event: TimelineEvent) {
        // Determine which timelines this event belongs on
        let timelines = self.determine_timelines(&event);
        
        for timeline_type in timelines {
            if let Some(timeline) = self.timelines.get_mut(&timeline_type) {
                let mut event_copy = event.clone();
                event_copy.timeline_type = timeline_type;
                timeline.add_event(event_copy.clone());
                
                // Record cross-reference
                self.event_to_timeline.insert(event_copy.id.clone(), timeline_type);
            }
        }
    }
    
    /// Determine which timelines an event belongs on
    fn determine_timelines(&self, event: &TimelineEvent) -> Vec<TimelineType> {
        let mut timelines = Vec::new();
        
        // All events go on episodic timeline
        timelines.push(TimelineType::Episodic);
        
        // Determine other timelines based on category and importance
        match event.category {
            EventCategory::LearningPoint | EventCategory::DecisionPoint => {
                timelines.push(TimelineType::Semantic);
                timelines.push(TimelineType::Procedural);
            }
            EventCategory::UserInteraction => {
                timelines.push(TimelineType::User);
                timelines.push(TimelineType::Working);
            }
            EventCategory::ToolExecution | EventCategory::FileOperation | EventCategory::CodeChange => {
                timelines.push(TimelineType::Procedural);
                timelines.push(TimelineType::Project);
                timelines.push(TimelineType::Working);
            }
            EventCategory::SuccessAchievement if event.importance >= EventImportance::High => {
                timelines.push(TimelineType::Semantic);
                timelines.push(TimelineType::Project);
            }
            EventCategory::ErrorOccurrence if event.importance >= EventImportance::Medium => {
                timelines.push(TimelineType::Procedural); // Learn from errors
                timelines.push(TimelineType::Project);
            }
            _ => {}
        }
        
        // Remove duplicates
        timelines.sort();
        timelines.dedup();
        
        timelines
    }
    
    /// Get events across all timelines for a time period
    pub fn get_events_in_period(&self, start: u64, end: u64) -> Vec<&TimelineEvent> {
        let mut all_events = Vec::new();
        
        for timeline in self.timelines.values() {
            all_events.extend(timeline.get_events_in_range(start, end));
        }
        
        // Sort by timestamp
        all_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        
        all_events
    }
    
    /// Get timeline for a specific type
    pub fn get_timeline(&self, timeline_type: TimelineType) -> Option<&Timeline> {
        self.timelines.get(&timeline_type)
    }
    
    /// Get working memory (recent events for current task)
    pub fn get_working_memory(&self, lookback_ms: u64) -> Vec<&TimelineEvent> {
        let now = current_timestamp();
        let start = now.saturating_sub(lookback_ms);
        
        if let Some(timeline) = self.timelines.get(&TimelineType::Working) {
            timeline.get_events_in_range(start, now)
        } else {
            Vec::new()
        }
    }
    
    /// Get episodic memory (recent detailed events)
    pub fn get_episodic_memory(&self, lookback_ms: u64) -> Vec<&TimelineEvent> {
        let now = current_timestamp();
        let start = now.saturating_sub(lookback_ms);
        
        if let Some(timeline) = self.timelines.get(&TimelineType::Episodic) {
            timeline.get_events_in_range(start, now)
        } else {
            Vec::new()
        }
    }
    
    /// Get semantic memory (facts and knowledge)
    pub fn get_semantic_memory(&self, query: &str) -> Vec<&TimelineEvent> {
        if let Some(timeline) = self.timelines.get(&TimelineType::Semantic) {
            // Simple keyword matching for now
            let query_lower = query.to_lowercase();
            timeline.events.values()
                .flat_map(|events| events.iter())
                .filter(|event| {
                    event.title.to_lowercase().contains(&query_lower) ||
                    event.description.to_lowercase().contains(&query_lower) ||
                    event.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
                })
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Get procedural memory (how-to knowledge)
    pub fn get_procedural_memory(&self, task: &str) -> Vec<&TimelineEvent> {
        if let Some(timeline) = self.timelines.get(&TimelineType::Procedural) {
            timeline.get_events_by_category(&EventCategory::ToolExecution)
                .into_iter()
                .chain(timeline.get_events_by_category(&EventCategory::CodeChange))
                .filter(|event| event.success.unwrap_or(false)) // Only successful procedures
                .filter(|event| {
                    event.title.to_lowercase().contains(&task.to_lowercase()) ||
                    event.description.to_lowercase().contains(&task.to_lowercase())
                })
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Start a new session
    pub fn start_new_session(&mut self) {
        self.current_session_id = uuid::Uuid::new_v4().to_string();
        self.session_start_time = current_timestamp();
        
        // Clear working timeline for new session
        if let Some(timeline) = self.timelines.get_mut(&TimelineType::Working) {
            timeline.events.clear();
            timeline.rebuild_indexes();
        }
    }
    
    /// Get current session summary
    pub fn get_session_summary(&self) -> TimelineSummary {
        let now = current_timestamp();
        
        if let Some(timeline) = self.timelines.get(&TimelineType::Episodic) {
            timeline.get_summary(self.session_start_time, now, 10)
        } else {
            TimelineSummary {
                start: self.session_start_time,
                end: now,
                total_events: 0,
                events_by_category: HashMap::new(),
                top_events: Vec::new(),
            }
        }
    }
}

// ── Utility Functions ────────────────────────────────────────────────────

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, NaiveDateTime, Utc};
    
    let naive = NaiveDateTime::from_timestamp_millis(timestamp as i64)
        .unwrap_or_else(|| NaiveDateTime::from_timestamp_millis(0).unwrap());
    let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);
    
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

// ── Integration Example ──────────────────────────────────────────────────

/// Helper to create timeline events from agent interactions
pub struct TimelineRecorder {
    manager: Arc<RwLock<TimelineManager>>,
}

impl TimelineRecorder {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(RwLock::new(TimelineManager::new())),
        }
    }
    
    /// Record a user interaction
    pub async fn record_user_interaction(&self, prompt: &str, response_summary: &str) {
        let event = TimelineEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timeline_type: TimelineType::Episodic, // Will be overridden
            timestamp: current_timestamp(),
            duration: None,
            relative_time: None,
            title: format!("User: {}", prompt.chars().take(50).collect::<String>()),
            description: format!("User asked: {}\nAssistant responded: {}", prompt, response_summary),
            category: EventCategory::UserInteraction,
            parent_id: None,
            related_ids: Vec::new(),
            precedes_ids: Vec::new(),
            follows_ids: Vec::new(),
            importance: EventImportance::Medium,
            confidence: 1.0,
            source: EventSource::UserInput,
            tags: vec!["interaction".to_string(), "user".to_string()],
            completed: true,
            success: Some(true),
            user_feedback: None,
        };
        
        let mut manager = self.manager.write().await;
        manager.record_event(event);
    }
    
    /// Record a tool execution
    pub async fn record_tool_execution(&self, tool_name: &str, target: &str, success: bool) {
        let event = TimelineEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timeline_type: TimelineType::Episodic,
            timestamp: current_timestamp(),
            duration: None,
            relative_time: None,
            title: format!("Tool: {} {}", tool_name, target),
            description: format!("Executed {} on {}", tool_name, target),
            category: EventCategory::ToolExecution,
            parent_id: None,
            related_ids: Vec::new(),
            precedes_ids: Vec::new(),
            follows_ids: Vec::new(),
            importance: if success { EventImportance::Medium } else { EventImportance::High },
            confidence: 1.0,
            source: EventSource::AgentAction,
            tags: vec!["tool".to_string(), tool_name.to_string()],
            completed: true,
            success: Some(success),
            user_feedback: None,
        };
        
        let mut manager = self.manager.write().await;
        manager.record_event(event);
    }
    
    /// Record a learning point
    pub async fn record_learning(&self, topic: &str, fact: &str, importance: EventImportance) {
        let event = TimelineEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timeline_type: TimelineType::Episodic,
            timestamp: current_timestamp(),
            duration: None,
            relative_time: None,
            title: format!("Learned: {}", topic),
            description: fact.to_string(),
            category: EventCategory::LearningPoint,
            parent_id: None,
            related_ids: Vec::new(),
            precedes_ids: Vec::new(),
            follows_ids: Vec::new(),
            importance,
            confidence: 0.8,
            source: EventSource::AgentAction,
            tags: vec!["learning".to_string(), "knowledge".to_string()],
            completed: true,
            success: Some(true),
            user_feedback: None,
        };
        
        let mut manager = self.manager.write().await;
        manager.record_event(event);
    }
    
    /// Get context for current task
    pub async fn get_task_context(&self, lookback_minutes: u64) -> String {
        let manager = self.manager.read().await;
        
        // Get working memory (last N minutes)
        let working_memory = manager.get_working_memory(lookback_minutes * 60 * 1000);
        
        // Get relevant procedural memory
        let current_task = "current"; // Would be determined from context
        let procedural_memory = manager.get_procedural_memory(current_task);
        
        let mut context = String::from("--- Timeline Context ---\n\n");
        
        if !working_memory.is_empty() {
            context.push_str("## Recent Activity\n");
            for event in working_memory.iter().take(5) {
                let time_str = format_timestamp(event.timestamp);
                context.push_str(&format!("• [{}] {}: {}\n", 
                    time_str, event.title, event.description));
            }
            context.push_str("\n");
        }
        
        if !procedural_memory.is_empty() {
            context.push_str("## Relevant Procedures\n");
            for event in procedural_memory.iter().take(3) {
                context.push_str(&format!("• {}: {}\n", event.title, event.description));
            }
        }
        
        context
    }
    
    /// Get session summary
    pub async fn get_session_summary(&self) -> String {
        let manager = self.manager.read().await;
        let summary = manager.get_session_summary();
        
        summary.to_markdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_timeline_creation() {
        let timeline = Timeline::new(
            TimelineType::Episodic,
            TimeGranularity::Seconds,
            RetentionPolicy {
                max_age: Some(1000),
                max_count: Some(10),
                compression_after: None,
                importance_threshold: 0.3,
            }
        );
        
        assert_eq!(timeline.timeline_type, TimelineType::Episodic);
        assert_eq!(timeline.granularity, TimeGranularity::Seconds);
    }
    
    #[test]
    fn test_event_recording() {
        let mut timeline = Timeline::new(
            TimelineType::Episodic,
            TimeGranularity::Seconds,
            RetentionPolicy {
                max_age: None,
                max_count: None,
                compression_after: None,
                importance_threshold: 0.0,
            }
        );
        
        let event = TimelineEvent {
            id: "test1".to_string(),
            timeline_type: TimelineType::Episodic,
            timestamp: 1000,
            duration: None,
            relative_time: None,
            title: "Test Event".to_string(),
            description: "This is a test".to_string(),
            category: EventCategory::UserInteraction,
            parent_id: None,
            related_ids: Vec::new(),
            precedes_ids: Vec::new(),
            follows_ids: Vec::new(),
            importance: EventImportance::Medium,
            confidence: 1.0,
            source: EventSource::UserInput,
            tags: vec!["test".to_string()],
            completed: true,
            success: Some(true),
            user_feedback: None,
        };
        
        timeline.add_event(event.clone());
        
        let retrieved = timeline.get_event_by_id("test1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Event");
    }
    
    #[tokio::test]
    async fn test_timeline_manager() {
        let recorder = TimelineRecorder::new();
        
        // Record some events
        recorder.record_user_interaction("Hello", "Hi there!").await;
        recorder.record_tool_execution("read_file", "main.rs", true).await;
        
        // Get context
        let context = recorder.get_task_context(5).await;
        assert!(!context.is_empty());
        assert!(context.contains("Timeline Context"));
    }
}