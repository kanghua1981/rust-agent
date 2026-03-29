use crate::tools::browser::config::BrowserProfile;
use crate::tools::browser::error::{BrowserError, connection_error};
use chromiumoxide::browser::Browser;
use chromiumoxide::page::Page;
use std::collections::HashMap;
use tokio::task::JoinHandle;

/// State of a browser instance
pub struct BrowserState {
    /// Browser instance
    browser: Browser,
    /// Browser handler task
    handler_task: JoinHandle<()>,
    /// Browser profile
    profile: BrowserProfile,
    /// Instance name
    instance_name: String,
    /// Active pages/tabs
    pages: HashMap<String, PageState>,
    /// Next page ID
    next_page_id: u32,
    /// Whether the browser is closed
    closed: bool,
}

impl BrowserState {
    /// Create a new browser state
    pub fn new(
        browser: Browser,
        handler_task: JoinHandle<()>,
        profile: BrowserProfile,
        instance_name: String,
    ) -> Self {
        Self {
            browser,
            handler_task,
            profile,
            instance_name,
            pages: HashMap::new(),
            next_page_id: 1,
            closed: false,
        }
    }
    
    /// Create a new page/tab
    pub async fn create_page(&mut self, url: Option<&str>) -> Result<String, BrowserError> {
        if self.closed {
            return Err(connection_error("Browser is closed".to_string()));
        }
        
        let page_id = self.next_page_id;
        self.next_page_id += 1;
        
        let page_name = format!("page_{}", page_id);
        
        // Create new page
        let page = self.browser
            .new_page(url.unwrap_or("about:blank"))
            .await
            .map_err(|e| connection_error(format!("Failed to create page: {}", e)))?;
        
        // Create page state
        let page_state = PageState::new(page, page_name.clone(), page_id);
        
        // Store page state
        self.pages.insert(page_name.clone(), page_state);
        
        Ok(page_name)
    }
    
    /// Get a page by name
    pub fn get_page(&mut self, page_name: &str) -> Option<&mut PageState> {
        self.pages.get_mut(page_name)
    }
    
    /// Close a page
    pub async fn close_page(&mut self, page_name: &str) -> Result<(), BrowserError> {
        if let Some(mut page_state) = self.pages.remove(page_name) {
            page_state.close().await?;
        }
        Ok(())
    }
    
    /// Rename a page
    pub fn rename_page(&mut self, old_name: &str, new_name: &str) -> Result<(), BrowserError> {
        if self.pages.contains_key(new_name) {
            return Err(BrowserError::Operation(format!("Page '{}' already exists", new_name)));
        }
        
        if let Some(mut page_state) = self.pages.remove(old_name) {
            page_state.rename(new_name.to_string());
            self.pages.insert(new_name.to_string(), page_state);
            Ok(())
        } else {
            Err(BrowserError::NotFound(format!("Page '{}' not found", old_name)))
        }
    }
    
    /// Close all pages
    pub async fn close_all_pages(&mut self) -> Result<(), BrowserError> {
        let page_names: Vec<String> = self.pages.keys().cloned().collect();
        
        for page_name in page_names {
            self.close_page(&page_name).await?;
        }
        
        Ok(())
    }
    
    /// List all pages
    pub fn list_pages(&self) -> Vec<&PageState> {
        self.pages.values().collect()
    }
    
    /// Get page names
    pub fn page_names(&self) -> Vec<String> {
        self.pages.keys().cloned().collect()
    }
    
    /// Close the browser
    pub async fn close(&mut self) -> Result<(), BrowserError> {
        if self.closed {
            return Ok(());
        }
        
        // Close all pages first
        self.close_all_pages().await?;
        
        // Close browser
        self.browser
            .close()
            .await
            .map_err(|e| connection_error(format!("Failed to close browser: {}", e)))?;
        
        // Wait for handler task to finish
        self.handler_task.abort();
        
        self.closed = true;
        
        Ok(())
    }
    
    /// Check if browser is closed
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    
    /// Get the browser profile
    pub fn profile(&self) -> &BrowserProfile {
        &self.profile
    }
    
    /// Get the instance name
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }
    
    /// Get the browser instance (for low-level operations)
    pub fn browser(&self) -> &Browser {
        &self.browser
    }
    
    /// Set page group
    pub fn set_page_group(&mut self, page_name: &str, group_name: Option<&str>) -> Result<(), BrowserError> {
        if let Some(page_state) = self.pages.get_mut(page_name) {
            page_state.tab_state_mut().group = group_name.map(|s| s.to_string());
            Ok(())
        } else {
            Err(BrowserError::NotFound(format!("Page '{}' not found", page_name)))
        }
    }
    
    /// Get pages by group
    pub fn get_pages_by_group(&self, group_name: &str) -> Vec<&PageState> {
        self.pages.values()
            .filter(|page| page.tab_state().group.as_deref() == Some(group_name))
            .collect()
    }
    
    /// Get all groups
    pub fn get_groups(&self) -> Vec<String> {
        let mut groups = std::collections::HashSet::new();
        for page in self.pages.values() {
            if let Some(group) = &page.tab_state().group {
                groups.insert(group.clone());
            }
        }
        groups.into_iter().collect()
    }
    
    /// Get performance metrics for a page
    pub fn get_page_performance(&self, page_name: &str) -> Option<&PerformanceMetrics> {
        self.pages.get(page_name).map(|page| &page.tab_state().performance)
    }
    
    /// Update performance metrics for a page
    pub fn update_page_performance<F>(&mut self, page_name: &str, updater: F) -> Result<(), BrowserError>
    where
        F: FnOnce(&mut PerformanceMetrics),
    {
        if let Some(page_state) = self.pages.get_mut(page_name) {
            updater(&mut page_state.tab_state_mut().performance);
            Ok(())
        } else {
            Err(BrowserError::NotFound(format!("Page '{}' not found", page_name)))
        }
    }
    
    /// Get all pages performance summary
    pub fn get_performance_summary(&self) -> PerformanceSummary {
        let mut summary = PerformanceSummary::default();
        
        for page in self.pages.values() {
            let metrics = &page.tab_state().performance;
            
            summary.total_pages += 1;
            
            if let Some(memory) = metrics.memory_usage {
                summary.total_memory += memory;
                summary.max_memory = summary.max_memory.max(memory);
            }
            
            if let Some(load_time) = metrics.load_time {
                summary.total_load_time += load_time;
                summary.max_load_time = summary.max_load_time.max(load_time);
            }
            
            summary.total_requests += metrics.request_count;
            summary.total_data_transferred += metrics.data_transferred;
        }
        
        if summary.total_pages > 0 {
            summary.avg_memory = summary.total_memory / summary.total_pages as u64;
            summary.avg_load_time = summary.total_load_time / summary.total_pages as u64;
        }
        
        summary
    }
}

/// Performance summary for all tabs
#[derive(Debug, Clone, Default)]
pub struct PerformanceSummary {
    /// Total number of pages
    pub total_pages: u32,
    /// Total memory usage in bytes
    pub total_memory: u64,
    /// Average memory usage in bytes
    pub avg_memory: u64,
    /// Maximum memory usage in bytes
    pub max_memory: u64,
    /// Total load time in milliseconds
    pub total_load_time: u64,
    /// Average load time in milliseconds
    pub avg_load_time: u64,
    /// Maximum load time in milliseconds
    pub max_load_time: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Total data transferred in bytes
    pub total_data_transferred: u64,
}

/// State of a page/tab
pub struct PageState {
    /// Page instance
    page: Page,
    /// Page name
    name: String,
    /// Page ID
    id: u32,
    /// Whether the page is closed
    closed: bool,
    /// Tab state (for multi-tab management)
    tab_state: TabState,
}

impl PageState {
    /// Create a new page state
    pub fn new(page: Page, name: String, id: u32) -> Self {
        Self {
            page,
            name,
            id,
            closed: false,
            tab_state: TabState::default(),
        }
    }
    
    /// Get the page instance
    pub fn page(&self) -> &Page {
        &self.page
    }
    
    /// Get mutable access to the page instance
    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }
    
    /// Get the page name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get the page ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// Check if page is closed
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    
    /// Close the page
    pub async fn close(&mut self) -> Result<(), BrowserError> {
        if self.closed {
            return Ok(());
        }
        
        // Note: chromiumoxide doesn't have a direct page.close() method
        // The page will be cleaned up when the browser is closed
        self.closed = true;
        
        Ok(())
    }
    
    /// Get tab state
    pub fn tab_state(&self) -> &TabState {
        &self.tab_state
    }
    
    /// Get mutable tab state
    pub fn tab_state_mut(&mut self) -> &mut TabState {
        &mut self.tab_state
    }
    
    /// Rename the page
    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }
}

/// State of a tab within a browser
#[derive(Debug, Clone, Default)]
pub struct TabState {
    /// Tab title
    pub title: Option<String>,
    /// Tab URL
    pub url: Option<String>,
    /// Whether tab is active
    pub active: bool,
    /// Whether tab is loading
    pub loading: bool,
    /// Tab group (for grouping related tabs)
    pub group: Option<String>,
    /// Tab navigation history
    pub history: Vec<String>,
    /// Current history position
    pub history_position: usize,
    /// Tab cookies
    pub cookies: Vec<Cookie>,
    /// Tab local storage
    pub local_storage: Vec<LocalStorageItem>,
    /// Tab session storage
    pub session_storage: Vec<SessionStorageItem>,
    /// Performance metrics
    pub performance: PerformanceMetrics,
}

/// Performance metrics for a tab
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Memory usage in bytes
    pub memory_usage: Option<u64>,
    /// Load time in milliseconds
    pub load_time: Option<u64>,
    /// Number of requests
    pub request_count: u32,
    /// Total data transferred in bytes
    pub data_transferred: u64,
    /// JavaScript heap size in bytes
    pub js_heap_size: Option<u64>,
    /// DOM node count
    pub dom_node_count: Option<u32>,
    /// Last updated timestamp
    pub last_updated: i64,
}

/// Cookie representation
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub expires: Option<i64>,
}

/// Same site cookie setting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

/// Local storage item
#[derive(Debug, Clone)]
pub struct LocalStorageItem {
    pub key: String,
    pub value: String,
}

/// Session storage item
#[derive(Debug, Clone)]
pub struct SessionStorageItem {
    pub key: String,
    pub value: String,
}