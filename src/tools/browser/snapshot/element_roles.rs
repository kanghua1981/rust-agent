#![allow(dead_code)]

use std::collections::HashMap;

/// Element roles reference system
#[derive(Debug, Clone)]
pub struct ElementRoles {
    /// HTML element to default ARIA role mapping
    html_to_aria: HashMap<String, String>,
    
    /// ARIA role to description mapping
    role_descriptions: HashMap<String, String>,
    
    /// ARIA properties and their purposes
    aria_properties: HashMap<String, String>,
    
    /// ARIA states and their purposes
    aria_states: HashMap<String, String>,
    
    /// Landmark roles
    landmark_roles: Vec<String>,
    
    /// Widget roles
    widget_roles: Vec<String>,
    
    /// Document structure roles
    structure_roles: Vec<String>,
    
    /// Live region roles
    live_region_roles: Vec<String>,
}

impl ElementRoles {
    /// Create a new element roles reference
    pub fn new() -> Self {
        let mut html_to_aria = HashMap::new();
        let mut role_descriptions = HashMap::new();
        let mut aria_properties = HashMap::new();
        let mut aria_states = HashMap::new();
        
        // HTML element to ARIA role mappings
        html_to_aria.insert("article".to_string(), "article".to_string());
        html_to_aria.insert("aside".to_string(), "complementary".to_string());
        html_to_aria.insert("button".to_string(), "button".to_string());
        html_to_aria.insert("details".to_string(), "group".to_string());
        html_to_aria.insert("dialog".to_string(), "dialog".to_string());
        html_to_aria.insert("footer".to_string(), "contentinfo".to_string());
        html_to_aria.insert("form".to_string(), "form".to_string());
        html_to_aria.insert("header".to_string(), "banner".to_string());
        html_to_aria.insert("li".to_string(), "listitem".to_string());
        html_to_aria.insert("main".to_string(), "main".to_string());
        html_to_aria.insert("nav".to_string(), "navigation".to_string());
        html_to_aria.insert("section".to_string(), "region".to_string());
        html_to_aria.insert("table".to_string(), "table".to_string());
        html_to_aria.insert("td".to_string(), "cell".to_string());
        html_to_aria.insert("th".to_string(), "columnheader".to_string());
        html_to_aria.insert("tr".to_string(), "row".to_string());
        
        // ARIA role descriptions
        role_descriptions.insert("alert".to_string(), "A message with important, time-sensitive information".to_string());
        role_descriptions.insert("alertdialog".to_string(), "A type of dialog that contains an alert message".to_string());
        role_descriptions.insert("application".to_string(), "A region declared as a web application".to_string());
        role_descriptions.insert("article".to_string(), "A section of a page that consists of a composition".to_string());
        role_descriptions.insert("banner".to_string(), "A region that contains mostly site-oriented content".to_string());
        role_descriptions.insert("button".to_string(), "An input that allows for user-triggered actions".to_string());
        role_descriptions.insert("checkbox".to_string(), "A checkable input that has three possible values".to_string());
        role_descriptions.insert("complementary".to_string(), "A supporting section of the document".to_string());
        role_descriptions.insert("contentinfo".to_string(), "A large perceivable region that contains information about the parent document".to_string());
        role_descriptions.insert("dialog".to_string(), "A dialog is a window overlaid on either the primary window".to_string());
        role_descriptions.insert("form".to_string(), "A landmark region that contains a collection of items and objects".to_string());
        role_descriptions.insert("grid".to_string(), "A widget that contains one or more rows of cells".to_string());
        role_descriptions.insert("heading".to_string(), "A heading for a section of the page".to_string());
        role_descriptions.insert("img".to_string(), "A container for a collection of elements that form an image".to_string());
        role_descriptions.insert("link".to_string(), "An interactive reference to an internal or external resource".to_string());
        role_descriptions.insert("list".to_string(), "A group of non-interactive list items".to_string());
        role_descriptions.insert("listbox".to_string(), "A widget that allows the user to select one or more items".to_string());
        role_descriptions.insert("listitem".to_string(), "A single item in a list or directory".to_string());
        role_descriptions.insert("main".to_string(), "The main content of a document".to_string());
        role_descriptions.insert("navigation".to_string(), "A collection of navigational elements".to_string());
        role_descriptions.insert("region".to_string(), "A perceivable section containing content that is relevant to a specific purpose".to_string());
        role_descriptions.insert("search".to_string(), "A landmark region that contains a collection of items and objects".to_string());
        
        // ARIA properties
        aria_properties.insert("aria-label".to_string(), "Defines a string value that labels the current element".to_string());
        aria_properties.insert("aria-labelledby".to_string(), "Identifies the element (or elements) that labels the current element".to_string());
        aria_properties.insert("aria-describedby".to_string(), "Identifies the element (or elements) that describes the object".to_string());
        aria_properties.insert("aria-hidden".to_string(), "Indicates whether the element is exposed to an accessibility API".to_string());
        aria_properties.insert("aria-live".to_string(), "Indicates that an element will be updated".to_string());
        aria_properties.insert("aria-atomic".to_string(), "Indicates whether assistive technologies will present all changed regions".to_string());
        aria_properties.insert("aria-relevant".to_string(), "Indicates what types of changes should be presented to the user".to_string());
        aria_properties.insert("aria-busy".to_string(), "Indicates whether an element is being modified".to_string());
        
        // ARIA states
        aria_states.insert("aria-checked".to_string(), "Indicates the current 'checked' state of checkboxes, radio buttons, and other widgets".to_string());
        aria_states.insert("aria-disabled".to_string(), "Indicates that the element is perceivable but disabled".to_string());
        aria_states.insert("aria-expanded".to_string(), "Indicates whether the element, or another grouping element it controls, is currently expanded or collapsed".to_string());
        aria_states.insert("aria-selected".to_string(), "Indicates the current 'selected' state of various widgets".to_string());
        aria_states.insert("aria-pressed".to_string(), "Indicates the current 'pressed' state of toggle buttons".to_string());
        aria_states.insert("aria-invalid".to_string(), "Indicates the entered value does not conform to the format expected by the application".to_string());
        aria_states.insert("aria-required".to_string(), "Indicates that user input is required on the element before a form may be submitted".to_string());
        aria_states.insert("aria-readonly".to_string(), "Indicates that the element is not editable, but is otherwise operable".to_string());
        
        // Role categories
        let landmark_roles = vec![
            "banner".to_string(),
            "complementary".to_string(),
            "contentinfo".to_string(),
            "form".to_string(),
            "main".to_string(),
            "navigation".to_string(),
            "region".to_string(),
            "search".to_string(),
        ];
        
        let widget_roles = vec![
            "button".to_string(),
            "checkbox".to_string(),
            "gridcell".to_string(),
            "link".to_string(),
            "menuitem".to_string(),
            "menuitemcheckbox".to_string(),
            "menuitemradio".to_string(),
            "option".to_string(),
            "progressbar".to_string(),
            "radio".to_string(),
            "scrollbar".to_string(),
            "slider".to_string(),
            "spinbutton".to_string(),
            "switch".to_string(),
            "tab".to_string(),
            "tabpanel".to_string(),
            "textbox".to_string(),
            "treeitem".to_string(),
        ];
        
        let structure_roles = vec![
            "article".to_string(),
            "cell".to_string(),
            "columnheader".to_string(),
            "definition".to_string(),
            "directory".to_string(),
            "document".to_string(),
            "group".to_string(),
            "heading".to_string(),
            "img".to_string(),
            "list".to_string(),
            "listitem".to_string(),
            "math".to_string(),
            "none".to_string(),
            "note".to_string(),
            "presentation".to_string(),
            "row".to_string(),
            "rowgroup".to_string(),
            "rowheader".to_string(),
            "separator".to_string(),
            "table".to_string(),
            "toolbar".to_string(),
            "tooltip".to_string(),
        ];
        
        let live_region_roles = vec![
            "alert".to_string(),
            "log".to_string(),
            "marquee".to_string(),
            "status".to_string(),
            "timer".to_string(),
        ];
        
        Self {
            html_to_aria,
            role_descriptions,
            aria_properties,
            aria_states,
            landmark_roles,
            widget_roles,
            structure_roles,
            live_region_roles,
        }
    }
    
    /// Get ARIA role for HTML element
    pub fn get_aria_role_for_html(&self, html_tag: &str) -> Option<&String> {
        self.html_to_aria.get(html_tag)
    }
    
    /// Get description for ARIA role
    pub fn get_role_description(&self, role: &str) -> Option<&String> {
        self.role_descriptions.get(role)
    }
    
    /// Get description for ARIA property
    pub fn get_property_description(&self, property: &str) -> Option<&String> {
        self.aria_properties.get(property)
    }
    
    /// Get description for ARIA state
    pub fn get_state_description(&self, state: &str) -> Option<&String> {
        self.aria_states.get(state)
    }
    
    /// Check if role is a landmark role
    pub fn is_landmark_role(&self, role: &str) -> bool {
        self.landmark_roles.contains(&role.to_string())
    }
    
    /// Check if role is a widget role
    pub fn is_widget_role(&self, role: &str) -> bool {
        self.widget_roles.contains(&role.to_string())
    }
    
    /// Check if role is a structure role
    pub fn is_structure_role(&self, role: &str) -> bool {
        self.structure_roles.contains(&role.to_string())
    }
    
    /// Check if role is a live region role
    pub fn is_live_region_role(&self, role: &str) -> bool {
        self.live_region_roles.contains(&role.to_string())
    }
    
    /// Get all roles in a category
    pub fn get_roles_by_category(&self, category: &str) -> Vec<&String> {
        match category {
            "landmark" => self.landmark_roles.iter().collect(),
            "widget" => self.widget_roles.iter().collect(),
            "structure" => self.structure_roles.iter().collect(),
            "live_region" => self.live_region_roles.iter().collect(),
            _ => Vec::new(),
        }
    }
    
    /// Validate ARIA role usage
    pub fn validate_role_usage(&self, role: &str, html_tag: Option<&str>) -> Vec<String> {
        let mut warnings = Vec::new();
        
        // Check if role is valid
        if !self.role_descriptions.contains_key(role) {
            warnings.push(format!("Unknown ARIA role: {}", role));
        }
        
        // Check if role conflicts with default HTML semantics
        if let Some(tag) = html_tag {
            if let Some(default_role) = self.get_aria_role_for_html(tag) {
                if default_role == role {
                    warnings.push(format!("Redundant ARIA role: {} on <{}> element", role, tag));
                }
            }
        }
        
        warnings
    }
}

impl Default for ElementRoles {
    fn default() -> Self {
        Self::new()
    }
}