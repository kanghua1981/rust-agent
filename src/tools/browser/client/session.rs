use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::{BrowserManager, BrowserState, PerformanceMetrics, PerformanceSummary};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Browser session manager
pub struct BrowserSession {
    /// Browser manager
    manager: Arc<BrowserManager>,
    /// Current browser instance name
    current_instance: Option<String>,
    /// Current page name
    current_page: Option<String>,
}

impl BrowserSession {
    /// Create a new browser session
    pub fn new(manager: Arc<BrowserManager>) -> Self {
        Self {
            manager,
            current_instance: None,
            current_page: None,
        }
    }
    
    /// Connect to a browser instance
    pub async fn connect(&mut self, instance_name: &str) -> BrowserResult<()> {
        if !self.manager.has_browser(instance_name).await {
            return Err(BrowserError::NotFound(format!("Browser instance '{}' not found", instance_name)));
        }
        
        self.current_instance = Some(instance_name.to_string());
        self.current_page = None;
        
        Ok(())
    }
    
    /// Create a new browser instance
    pub async fn create_instance(&mut self, instance_name: &str) -> BrowserResult<()> {
        self.manager.launch_browser(None, Some(instance_name)).await?;
        self.current_instance = Some(instance_name.to_string());
        self.current_page = None;
        
        Ok(())
    }
    
    /// Create a new page/tab
    pub async fn create_page(&mut self, url: Option<&str>) -> BrowserResult<String> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let mut state = browser_state.lock().await;
        
        let page_name = state.create_page(url).await?;
        self.current_page = Some(page_name.clone());
        
        Ok(page_name)
    }
    
    /// Switch to a different page
    pub async fn switch_page(&mut self, page_name: &str) -> BrowserResult<()> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let mut state = browser_state.lock().await;
        
        if state.get_page(page_name).is_none() {
            return Err(BrowserError::NotFound(format!("Page '{}' not found", page_name)));
        }
        
        self.current_page = Some(page_name.to_string());
        
        Ok(())
    }
    
    /// Get the current browser state
    pub async fn current_browser(&self) -> BrowserResult<Arc<Mutex<BrowserState>>> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        self.manager.get_browser(instance_name).await
    }
    
    /// Get the current page state
    pub async fn current_page(&self) -> BrowserResult<(Arc<Mutex<BrowserState>>, String)> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let page_name = self.current_page
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No page selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        
        // Verify page exists
        {
            let mut state = browser_state.lock().await;
            if state.get_page(page_name).is_none() {
                return Err(BrowserError::NotFound(format!("Page '{}' not found", page_name)));
            }
        }
        
        Ok((browser_state, page_name.clone()))
    }
    
    /// Close current page
    pub async fn close_current_page(&mut self) -> BrowserResult<()> {
        let (browser_state, page_name) = self.current_page().await?;
        let mut state = browser_state.lock().await;
        
        state.close_page(&page_name).await?;
        self.current_page = None;
        
        Ok(())
    }
    
    /// Rename a page
    pub async fn rename_page(&mut self, old_name: &str, new_name: &str) -> BrowserResult<()> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let mut state = browser_state.lock().await;
        
        state.rename_page(old_name, new_name)?;
        
        // Update current page name if it was renamed
        if let Some(current_page) = &self.current_page {
            if current_page == old_name {
                self.current_page = Some(new_name.to_string());
            }
        }
        
        Ok(())
    }
    
    /// Close current browser instance
    pub async fn close_current_instance(&mut self) -> BrowserResult<()> {
        let instance_name = self.current_instance
            .take()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        self.manager.close_browser(&instance_name).await?;
        self.current_page = None;
        
        Ok(())
    }
    
    /// List available browser instances
    pub async fn list_instances(&self) -> BrowserResult<Vec<String>> {
        Ok(self.manager.list_browsers().await)
    }
    
    /// List pages in current browser instance
    pub async fn list_pages(&self) -> BrowserResult<Vec<String>> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        Ok(state.page_names())
    }
    
    /// Get current instance name
    pub fn current_instance(&self) -> Option<&str> {
        self.current_instance.as_deref()
    }
    
    /// Get current page name
    pub fn current_page_name(&self) -> Option<&str> {
        self.current_page.as_deref()
    }
    
    /// Check if a browser instance is connected
    pub fn is_connected(&self) -> bool {
        self.current_instance.is_some()
    }
    
    /// Check if a page is selected
    pub fn has_page(&self) -> bool {
        self.current_page.is_some()
    }
    
    /// Set page group
    pub async fn set_page_group(&mut self, page_name: &str, group_name: Option<&str>) -> BrowserResult<()> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let mut state = browser_state.lock().await;
        
        state.set_page_group(page_name, group_name)
    }
    
    /// Get pages by group
    pub async fn get_pages_by_group(&self, group_name: &str) -> BrowserResult<Vec<String>> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        let pages = state.get_pages_by_group(group_name);
        Ok(pages.iter().map(|p| p.name().to_string()).collect())
    }
    
    /// Get all groups
    pub async fn get_groups(&self) -> BrowserResult<Vec<String>> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        Ok(state.get_groups())
    }
    
    /// Get performance metrics for a page
    pub async fn get_page_performance(&self, page_name: &str) -> BrowserResult<Option<PerformanceMetrics>> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        Ok(state.get_page_performance(page_name).cloned())
    }
    
    /// Get performance summary for all pages
    pub async fn get_performance_summary(&self) -> BrowserResult<PerformanceSummary> {
        let instance_name = self.current_instance
            .as_ref()
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = self.manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        Ok(state.get_performance_summary())
    }
}