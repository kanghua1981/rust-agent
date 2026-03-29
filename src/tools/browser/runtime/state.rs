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