use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// AI-friendly snapshot of a web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSnapshot {
    /// Page title
    pub title: Option<String>,
    
    /// Page URL
    pub url: Option<String>,
    
    /// Main content text (extracted)
    pub main_content: String,
    
    /// Page structure (headings, sections, etc.)
    pub structure: Vec<PageSection>,
    
    /// Interactive elements (buttons, links, forms)
    pub interactive_elements: Vec<InteractiveElement>,
    
    /// Images with alt text
    pub images: Vec<ImageInfo>,
    
    /// Tables with data
    pub tables: Vec<TableInfo>,
    
    /// Page metadata
    pub metadata: HashMap<String, String>,
    
    /// Accessibility score (0-100)
    pub accessibility_score: u8,
    
    /// Semantic HTML usage
    pub semantic_elements: Vec<SemanticElement>,
    
    /// Timestamp of snapshot
    pub timestamp: i64,
}

/// Page section (heading + content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSection {
    /// Heading level (1-6)
    pub heading_level: u8,
    
    /// Heading text
    pub heading_text: String,
    
    /// Section content
    pub content: String,
    
    /// Section ID (if any)
    pub id: Option<String>,
    
    /// ARIA label (if any)
    pub aria_label: Option<String>,
}

/// Interactive element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElement {
    /// Element type (button, link, input, etc.)
    pub element_type: String,
    
    /// Element text/label
    pub label: Option<String>,
    
    /// Element selector
    pub selector: String,
    
    /// ARIA role
    pub aria_role: Option<String>,
    
    /// ARIA label
    pub aria_label: Option<String>,
    
    /// Whether element is visible
    pub visible: bool,
    
    /// Whether element is enabled
    pub enabled: bool,
}

/// Image information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Image source URL
    pub src: String,
    
    /// Alt text
    pub alt: Option<String>,
    
    /// Image dimensions (width x height)
    pub dimensions: Option<(u32, u32)>,
    
    /// Image title
    pub title: Option<String>,
    
    /// Whether image is decorative
    pub decorative: bool,
}

/// Table information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table caption
    pub caption: Option<String>,
    
    /// Number of rows
    pub rows: usize,
    
    /// Number of columns
    pub columns: usize,
    
    /// Table headers
    pub headers: Vec<String>,
    
    /// Table data (first few rows)
    pub data: Vec<Vec<String>>,
    
    /// Whether table has proper markup
    pub has_proper_markup: bool,
}

/// Semantic HTML element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticElement {
    /// Element tag (article, section, nav, etc.)
    pub tag: String,
    
    /// Element content
    pub content: String,
    
    /// ARIA role
    pub aria_role: Option<String>,
    
    /// ARIA label
    pub aria_label: Option<String>,
}

impl AiSnapshot {
    /// Create a new AI snapshot
    pub fn new() -> Self {
        Self {
            title: None,
            url: None,
            main_content: String::new(),
            structure: Vec::new(),
            interactive_elements: Vec::new(),
            images: Vec::new(),
            tables: Vec::new(),
            metadata: HashMap::new(),
            accessibility_score: 0,
            semantic_elements: Vec::new(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    
    /// Convert snapshot to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Convert snapshot to Markdown format
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        
        if let Some(title) = &self.title {
            md.push_str(&format!("# {}\n\n", title));
        }
        
        if let Some(url) = &self.url {
            md.push_str(&format!("**URL**: {}\n\n", url));
        }
        
        md.push_str("## Page Structure\n\n");
        for section in &self.structure {
            md.push_str(&format!("{} {}\n\n", "#".repeat(section.heading_level as usize), section.heading_text));
            md.push_str(&format!("{}\n\n", section.content));
        }
        
        md.push_str("## Interactive Elements\n\n");
        for element in &self.interactive_elements {
            md.push_str(&format!("- **{}**: {}\n", element.element_type, element.label.as_deref().unwrap_or("No label")));
            md.push_str(&format!("  - Selector: `{}`\n", element.selector));
            if let Some(role) = &element.aria_role {
                md.push_str(&format!("  - ARIA Role: {}\n", role));
            }
            md.push_str(&format!("  - Visible: {}, Enabled: {}\n\n", element.visible, element.enabled));
        }
        
        md.push_str("## Accessibility\n\n");
        md.push_str(&format!("**Score**: {}%\n\n", self.accessibility_score));
        
        md.push_str("## Semantic Elements\n\n");
        for element in &self.semantic_elements {
            md.push_str(&format!("- **<{}>**: {}\n", element.tag, element.content));
        }
        
        md
    }
}

impl Default for AiSnapshot {
    fn default() -> Self {
        Self::new()
    }
}