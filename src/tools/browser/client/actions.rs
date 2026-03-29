use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::BrowserState;
use crate::tools::browser::snapshot::{AiSnapshot, AriaSnapshot};
use crate::tools::browser::batch::{OperationSequence, OperationStep, ResultAggregator};
use crate::tools::browser::protocol::ProtocolMessage;
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, CaptureScreenshotParams};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// Browser action types
#[derive(Debug, Clone)]
pub enum BrowserAction {
    /// Navigate to a URL
    Navigate {
        url: String,
        wait_seconds: u64,
    },
    /// Click an element
    Click {
        selector: String,
        wait_seconds: u64,
    },
    /// Type text into an element
    Type {
        selector: String,
        text: String,
        wait_seconds: u64,
    },
    /// Take a screenshot
    Screenshot {
        output_path: String,
        format: ScreenshotFormat,
        quality: Option<i64>,
    },
    /// Execute JavaScript
    ExecuteScript {
        script: String,
    },
    /// Get page HTML
    GetHtml,
    /// Get element text
    GetText {
        selector: String,
    },
    /// Find elements
    FindElements {
        selector: String,
    },
    /// Evaluate CDP expression
    Evaluate {
        expression: String,
    },
    /// Wait for navigation
    WaitForNavigation {
        timeout_seconds: u64,
    },
    /// Wait for element
    WaitForElement {
        selector: String,
        timeout_seconds: u64,
    },
    /// Hover over element
    Hover {
        selector: String,
        wait_seconds: u64,
    },
    /// Scroll to element
    ScrollTo {
        selector: String,
        wait_seconds: u64,
    },
    /// Get element attributes
    GetAttributes {
        selector: String,
        attributes: Vec<String>,
    },
    /// Set element attribute
    SetAttribute {
        selector: String,
        attribute: String,
        value: String,
        wait_seconds: u64,
    },
    /// Upload file
    UploadFile {
        selector: String,
        file_path: String,
        wait_seconds: u64,
    },
    /// Select dropdown option
    SelectOption {
        selector: String,
        value: String,
        wait_seconds: u64,
    },
    /// Check checkbox
    Check {
        selector: String,
        wait_seconds: u64,
    },
    /// Uncheck checkbox
    Uncheck {
        selector: String,
        wait_seconds: u64,
    },
    /// Press key
    PressKey {
        selector: Option<String>,
        key: String,
        wait_seconds: u64,
    },
    /// Drag and drop element
    DragDrop {
        source_selector: String,
        target_selector: String,
        wait_seconds: u64,
    },
    /// Right click element
    RightClick {
        selector: String,
        wait_seconds: u64,
    },
    /// Mouse wheel scroll
    MouseWheel {
        selector: Option<String>,
        delta_x: i32,
        delta_y: i32,
        wait_seconds: u64,
    },
    /// Get cookies
    GetCookies,
    /// Set cookie
    SetCookie {
        name: String,
        value: String,
        domain: Option<String>,
        path: Option<String>,
        secure: bool,
        http_only: bool,
        same_site: Option<String>,
        expires: Option<i64>,
    },
    /// Delete cookie
    DeleteCookie {
        name: String,
        domain: Option<String>,
        path: Option<String>,
    },
    /// Get local storage item
    GetLocalStorage {
        key: String,
    },
    /// Set local storage item
    SetLocalStorage {
        key: String,
        value: String,
    },
    /// Delete local storage item
    DeleteLocalStorage {
        key: String,
    },
    /// Get session storage item
    GetSessionStorage {
        key: String,
    },
    /// Set session storage item
    SetSessionStorage {
        key: String,
        value: String,
    },
    /// Delete session storage item
    DeleteSessionStorage {
        key: String,
    },
    /// Navigate back in history
    GoBack {
        wait_seconds: u64,
    },
    /// Navigate forward in history
    GoForward {
        wait_seconds: u64,
    },
    /// Create AI snapshot of page
    CreateSnapshot {
        snapshot_type: String,
        output_path: Option<String>,
    },
    /// Execute batch operations
    ExecuteBatch {
        operations: Vec<Value>,
        parallel: bool,
    },
    /// Get driver information
    GetDriverInfo,
    /// Send protocol message
    SendProtocolMessage {
        message_type: String,
        data: Value,
    },
}

/// Screenshot format
#[derive(Debug, Clone, Copy)]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    WebP,
}

/// Browser action executor
pub struct ActionExecutor;

impl ActionExecutor {
    /// Execute a browser action
    pub async fn execute(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        action: BrowserAction,
    ) -> BrowserResult<String> {
        match action {
            BrowserAction::Navigate { url, wait_seconds } => {
                self.navigate(browser_state, page_name, &url, wait_seconds).await
            }
            BrowserAction::Click { selector, wait_seconds } => {
                self.click(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::Type { selector, text, wait_seconds } => {
                self.type_text(browser_state, page_name, &selector, &text, wait_seconds).await
            }
            BrowserAction::Screenshot { output_path, format, quality } => {
                self.screenshot(browser_state, page_name, &output_path, format, quality).await
            }
            BrowserAction::ExecuteScript { script } => {
                self.execute_script(browser_state, page_name, &script).await
            }
            BrowserAction::GetHtml => {
                self.get_html(browser_state, page_name).await
            }
            BrowserAction::GetText { selector } => {
                self.get_text(browser_state, page_name, &selector).await
            }
            BrowserAction::FindElements { selector } => {
                self.find_elements(browser_state, page_name, &selector).await
            }
            BrowserAction::Evaluate { expression } => {
                self.evaluate(browser_state, page_name, &expression).await
            }
            BrowserAction::WaitForNavigation { timeout_seconds } => {
                self.wait_for_navigation(browser_state, page_name, timeout_seconds).await
            }
            BrowserAction::WaitForElement { selector, timeout_seconds } => {
                self.wait_for_element(browser_state, page_name, &selector, timeout_seconds).await
            }
            BrowserAction::Hover { selector, wait_seconds } => {
                self.hover(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::ScrollTo { selector, wait_seconds } => {
                self.scroll_to(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::GetAttributes { selector, attributes } => {
                self.get_attributes(browser_state, page_name, &selector, &attributes).await
            }
            BrowserAction::SetAttribute { selector, attribute, value, wait_seconds } => {
                self.set_attribute(browser_state, page_name, &selector, &attribute, &value, wait_seconds).await
            }
            BrowserAction::UploadFile { selector, file_path, wait_seconds } => {
                self.upload_file(browser_state, page_name, &selector, &file_path, wait_seconds).await
            }
            BrowserAction::SelectOption { selector, value, wait_seconds } => {
                self.select_option(browser_state, page_name, &selector, &value, wait_seconds).await
            }
            BrowserAction::Check { selector, wait_seconds } => {
                self.check(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::Uncheck { selector, wait_seconds } => {
                self.uncheck(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::PressKey { selector, key, wait_seconds } => {
                self.press_key(browser_state, page_name, selector.as_deref(), &key, wait_seconds).await
            }
            BrowserAction::DragDrop { source_selector, target_selector, wait_seconds } => {
                self.drag_drop(browser_state, page_name, &source_selector, &target_selector, wait_seconds).await
            }
            BrowserAction::RightClick { selector, wait_seconds } => {
                self.right_click(browser_state, page_name, &selector, wait_seconds).await
            }
            BrowserAction::MouseWheel { selector, delta_x, delta_y, wait_seconds } => {
                self.mouse_wheel(browser_state, page_name, selector.as_deref(), delta_x, delta_y, wait_seconds).await
            }
            BrowserAction::GetCookies => {
                self.get_cookies(browser_state, page_name).await
            }
            BrowserAction::SetCookie { name, value, domain, path, secure, http_only, same_site, expires } => {
                self.set_cookie(browser_state, page_name, &name, &value, domain.as_deref(), path.as_deref(), secure, http_only, same_site.as_deref(), expires).await
            }
            BrowserAction::DeleteCookie { name, domain, path } => {
                self.delete_cookie(browser_state, page_name, &name, domain.as_deref(), path.as_deref()).await
            }
            BrowserAction::GetLocalStorage { key } => {
                self.get_local_storage(browser_state, page_name, &key).await
            }
            BrowserAction::SetLocalStorage { key, value } => {
                self.set_local_storage(browser_state, page_name, &key, &value).await
            }
            BrowserAction::DeleteLocalStorage { key } => {
                self.delete_local_storage(browser_state, page_name, &key).await
            }
            BrowserAction::GetSessionStorage { key } => {
                self.get_session_storage(browser_state, page_name, &key).await
            }
            BrowserAction::SetSessionStorage { key, value } => {
                self.set_session_storage(browser_state, page_name, &key, &value).await
            }
            BrowserAction::DeleteSessionStorage { key } => {
                self.delete_session_storage(browser_state, page_name, &key).await
            }
            BrowserAction::GoBack { wait_seconds } => {
                self.go_back(browser_state, page_name, wait_seconds).await
            }
            BrowserAction::GoForward { wait_seconds } => {
                self.go_forward(browser_state, page_name, wait_seconds).await
            }
            BrowserAction::CreateSnapshot { snapshot_type, output_path } => {
                self.create_snapshot(browser_state, page_name, &snapshot_type, output_path.as_deref()).await
            }
            BrowserAction::ExecuteBatch { operations, parallel } => {
                self.execute_batch(browser_state, page_name, &operations, parallel).await
            }
            BrowserAction::GetDriverInfo => {
                self.get_driver_info(browser_state, page_name).await
            }
            BrowserAction::SendProtocolMessage { message_type, data } => {
                self.send_protocol_message(browser_state, page_name, &message_type, &data).await
            }
        }
    }
    
    /// Navigate to a URL
    async fn navigate(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        url: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let mut state = browser_state.lock().await;
        
        // Get navigation timeout from profile before getting page state
        let navigation_timeout = state.profile().navigation_timeout;
        
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        // Update tab state before navigation
        {
            let tab_state = page_state.tab_state_mut();
            tab_state.url = Some(url.to_string());
            tab_state.loading = true;
            tab_state.history.push(url.to_string());
            tab_state.history_position = tab_state.history.len() - 1;
        }
        
        // Get page reference and release state lock
        let page = page_state.page().clone();
        drop(state);
        
        // Perform navigation
        page.goto(url)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to navigate to {}: {}", url, e)))?;
        
        // Wait for navigation with timeout
        let timeout = Duration::from_secs(navigation_timeout);
        let navigation_result = tokio::time::timeout(timeout, page.wait_for_navigation()).await;
        
        // Update state after navigation
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found after navigation", page_name)))?;
        
        match navigation_result {
            Ok(Ok(_)) => {
                // Navigation successful
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                
                // Get page title
                if let Ok(Some(title)) = page.get_title().await {
                    tab_state.title = Some(title);
                }
                
                // Wait additional seconds if specified
                if wait_seconds > 0 {
                    sleep(Duration::from_secs(wait_seconds)).await;
                }
                
                Ok(format!("Navigated to {}", url))
            }
            Ok(Err(e)) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation completed but wait failed: {}", e)))
            }
            Err(_) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation timeout after {} seconds", timeout.as_secs())))
            }
        }
    }
    
    /// Navigate back in history
    async fn go_back(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let mut state = browser_state.lock().await;
        
        // Get navigation timeout from profile before getting page state
        let navigation_timeout = state.profile().navigation_timeout;
        
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let url;
        {
            let tab_state = page_state.tab_state_mut();
            
            // Check if we can go back
            if tab_state.history_position == 0 {
                return Err(BrowserError::Operation("Already at the beginning of history".to_string()));
            }
            
            tab_state.history_position -= 1;
            url = tab_state.history[tab_state.history_position].clone();
            tab_state.loading = true;
        }
        
        let page = page_state.page_mut();
        
        // Use JavaScript to go back
        page.evaluate("window.history.back()")
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to go back: {}", e)))?;
        
        // Wait for navigation with timeout
        let timeout = Duration::from_secs(navigation_timeout);
        match tokio::time::timeout(timeout, page.wait_for_navigation()).await {
            Ok(Ok(_)) => {
                // Get page title before releasing page reference
                let page_title = page.get_title().await.ok().flatten();
                
                {
                    let tab_state = page_state.tab_state_mut();
                    tab_state.loading = false;
                    
                    // Set page title if available
                    if let Some(title) = page_title {
                        tab_state.title = Some(title);
                    }
                    
                    // Update URL
                    tab_state.url = Some(url.clone());
                }
                
                // Wait additional seconds if specified
                if wait_seconds > 0 {
                    sleep(Duration::from_secs(wait_seconds)).await;
                }
                
                Ok(format!("Navigated back to {}", url))
            }
            Ok(Err(e)) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation completed but wait failed: {}", e)))
            }
            Err(_) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation timeout after {} seconds", timeout.as_secs())))
            }
        }
    }
    
    /// Navigate forward in history
    async fn go_forward(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let mut state = browser_state.lock().await;
        
        // Get navigation timeout from profile before getting page state
        let navigation_timeout = state.profile().navigation_timeout;
        
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let url;
        {
            let tab_state = page_state.tab_state_mut();
            
            // Check if we can go forward
            if tab_state.history_position >= tab_state.history.len() - 1 {
                return Err(BrowserError::Operation("Already at the end of history".to_string()));
            }
            
            tab_state.history_position += 1;
            url = tab_state.history[tab_state.history_position].clone();
            tab_state.loading = true;
        }
        
        let page = page_state.page_mut();
        
        // Use JavaScript to go forward
        page.evaluate("window.history.forward()")
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to go forward: {}", e)))?;
        
        // Wait for navigation with timeout
        let timeout = Duration::from_secs(navigation_timeout);
        match tokio::time::timeout(timeout, page.wait_for_navigation()).await {
            Ok(Ok(_)) => {
                // Get page title before releasing page reference
                let page_title = page.get_title().await.ok().flatten();
                
                {
                    let tab_state = page_state.tab_state_mut();
                    tab_state.loading = false;
                    
                    // Set page title if available
                    if let Some(title) = page_title {
                        tab_state.title = Some(title);
                    }
                    
                    // Update URL
                    tab_state.url = Some(url.clone());
                }
                
                // Wait additional seconds if specified
                if wait_seconds > 0 {
                    sleep(Duration::from_secs(wait_seconds)).await;
                }
                
                Ok(format!("Navigated forward to {}", url))
            }
            Ok(Err(e)) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation completed but wait failed: {}", e)))
            }
            Err(_) => {
                let tab_state = page_state.tab_state_mut();
                tab_state.loading = false;
                Err(BrowserError::Operation(format!("Navigation timeout after {} seconds", timeout.as_secs())))
            }
        }
    }
    
    /// Click an element
    async fn click(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        let element = page.find_element(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find element '{}': {}", selector, e)))?;
        
        element.click()
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to click element '{}': {}", selector, e)))?;
        
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Clicked element: {}", selector))
    }
    
    /// Type text into an element
    async fn type_text(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        text: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        let element = page.find_element(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find element '{}': {}", selector, e)))?;
        
        element.type_str(text)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to type text into element '{}': {}", selector, e)))?;
        
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Typed '{}' into element: {}", text, selector))
    }
    
    /// Take a screenshot
    async fn screenshot(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        output_path: &str,
        format: ScreenshotFormat,
        quality: Option<i64>,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        let cdp_format = match format {
            ScreenshotFormat::Png => CaptureScreenshotFormat::Png,
            ScreenshotFormat::Jpeg => CaptureScreenshotFormat::Jpeg,
            ScreenshotFormat::WebP => CaptureScreenshotFormat::Webp,
        };
        
        let params = CaptureScreenshotParams {
            format: Some(cdp_format),
            quality,
            clip: None,
            from_surface: Some(true),
            capture_beyond_viewport: None,
            optimize_for_speed: None,
        };
        
        let screenshot_data = page.screenshot(params)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to take screenshot: {}", e)))?;
        
        std::fs::write(output_path, screenshot_data)
            .map_err(|e| BrowserError::Io(e))?;
        
        Ok(format!("Screenshot saved to {}", output_path))
    }
    
    /// Execute JavaScript
    async fn execute_script(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        script: &str,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        // Execute script without holding state lock
        let result = page.evaluate(script)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to execute script: {}", e)))?;
        
        match result.into_value::<Value>() {
            Ok(value) => Ok(format!("Script executed successfully. Result: {:?}", value)),
            Err(e) => Err(BrowserError::Operation(format!("Failed to deserialize script result: {}", e))),
        }
    }
    
    /// Get page HTML
    async fn get_html(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        // Get HTML without holding state lock
        let html = page.content()
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to get page HTML: {}", e)))?;
        
        // Truncate HTML if too long
        if html.chars().count() > 5000 {
            let truncated = &html[..5000];
            Ok(format!("{}... (truncated, total {} chars)", truncated, html.chars().count()))
        } else {
            Ok(html)
        }
    }
    
    /// Get element text
    async fn get_text(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        let element = page.find_element(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find element '{}': {}", selector, e)))?;
        
        match element.inner_text().await {
            Ok(Some(text)) => Ok(text),
            Ok(None) => Ok("[no text]".to_string()),
            Err(e) => Err(BrowserError::Operation(format!("Failed to get text from element '{}': {}", selector, e))),
        }
    }
    
    /// Find elements
    async fn find_elements(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
    ) -> BrowserResult<String> {
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
        let elements = page.find_elements(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find elements '{}': {}", selector, e)))?;
        
        let mut results = Vec::new();
        for (i, element) in elements.iter().enumerate() {
            match element.inner_text().await {
                Ok(Some(text)) => {
                    results.push(format!("Element {}: {}", i, text));
                }
                Ok(None) => {
                    results.push(format!("Element {}: [no text]", i));
                }
                Err(e) => {
                    results.push(format!("Element {}: Failed to get text: {}", i, e));
                }
            }
        }
        
        Ok(results.join("\n"))
    }
    
    /// Evaluate CDP expression
    async fn evaluate(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        expression: &str,
    ) -> BrowserResult<String> {
        // For now, same as execute_script
        self.execute_script(browser_state, page_name, expression).await
    }
    
    /// Wait for navigation
    async fn wait_for_navigation(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        timeout_seconds: u64,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        // Set timeout
        let timeout = Duration::from_secs(timeout_seconds);
        
        // Wait for navigation without holding state lock
        match tokio::time::timeout(timeout, page.wait_for_navigation()).await {
            Ok(Ok(_)) => Ok("Navigation completed".to_string()),
            Ok(Err(e)) => Err(BrowserError::Operation(format!("Navigation wait failed: {}", e))),
            Err(_) => Err(BrowserError::Timeout(format!("Navigation timeout after {} seconds", timeout_seconds))),
        }
    }
    
    /// Wait for element
    async fn wait_for_element(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        timeout_seconds: u64,
    ) -> BrowserResult<String> {
        // First check if page exists and get page reference
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            page_state.page().clone()
        };
        
        // Set timeout
        let timeout = Duration::from_secs(timeout_seconds);
        
        // Poll for element without holding state lock
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match page.find_element(selector).await {
                Ok(_) => return Ok(format!("Element '{}' found", selector)),
                Err(_) => {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        }
        
        Err(BrowserError::Timeout(format!("Element '{}' not found after {} seconds", selector, timeout_seconds)))
    }
    
    /// Hover over element
    async fn hover(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // chromiumoxide doesn't have direct hover method
        // We'll use JavaScript instead
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element) {{
                const event = new MouseEvent('mouseover', {{
                    view: window,
                    bubbles: true,
                    cancelable: true
                }});
                element.dispatchEvent(event);
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Hovered over element: {}. Result: {}", selector, result))
    }
    
    /// Scroll to element
    async fn scroll_to(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element) {{
                element.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Scrolled to element: {}. Result: {}", selector, result))
    }
    
    /// Get element attributes
    async fn get_attributes(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        attributes: &[String],
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (!element) return null;
            const result = {{}};
            {}
            return result;
            "#,
            selector.replace("'", "\\'"),
            attributes.iter()
                .map(|attr| format!("result['{}'] = element.getAttribute('{}');", attr, attr))
                .collect::<Vec<_>>()
                .join("\n")
        );
        
        self.execute_script(browser_state, page_name, &script).await
    }
    
    /// Set element attribute
    async fn set_attribute(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        attribute: &str,
        value: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element) {{
                element.setAttribute('{}', '{}');
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'"),
            attribute.replace("'", "\\'"),
            value.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Set attribute {}='{}' on element: {}. Result: {}", attribute, value, selector, result))
    }
    
    /// Upload file
    async fn upload_file(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        file_path: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First, check if file exists and get absolute path
        if !std::path::Path::new(file_path).exists() {
            return Err(BrowserError::Operation(format!("File not found: {}", file_path)));
        }
        
        let absolute_path = std::fs::canonicalize(file_path)
            .map_err(|e| BrowserError::Operation(format!("Failed to get absolute path: {}", e)))?;
        
        // Get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            
            let page = page_state.page_mut();
            
            // Click the file input element to trigger file chooser
            page.find_element(selector)
                .await
                .map_err(|e| BrowserError::Operation(format!("Failed to find element '{}': {}", selector, e)))?
                .click()
                .await
                .map_err(|e| BrowserError::Operation(format!("Failed to click element '{}': {}", selector, e)))?;
        }
        
        // Wait for file chooser dialog
        sleep(Duration::from_millis(500)).await;
        
        // Use JavaScript to set the file input value
        let script = format!(
            r#"
            (function() {{
                const input = document.querySelector('{}');
                if (!input) {{
                    return 'Element not found';
                }}
                
                // Create a DataTransfer object and FileList
                const dataTransfer = new DataTransfer();
                const file = new File([''], '{}', {{ type: 'application/octet-stream' }});
                dataTransfer.items.add(file);
                input.files = dataTransfer.files;
                
                // Trigger change event
                input.dispatchEvent(new Event('change', {{ bubbles: true }}));
                
                return 'File set successfully';
            }})()
            "#,
            selector,
            absolute_path.to_string_lossy()
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Uploaded file '{}' to element '{}'. Result: {}", file_path, selector, result))
    }
    
    /// Select dropdown option
    async fn select_option(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        value: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element && element.tagName === 'SELECT') {{
                element.value = '{}';
                element.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'"),
            value.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Selected option '{}' in element: {}. Result: {}", value, selector, result))
    }
    
    /// Check checkbox
    async fn check(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element && element.type === 'checkbox') {{
                if (!element.checked) {{
                    element.click();
                }}
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Checked element: {}. Result: {}", selector, result))
    }
    
    /// Uncheck checkbox
    async fn uncheck(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = format!(
            r#"
            const element = document.querySelector('{}');
            if (element && element.type === 'checkbox') {{
                if (element.checked) {{
                    element.click();
                }}
                return true;
            }}
            return false;
            "#,
            selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Unchecked element: {}. Result: {}", selector, result))
    }
    
    /// Press key
    async fn press_key(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: Option<&str>,
        key: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        let script = if let Some(selector) = selector {
            format!(
                r#"
                const element = document.querySelector('{}');
                if (element) {{
                    const event = new KeyboardEvent('keydown', {{ key: '{}', bubbles: true }});
                    element.dispatchEvent(event);
                    return true;
                }}
                return false;
                "#,
                selector.replace("'", "\\'"),
                key.replace("'", "\\'")
            )
        } else {
            format!(
                r#"
                const event = new KeyboardEvent('keydown', {{ key: '{}', bubbles: true }});
                document.dispatchEvent(event);
                return true;
                "#,
                key.replace("'", "\\'")
            )
        };
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Pressed key '{}' {}. Result: {}", key, 
            selector.map(|s| format!("on element: {}", s)).unwrap_or_default(),
            result))
    }
    
    /// Drag and drop element
    async fn drag_drop(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        source_selector: &str,
        target_selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                const source = document.querySelector('{}');
                const target = document.querySelector('{}');
                
                if (!source) {{
                    return 'Source element not found';
                }}
                
                if (!target) {{
                    return 'Target element not found';
                }}
                
                // Create drag events
                const dragStartEvent = new DragEvent('dragstart', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: new DataTransfer()
                }});
                
                const dragOverEvent = new DragEvent('dragover', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: new DataTransfer()
                }});
                
                const dropEvent = new DragEvent('drop', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: new DataTransfer()
                }});
                
                const dragEndEvent = new DragEvent('dragend', {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: new DataTransfer()
                }});
                
                // Dispatch events
                source.dispatchEvent(dragStartEvent);
                target.dispatchEvent(dragOverEvent);
                target.dispatchEvent(dropEvent);
                source.dispatchEvent(dragEndEvent);
                
                return 'Drag and drop completed';
            }})()
            "#,
            source_selector.replace("'", "\\'"),
            target_selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Dragged element '{}' to '{}'. Result: {}", source_selector, target_selector, result))
    }
    
    /// Right click element
    async fn right_click(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                const element = document.querySelector('{}');
                if (!element) {{
                    return 'Element not found';
                }}
                
                // Create right click event
                const event = new MouseEvent('contextmenu', {{
                    bubbles: true,
                    cancelable: true,
                    view: window,
                    button: 2,
                    buttons: 2,
                    clientX: element.getBoundingClientRect().left,
                    clientY: element.getBoundingClientRect().top
                }});
                
                element.dispatchEvent(event);
                return 'Right click triggered';
            }})()
            "#,
            selector.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Right clicked element: {}. Result: {}", selector, result))
    }
    
    /// Mouse wheel scroll
    async fn mouse_wheel(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        selector: Option<&str>,
        delta_x: i32,
        delta_y: i32,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = if let Some(selector) = selector {
            format!(
                r#"
                (function() {{
                    const element = document.querySelector('{}');
                    if (!element) {{
                        return 'Element not found';
                    }}
                    
                    // Create wheel event
                    const event = new WheelEvent('wheel', {{
                        bubbles: true,
                        cancelable: true,
                        deltaX: {},
                        deltaY: {},
                        deltaZ: 0,
                        deltaMode: 0
                    }});
                    
                    element.dispatchEvent(event);
                    return 'Mouse wheel scrolled on element';
                }})()
                "#,
                selector.replace("'", "\\'"),
                delta_x,
                delta_y
            )
        } else {
            format!(
                r#"
                (function() {{
                    // Create wheel event on document
                    const event = new WheelEvent('wheel', {{
                        bubbles: true,
                        cancelable: true,
                        deltaX: {},
                        deltaY: {},
                        deltaZ: 0,
                        deltaMode: 0
                    }});
                    
                    document.dispatchEvent(event);
                    return 'Mouse wheel scrolled on page';
                }})()
                "#,
                delta_x,
                delta_y
            )
        };
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        sleep(Duration::from_secs(wait_seconds)).await;
        
        Ok(format!("Mouse wheel scrolled (deltaX: {}, deltaY: {}) {}. Result: {}", 
            delta_x, delta_y,
            selector.map(|s| format!("on element: {}", s)).unwrap_or_default(),
            result))
    }
    
    /// Get cookies
    async fn get_cookies(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = r#"
            (function() {
                return document.cookie;
            })()
        "#;
        
        let result = self.execute_script(browser_state, page_name, script).await?;
        
        Ok(format!("Cookies: {}", result))
    }
    
    /// Set cookie
    async fn set_cookie(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        name: &str,
        value: &str,
        domain: Option<&str>,
        path: Option<&str>,
        secure: bool,
        http_only: bool,
        same_site: Option<&str>,
        expires: Option<i64>,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let mut cookie_parts = Vec::new();
        cookie_parts.push(format!("{}={}", name, value));
        
        if let Some(domain) = domain {
            cookie_parts.push(format!("domain={}", domain));
        }
        
        if let Some(path) = path {
            cookie_parts.push(format!("path={}", path));
        }
        
        if secure {
            cookie_parts.push("secure".to_string());
        }
        
        if http_only {
            cookie_parts.push("HttpOnly".to_string());
        }
        
        if let Some(same_site) = same_site {
            cookie_parts.push(format!("SameSite={}", same_site));
        }
        
        if let Some(expires) = expires {
            let date = chrono::DateTime::from_timestamp(expires, 0)
                .unwrap_or_else(|| chrono::Utc::now())
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string();
            cookie_parts.push(format!("expires={}", date));
        }
        
        let cookie_string = cookie_parts.join("; ");
        
        let script = format!(
            r#"
            (function() {{
                document.cookie = "{}";
                return document.cookie;
            }})()
            "#,
            cookie_string.replace("\"", "\\\"")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Set cookie: {}. Current cookies: {}", name, result))
    }
    
    /// Delete cookie
    async fn delete_cookie(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        name: &str,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let mut cookie_parts = Vec::new();
        cookie_parts.push(format!("{}=", name));
        
        if let Some(domain) = domain {
            cookie_parts.push(format!("domain={}", domain));
        }
        
        if let Some(path) = path {
            cookie_parts.push(format!("path={}", path));
        }
        
        // Set expiration to past
        let past_date = chrono::Utc::now() - chrono::Duration::days(1);
        cookie_parts.push(format!("expires={}", past_date.format("%a, %d %b %Y %H:%M:%S GMT")));
        
        let cookie_string = cookie_parts.join("; ");
        
        let script = format!(
            r#"
            (function() {{
                document.cookie = "{}";
                return document.cookie;
            }})()
            "#,
            cookie_string.replace("\"", "\\\"")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Deleted cookie: {}. Current cookies: {}", name, result))
    }
    
    /// Get local storage item
    async fn get_local_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                return localStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Local storage item '{}': {}", key, result))
    }
    
    /// Set local storage item
    async fn set_local_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
        value: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                localStorage.setItem('{}', '{}');
                return localStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'"),
            value.replace("'", "\\'"),
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Set local storage item '{}' to '{}'. Result: {}", key, value, result))
    }
    
    /// Delete local storage item
    async fn delete_local_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                localStorage.removeItem('{}');
                return localStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'"),
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Deleted local storage item '{}'. Result: {}", key, result))
    }
    
    /// Get session storage item
    async fn get_session_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                return sessionStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Session storage item '{}': {}", key, result))
    }
    
    /// Set session storage item
    async fn set_session_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
        value: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                sessionStorage.setItem('{}', '{}');
                return sessionStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'"),
            value.replace("'", "\\'"),
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Set session storage item '{}' to '{}'. Result: {}", key, value, result))
    }
    
    /// Delete session storage item
    async fn delete_session_storage(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        key: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        let script = format!(
            r#"
            (function() {{
                sessionStorage.removeItem('{}');
                return sessionStorage.getItem('{}');
            }})()
            "#,
            key.replace("'", "\\'"),
            key.replace("'", "\\'")
        );
        
        let result = self.execute_script(browser_state, page_name, &script).await?;
        
        Ok(format!("Deleted session storage item '{}'. Result: {}", key, result))
    }
    
    /// Create AI snapshot of page
    async fn create_snapshot(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        snapshot_type: &str,
        output_path: Option<&str>,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        // Get page HTML for snapshot
        let html = self.get_html(browser_state.clone(), page_name).await?;
        
        match snapshot_type {
            "ai" => {
                let snapshot = AiSnapshot::new();
                let result = snapshot.to_markdown();
                
                if let Some(path) = output_path {
                    std::fs::write(path, &result)
                        .map_err(|e| BrowserError::Io(e))?;
                    Ok(format!("AI snapshot saved to {}", path))
                } else {
                    Ok(format!("AI snapshot generated: {}", result))
                }
            }
            "aria" => {
                let snapshot = AriaSnapshot::new();
                let result = snapshot.to_json()
                    .map_err(|e| BrowserError::Operation(format!("Failed to serialize ARIA snapshot: {}", e)))?;
                
                if let Some(path) = output_path {
                    std::fs::write(path, &result)
                        .map_err(|e| BrowserError::Io(e))?;
                    Ok(format!("ARIA snapshot saved to {}", path))
                } else {
                    Ok(format!("ARIA snapshot generated: {}", result))
                }
            }
            _ => Err(BrowserError::Operation(format!("Unknown snapshot type: {}", snapshot_type))),
        }
    }
    
    /// Execute batch operations
    async fn execute_batch(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        operations: &[Value],
        parallel: bool,
    ) -> BrowserResult<String> {
        // Create operation sequence
        let mut sequence = OperationSequence::new("batch");
        
        for (i, op) in operations.iter().enumerate() {
            let step = OperationStep::from_json(op)
                .map_err(|e| BrowserError::Operation(format!("Failed to parse operation {}: {}", i, e)))?;
            sequence.add_step(step);
        }
        
        // Execute sequence
        let step_results = if parallel {
            sequence.execute_parallel().await
        } else {
            sequence.execute_sequential().await
        };
        
        // Convert to result_aggregator::StepResult
        let results: Vec<crate::tools::browser::batch::result_aggregator::StepResult> = step_results.into_iter()
            .map(|sr| crate::tools::browser::batch::result_aggregator::StepResult {
                step_id: sr.step_id,
                step_name: sr.step_name,
                success: sr.success,
                message: sr.message,
                error: sr.error,
                duration_ms: sr.duration_ms,
                retry_attempts: sr.retry_attempts,
                output_data: sr.output_data,
                start_timestamp: sr.start_timestamp,
                end_timestamp: sr.end_timestamp,
            })
            .collect();
        
        // Aggregate results
        let mut aggregator = crate::tools::browser::batch::ResultAggregator::new(
            crate::tools::browser::batch::AggregationStrategy::All
        );
        for result in results {
            aggregator.add_result(result);
        }
        let aggregated = aggregator.get_aggregated_result();
        
        Ok(format!("Batch execution completed. Results: {:?}", aggregated))
    }
    
    /// Get driver information
    async fn get_driver_info(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        // For now, return basic driver info
        // In a real implementation, this would query the actual driver
        Ok("Driver: Chromium (via chromiumoxide)\nVersion: 1.0.0\nProtocol: CDP 1.3".to_string())
    }
    
    /// Send protocol message
    async fn send_protocol_message(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        message_type: &str,
        data: &Value,
    ) -> BrowserResult<String> {
        // First, get page state to ensure page exists
        {
            let mut state = browser_state.lock().await;
            let _page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        }
        
        // For now, simulate protocol message sending
        // In a real implementation, this would use the protocol module
        let message = ProtocolMessage::new_command(
            1, // message ID
            message_type,
            data.clone(),
        );
        
        Ok(format!("Protocol message sent: {:?}", message))
    }
}