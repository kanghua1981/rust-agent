use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ARIA snapshot of a web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaSnapshot {
    /// Page title (from aria-label or title)
    pub page_title: Option<String>,
    
    /// Landmarks found on the page
    pub landmarks: Vec<AriaLandmark>,
    
    /// ARIA roles used on the page
    pub roles: Vec<AriaRole>,
    
    /// ARIA properties and states
    pub properties: Vec<AriaProperty>,
    
    /// Live regions
    pub live_regions: Vec<LiveRegion>,
    
    /// Focusable elements
    pub focusable_elements: Vec<FocusableElement>,
    
    /// Keyboard navigation order
    pub tab_order: Vec<TabOrderElement>,
    
    /// Accessibility violations
    pub violations: Vec<AccessibilityViolation>,
    
    /// ARIA tree structure
    pub aria_tree: Vec<AriaTreeNode>,
    
    /// Screen reader announcements
    pub announcements: Vec<ScreenReaderAnnouncement>,
}

/// ARIA landmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaLandmark {
    /// Landmark type (banner, main, navigation, etc.)
    pub landmark_type: String,
    
    /// Landmark label
    pub label: Option<String>,
    
    /// Element selector
    pub selector: String,
    
    /// Whether landmark is unique
    pub unique: bool,
    
    /// Landmark position (x, y, width, height)
    pub position: Option<(f64, f64, f64, f64)>,
}

/// ARIA role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaRole {
    /// Role name
    pub role: String,
    
    /// Element selector
    pub selector: String,
    
    /// Role description
    pub description: Option<String>,
    
    /// Whether role is valid
    pub valid: bool,
    
    /// Number of elements with this role
    pub count: usize,
}

/// ARIA property or state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaProperty {
    /// Property name (aria-label, aria-describedby, etc.)
    pub name: String,
    
    /// Property value
    pub value: String,
    
    /// Element selector
    pub selector: String,
    
    /// Whether property is valid
    pub valid: bool,
    
    /// Property type (property, state, etc.)
    pub property_type: String,
}

/// Live region for dynamic content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRegion {
    /// Live region type (polite, assertive, off)
    pub live_type: String,
    
    /// Element selector
    pub selector: String,
    
    /// Whether region is atomic
    pub atomic: bool,
    
    /// Whether region is relevant
    pub relevant: Option<String>,
    
    /// Current content
    pub content: String,
}

/// Focusable element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusableElement {
    /// Element type
    pub element_type: String,
    
    /// Element selector
    pub selector: String,
    
    /// Tab index
    pub tab_index: Option<i32>,
    
    /// Whether element is focusable by default
    pub focusable_by_default: bool,
    
    /// Whether element has focus
    pub has_focus: bool,
    
    /// Element label
    pub label: Option<String>,
}

/// Tab order element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabOrderElement {
    /// Tab order position
    pub position: usize,
    
    /// Element selector
    pub selector: String,
    
    /// Element type
    pub element_type: String,
    
    /// Element label
    pub label: Option<String>,
    
    /// Whether element is reachable
    pub reachable: bool,
}

/// Accessibility violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityViolation {
    /// Violation type
    pub violation_type: String,
    
    /// WCAG guideline
    pub wcag_guideline: String,
    
    /// Severity (low, medium, high, critical)
    pub severity: String,
    
    /// Element selector
    pub selector: String,
    
    /// Violation description
    pub description: String,
    
    /// How to fix
    pub fix: String,
}

/// ARIA tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AriaTreeNode {
    /// Node ID
    pub id: String,
    
    /// Node role
    pub role: String,
    
    /// Node label
    pub label: Option<String>,
    
    /// Node level in tree
    pub level: usize,
    
    /// Whether node is expanded
    pub expanded: bool,
    
    /// Whether node is selected
    pub selected: bool,
    
    /// Child nodes
    pub children: Vec<AriaTreeNode>,
}

/// Screen reader announcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenReaderAnnouncement {
    /// Announcement text
    pub text: String,
    
    /// Announcement priority (polite, assertive)
    pub priority: String,
    
    /// Timestamp
    pub timestamp: i64,
    
    /// Element selector (if related to element)
    pub selector: Option<String>,
}

impl AriaSnapshot {
    /// Create a new ARIA snapshot
    pub fn new() -> Self {
        Self {
            page_title: None,
            landmarks: Vec::new(),
            roles: Vec::new(),
            properties: Vec::new(),
            live_regions: Vec::new(),
            focusable_elements: Vec::new(),
            tab_order: Vec::new(),
            violations: Vec::new(),
            aria_tree: Vec::new(),
            announcements: Vec::new(),
        }
    }
    
    /// Convert snapshot to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Get accessibility score (0-100)
    pub fn accessibility_score(&self) -> u8 {
        let total_violations = self.violations.len();
        let critical_violations = self.violations.iter()
            .filter(|v| v.severity == "critical" || v.severity == "high")
            .count();
        
        // Calculate score based on violations
        let base_score = 100;
        let deduction = (critical_violations * 20) + (total_violations * 5);
        
        (base_score as isize - deduction as isize).max(0) as u8
    }
    
    /// Get summary of ARIA usage
    pub fn summary(&self) -> HashMap<String, usize> {
        let mut summary = HashMap::new();
        
        summary.insert("landmarks".to_string(), self.landmarks.len());
        summary.insert("roles".to_string(), self.roles.len());
        summary.insert("properties".to_string(), self.properties.len());
        summary.insert("live_regions".to_string(), self.live_regions.len());
        summary.insert("focusable_elements".to_string(), self.focusable_elements.len());
        summary.insert("violations".to_string(), self.violations.len());
        
        summary
    }
}

impl Default for AriaSnapshot {
    fn default() -> Self {
        Self::new()
    }
}