use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::BrowserState;
use crate::tools::browser::snapshot::{AiSnapshot, AriaSnapshot};

use crate::tools::browser::protocol::ProtocolMessage;
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, CaptureScreenshotParams};
use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;
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
    pub fn execute<'a>(
        &'a self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &'a str,
        action: BrowserAction,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = BrowserResult<String>> + Send + 'a>> {
        Box::pin(async move {
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
        }) // close Box::pin(async move {
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
        // Collect what we need under the lock, then drop it before async operations.
        let (page, navigation_timeout) = {
            let mut state = browser_state.lock().await;
            let navigation_timeout = state.profile().navigation_timeout;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            // Mark as loading
            page_state.tab_state_mut().loading = true;
            (page_state.page().clone(), navigation_timeout)
        };

        // Perform navigation without holding the lock.
        page.evaluate("window.history.back()")
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to go back: {}", e)))?;

        let timeout = Duration::from_secs(navigation_timeout);
        let nav_result = tokio::time::timeout(timeout, page.wait_for_navigation()).await;

        // Read actual URL and title from the browser.
        let actual_url = page.evaluate("window.location.href")
            .await.ok()
            .and_then(|r| r.into_value::<String>().ok());
        let actual_title = page.get_title().await.ok().flatten();

        // Update state with real values.
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found after go_back", page_name)))?;
        let tab_state = page_state.tab_state_mut();
        tab_state.loading = false;
        if let Some(url) = actual_url.clone() { tab_state.url = Some(url); }
        if let Some(title) = actual_title { tab_state.title = Some(title); }
        // Keep history position consistent (decrement if possible)
        if tab_state.history_position > 0 { tab_state.history_position -= 1; }
        drop(state);

        match nav_result {
            Ok(Ok(_)) | Ok(Err(_)) => {
                if wait_seconds > 0 { sleep(Duration::from_secs(wait_seconds)).await; }
                let url_display = actual_url.unwrap_or_else(|| "(unknown)".to_string());
                Ok(format!("Navigated back to {}", url_display))
            }
            Err(_) => Err(BrowserError::Timeout(
                format!("go_back timeout after {} seconds", navigation_timeout)
            )),
        }
    }
    
    /// Navigate forward in history
    async fn go_forward(
        &self,
        browser_state: Arc<Mutex<BrowserState>>,
        page_name: &str,
        wait_seconds: u64,
    ) -> BrowserResult<String> {
        // Collect what we need under the lock, then drop it before async operations.
        let (page, navigation_timeout) = {
            let mut state = browser_state.lock().await;
            let navigation_timeout = state.profile().navigation_timeout;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            page_state.tab_state_mut().loading = true;
            (page_state.page().clone(), navigation_timeout)
        };

        // Perform navigation without holding the lock.
        page.evaluate("window.history.forward()")
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to go forward: {}", e)))?;

        let timeout = Duration::from_secs(navigation_timeout);
        let nav_result = tokio::time::timeout(timeout, page.wait_for_navigation()).await;

        // Read actual URL and title from the browser.
        let actual_url = page.evaluate("window.location.href")
            .await.ok()
            .and_then(|r| r.into_value::<String>().ok());
        let actual_title = page.get_title().await.ok().flatten();

        // Update state with real values.
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found after go_forward", page_name)))?;
        let tab_state = page_state.tab_state_mut();
        tab_state.loading = false;
        if let Some(url) = actual_url.clone() { tab_state.url = Some(url); }
        if let Some(title) = actual_title { tab_state.title = Some(title); }
        // Keep history position consistent (advance if possible)
        if tab_state.history_position + 1 < tab_state.history.len() {
            tab_state.history_position += 1;
        }
        drop(state);

        match nav_result {
            Ok(Ok(_)) | Ok(Err(_)) => {
                if wait_seconds > 0 { sleep(Duration::from_secs(wait_seconds)).await; }
                let url_display = actual_url.unwrap_or_else(|| "(unknown)".to_string());
                Ok(format!("Navigated forward to {}", url_display))
            }
            Err(_) => Err(BrowserError::Timeout(
                format!("go_forward timeout after {} seconds", navigation_timeout)
            )),
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
        // Use chromiumoxide's native hover which dispatches real CDP mouse events,
        // triggering CSS :hover and pointer-event handlers correctly.
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            page_state.page().clone()
        };

        let element = page.find_element(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find element '{}': {}", selector, e)))?;

        element.hover()
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to hover over element '{}': {}", selector, e)))?;

        if wait_seconds > 0 {
            sleep(Duration::from_secs(wait_seconds)).await;
        }

        Ok(format!("Hovered over element: {}", selector))
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
        if !std::path::Path::new(file_path).exists() {
            return Err(BrowserError::Operation(format!("File not found: {}", file_path)));
        }

        let absolute_path = std::fs::canonicalize(file_path)
            .map_err(|e| BrowserError::Operation(format!("Failed to resolve file path: {}", e)))?;

        // Clone page reference so we don't hold the lock during async CDP calls.
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            page_state.page().clone()
        };

        let element = page.find_element(selector)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to find file input '{}': {}", selector, e)))?;

        // Use CDP DOM.setFileInputFiles to set real file bytes via backend_node_id.
        let backend_node_id = element.backend_node_id;
        let params = SetFileInputFilesParams::builder()
            .files(vec![absolute_path.to_string_lossy().into_owned()])
            .backend_node_id(backend_node_id)
            .build()
            .map_err(|e| BrowserError::Operation(format!("Failed to build SetFileInputFiles params: {}", e)))?;
        page.execute(params)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to set file on input '{}': {}", selector, e)))?;

        if wait_seconds > 0 {
            sleep(Duration::from_secs(wait_seconds)).await;
        }

        Ok(format!("Uploaded '{}' to input '{}'", absolute_path.display(), selector))
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
        let page = {
            let mut state = browser_state.lock().await;
            let page_state = state.get_page(page_name)
                .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
            page_state.page().clone()
        };

        match snapshot_type {
            "ai" => {
                // Extract page structure via JavaScript and parse into AiSnapshot.
                let script = r#"
(function() {
    var ts = Math.floor(Date.now() / 1000);
    var result = {
        title: document.title || null,
        url: window.location.href || null,
        main_content: '',
        structure: [],
        interactive_elements: [],
        images: [],
        tables: [],
        metadata: {},
        accessibility_score: 100,
        semantic_elements: [],
        timestamp: ts
    };

    // Meta tags
    document.querySelectorAll('meta[name], meta[property]').forEach(function(m) {
        var n = m.getAttribute('name') || m.getAttribute('property');
        var c = m.getAttribute('content');
        if (n && c) result.metadata[n] = c.substring(0, 200);
    });

    // Main content text
    var mainEl = document.querySelector('main, [role="main"], article, #content, .content, .main');
    result.main_content = ((mainEl || document.body || document.documentElement)
        .textContent || '').trim().replace(/\s+/g, ' ').substring(0, 3000);

    // Heading-based structure
    document.querySelectorAll('h1,h2,h3,h4,h5,h6').forEach(function(h) {
        var level = parseInt(h.tagName[1], 10);
        var nextSib = h.nextElementSibling;
        var content = nextSib ? (nextSib.textContent || '').trim().replace(/\s+/g, ' ').substring(0, 500) : '';
        result.structure.push({
            heading_level: level,
            heading_text: (h.textContent || '').trim().substring(0, 200),
            content: content,
            id: h.id || null,
            aria_label: h.getAttribute('aria-label') || null
        });
    });

    // Interactive elements (capped at 50)
    var seen = 0;
    document.querySelectorAll('a[href],button,input:not([type="hidden"]),select,textarea,[role="button"],[role="link"],[role="menuitem"]').forEach(function(el) {
        if (seen >= 50) return;
        seen++;
        var rect = el.getBoundingClientRect();
        var label = el.getAttribute('aria-label') ||
            el.getAttribute('placeholder') || el.getAttribute('title') ||
            (el.textContent || '').trim().substring(0, 100) || el.getAttribute('value') || null;
        var sel = el.id ? '#' + el.id :
            (el.name ? el.tagName.toLowerCase() + '[name="' + el.name + '"]' :
            el.tagName.toLowerCase() + (el.className ? '.' + el.className.trim().split(/\s+/)[0] : ''));
        result.interactive_elements.push({
            element_type: el.tagName.toLowerCase(),
            label: label,
            selector: sel,
            aria_role: el.getAttribute('role') || null,
            aria_label: el.getAttribute('aria-label') || null,
            visible: rect.width > 0 && rect.height > 0,
            enabled: !el.disabled
        });
    });

    // Images (capped at 20)
    seen = 0;
    document.querySelectorAll('img').forEach(function(img) {
        if (seen >= 20) return;
        seen++;
        var alt = img.getAttribute('alt');
        result.images.push({
            src: img.src || img.getAttribute('src') || '',
            alt: alt !== null ? alt : null,
            dimensions: (img.naturalWidth > 0 && img.naturalHeight > 0) ? [img.naturalWidth, img.naturalHeight] : null,
            title: img.getAttribute('title') || null,
            decorative: alt === '' || img.getAttribute('role') === 'presentation'
        });
    });

    // Tables (capped at 5)
    seen = 0;
    document.querySelectorAll('table').forEach(function(table) {
        if (seen >= 5) return;
        seen++;
        var rows = table.querySelectorAll('tr');
        var headers = [];
        var data = [];
        rows.forEach(function(row, ri) {
            var cells = row.querySelectorAll('th,td');
            var rowData = [];
            cells.forEach(function(cell) {
                var text = (cell.textContent || '').trim().substring(0, 100);
                if (cell.tagName === 'TH' || ri === 0) { headers.push(text); }
                else { rowData.push(text); }
            });
            if (rowData.length > 0 && ri > 0) data.push(rowData);
        });
        var caption = table.querySelector('caption');
        result.tables.push({
            caption: caption ? (caption.textContent || '').trim() : null,
            rows: rows.length,
            columns: rows.length > 0 ? rows[0].querySelectorAll('th,td').length : 0,
            headers: headers.slice(0, 10),
            data: data.slice(0, 5),
            has_proper_markup: table.querySelector('th') !== null
        });
    });

    // Semantic elements
    ['header','footer','nav','main','article','section','aside'].forEach(function(tag) {
        var els = document.querySelectorAll(tag);
        for (var i = 0; i < Math.min(els.length, 3); i++) {
            var el = els[i];
            result.semantic_elements.push({
                tag: tag,
                content: (el.textContent || '').trim().replace(/\s+/g, ' ').substring(0, 200),
                aria_role: el.getAttribute('role') || null,
                aria_label: el.getAttribute('aria-label') || null
            });
        }
    });

    // Simple accessibility score
    var score = 100;
    score -= document.querySelectorAll('img:not([alt])').length * 5;
    score -= document.querySelectorAll('input:not([id]):not([aria-label]):not([placeholder])').length * 3;
    score -= document.querySelectorAll('button:empty:not([aria-label])').length * 4;
    result.accessibility_score = Math.max(0, Math.min(100, score));

    return JSON.stringify(result);
})()
"#;
                let eval_result = page.evaluate(script)
                    .await
                    .map_err(|e| BrowserError::Operation(format!("Failed to extract page snapshot: {}", e)))?;

                let json_str: String = eval_result
                    .into_value()
                    .map_err(|e| BrowserError::Operation(format!("Snapshot JS did not return a string: {}", e)))?;

                let mut snapshot: AiSnapshot = serde_json::from_str(&json_str)
                    .map_err(|e| BrowserError::Operation(format!("Failed to parse AI snapshot JSON: {}", e)))?;

                // Ensure timestamp is fresh
                snapshot.timestamp = chrono::Utc::now().timestamp();

                let result = snapshot.to_markdown();

                if let Some(path) = output_path {
                    std::fs::write(path, &result).map_err(BrowserError::Io)?;
                    Ok(format!("AI snapshot saved to {} ({} chars, {} sections, {} interactive elements)",
                        path, result.len(), snapshot.structure.len(), snapshot.interactive_elements.len()))
                } else {
                    Ok(result)
                }
            }

            "aria" => {
                let script = r#"
(function() {
    var result = {
        page_title: document.title || null,
        landmarks: [],
        roles: [],
        properties: [],
        live_regions: [],
        focusable_elements: [],
        tab_order: [],
        violations: [],
        aria_tree: [],
        announcements: []
    };

    // Landmarks
    var landmarkMap = { HEADER:'banner', FOOTER:'contentinfo', MAIN:'main', NAV:'navigation', ASIDE:'complementary', FORM:'form' };
    var validLandmarks = ['banner','main','navigation','contentinfo','complementary','search','form','region'];
    document.querySelectorAll('header,main,nav,footer,aside,form,section,[role]').forEach(function(el) {
        var role = el.getAttribute('role') || (landmarkMap[el.tagName] || '');
        if (!validLandmarks.includes(role)) return;
        var rect = el.getBoundingClientRect();
        result.landmarks.push({
            landmark_type: role,
            label: el.getAttribute('aria-label') || el.getAttribute('title') || null,
            selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
            unique: true,
            position: [rect.left, rect.top, rect.width, rect.height]
        });
    });

    // ARIA roles summary
    var roleCount = {};
    document.querySelectorAll('[role]').forEach(function(el) {
        var r = el.getAttribute('role');
        if (r) roleCount[r] = (roleCount[r] || 0) + 1;
    });
    Object.keys(roleCount).forEach(function(role) {
        result.roles.push({ role: role, selector: '[role="' + role + '"]', description: null, valid: true, count: roleCount[role] });
    });

    // ARIA properties (10 per attribute type)
    ['aria-label','aria-describedby','aria-labelledby','aria-hidden','aria-expanded',
     'aria-selected','aria-checked','aria-disabled','aria-live','aria-atomic'].forEach(function(attr) {
        var els = document.querySelectorAll('[' + attr + ']');
        for (var i = 0; i < Math.min(els.length, 10); i++) {
            var el = els[i];
            result.properties.push({
                name: attr,
                value: el.getAttribute(attr) || '',
                selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
                valid: true,
                property_type: 'aria'
            });
        }
    });

    // Live regions
    document.querySelectorAll('[aria-live]').forEach(function(el) {
        result.live_regions.push({
            live_type: el.getAttribute('aria-live') || 'polite',
            selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
            atomic: el.getAttribute('aria-atomic') === 'true',
            relevant: el.getAttribute('aria-relevant') || null,
            content: (el.textContent || '').trim().substring(0, 200)
        });
    });

    // Focusable elements (capped at 30)
    var focusableSel = 'a[href],button:not([disabled]),input:not([disabled]):not([type="hidden"]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
    var seen = 0;
    document.querySelectorAll(focusableSel).forEach(function(el) {
        if (seen >= 30) return;
        seen++;
        result.focusable_elements.push({
            element_type: el.tagName.toLowerCase(),
            selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
            tab_index: typeof el.tabIndex !== 'undefined' ? el.tabIndex : null,
            focusable_by_default: true,
            has_focus: el === document.activeElement,
            label: el.getAttribute('aria-label') || (el.textContent || '').trim().substring(0, 50) || null
        });
    });

    // Tab order (capped at 20)
    seen = 0;
    document.querySelectorAll(focusableSel).forEach(function(el) {
        if (seen >= 20) return;
        seen++;
        result.tab_order.push({
            position: seen,
            selector: el.id ? '#' + el.id : el.tagName.toLowerCase(),
            element_type: el.tagName.toLowerCase(),
            label: el.getAttribute('aria-label') || (el.textContent || '').trim().substring(0, 50) || null,
            reachable: el.tabIndex >= 0
        });
    });

    // Accessibility violations
    document.querySelectorAll('img:not([alt])').forEach(function(el) {
        result.violations.push({
            violation_type: 'missing_alt_text', wcag_guideline: '1.1.1', severity: 'high',
            selector: el.id ? '#' + el.id : 'img[src="' + (el.src || '').split('/').pop() + '"]',
            description: 'Image missing alt attribute',
            fix: 'Add alt="" for decorative images or a descriptive alt text'
        });
    });
    document.querySelectorAll('button:not([aria-label]):not([aria-labelledby])').forEach(function(el) {
        if (!(el.textContent || '').trim()) {
            result.violations.push({
                violation_type: 'unlabeled_button', wcag_guideline: '4.1.2', severity: 'high',
                selector: el.id ? '#' + el.id : 'button',
                description: 'Button has no accessible label',
                fix: 'Add text content or aria-label to button'
            });
        }
    });
    document.querySelectorAll('input:not([id]):not([aria-label]):not([aria-labelledby])').forEach(function(el) {
        if (el.type !== 'hidden' && el.type !== 'submit' && el.type !== 'button') {
            result.violations.push({
                violation_type: 'input_no_label', wcag_guideline: '1.3.1', severity: 'medium',
                selector: el.name ? 'input[name="' + el.name + '"]' : 'input[type="' + (el.type || 'text') + '"]',
                description: 'Input has no associated label',
                fix: 'Add aria-label or associate input with a <label> element'
            });
        }
    });

    return JSON.stringify(result);
})()
"#;
                let eval_result = page.evaluate(script)
                    .await
                    .map_err(|e| BrowserError::Operation(format!("Failed to extract ARIA snapshot: {}", e)))?;

                let json_str: String = eval_result
                    .into_value()
                    .map_err(|e| BrowserError::Operation(format!("ARIA snapshot JS did not return a string: {}", e)))?;

                let snapshot: AriaSnapshot = serde_json::from_str(&json_str)
                    .map_err(|e| BrowserError::Operation(format!("Failed to parse ARIA snapshot JSON: {}", e)))?;

                let score = snapshot.accessibility_score();
                let summary = snapshot.summary();
                let result_json = snapshot.to_json()
                    .map_err(|e| BrowserError::Operation(format!("Failed to serialize ARIA snapshot: {}", e)))?;

                if let Some(path) = output_path {
                    std::fs::write(path, &result_json).map_err(BrowserError::Io)?;
                    Ok(format!("ARIA snapshot saved to {} (score: {}%, {} violations, {} landmarks)",
                        path, score,
                        summary.get("violations").unwrap_or(&0),
                        summary.get("landmarks").unwrap_or(&0)))
                } else {
                    Ok(format!("ARIA Score: {}%  |  Landmarks: {}  |  Roles: {}  |  Violations: {}\n\n{}",
                        score,
                        summary.get("landmarks").unwrap_or(&0),
                        summary.get("roles").unwrap_or(&0),
                        summary.get("violations").unwrap_or(&0),
                        result_json))
                }
            }

            _ => Err(BrowserError::Operation(
                format!("Unknown snapshot type: '{}'. Use 'ai' or 'aria'.", snapshot_type)
            )),
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
        if operations.is_empty() {
            return Ok("Batch: 0 operations (nothing to do)".to_string());
        }

        // Parse all operations into BrowserActions upfront so we can fail fast on bad input.
        let mut actions: Vec<(usize, BrowserAction)> = Vec::with_capacity(operations.len());
        for (i, op) in operations.iter().enumerate() {
            match Self::batch_op_to_action(op) {
                Ok(action) => actions.push((i, action)),
                Err(e) => return Err(BrowserError::Operation(
                    format!("Operation {} parse error: {}", i, e)
                )),
            }
        }

        let total = actions.len();
        let mut lines: Vec<String> = Vec::with_capacity(total);

        if parallel {
            // Run all actions sequentially but collect ALL results (don't stop on error).
            // True concurrent spawning isn't available here because chromiumoxide futures
            // are not Send; for browser automation on a single page this is also safer.
            let mut ok_count = 0usize;
            for (idx, action) in actions {
                let start = std::time::Instant::now();
                match self.execute(browser_state.clone(), page_name, action).await {
                    Ok(msg) => {
                        ok_count += 1;
                        let ms = start.elapsed().as_millis();
                        lines.push(format!("Step {}: OK ({}ms) — {}", idx + 1, ms,
                            msg.lines().next().unwrap_or("").chars().take(120).collect::<String>()));
                    }
                    Err(e) => {
                        let ms = start.elapsed().as_millis();
                        lines.push(format!("Step {}: ERROR ({}ms) — {}", idx + 1, ms, e));
                    }
                }
            }
            lines.insert(0, format!("Batch parallel: {}/{} succeeded", ok_count, total));
        } else {
            // Sequential: stop on first error by default.
            let mut ok_count = 0usize;
            for (idx, action) in actions {
                let start = std::time::Instant::now();
                match self.execute(browser_state.clone(), page_name, action).await {
                    Ok(msg) => {
                        ok_count += 1;
                        let ms = start.elapsed().as_millis();
                        lines.push(format!("Step {}: OK ({}ms) — {}", idx + 1, ms,
                            msg.lines().next().unwrap_or("").chars().take(120).collect::<String>()));
                    }
                    Err(e) => {
                        let ms = start.elapsed().as_millis();
                        lines.push(format!("Step {}: ERROR ({}ms) — {}", idx + 1, ms, e));
                        lines.insert(0, format!("Batch sequential: {}/{} succeeded (stopped on error)", ok_count, total));
                        return Ok(lines.join("\n"));
                    }
                }
            }
            lines.insert(0, format!("Batch sequential: {}/{} succeeded", ok_count, total));
        }

        Ok(lines.join("\n"))
    }

    /// Convert a batch operation JSON value into a BrowserAction.
    fn batch_op_to_action(op: &Value) -> Result<BrowserAction, String> {
        // Accept both "action" and "action_type" as the discriminator key.
        let action_type = op.get("action")
            .or_else(|| op.get("action_type"))
            .and_then(|v| v.as_str())
            .ok_or("Operation missing 'action' or 'action_type' field")?
            .to_lowercase();

        // Parameters may live inline at the top level or in a nested "parameters" object.
        let params_obj;
        let p: &Value = if let Some(nested) = op.get("parameters").filter(|v| v.is_object()) {
            // Merge top-level keys (except action/action_type) into a temporary Value.
            let mut merged = nested.clone();
            if let (Value::Object(ref mut m), Some(Value::Object(top))) =
                (&mut merged, Some(op.clone()))
            {
                for (k, v) in top {
                    if k != "action" && k != "action_type" && k != "parameters" {
                        m.entry(k).or_insert(v);
                    }
                }
            }
            params_obj = merged;
            &params_obj
        } else {
            op
        };

        let str_field = |key: &str| -> Result<String, String> {
            p.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| format!("Missing field '{}' for action '{}'", key, action_type))
        };
        let u64_field = |key: &str, default: u64| -> u64 {
            p.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
        };

        let fmt_field = |key: &str| -> ScreenshotFormat {
            match p.get(key).and_then(|v| v.as_str()).unwrap_or("png") {
                "jpeg" | "jpg" => ScreenshotFormat::Jpeg,
                "webp" => ScreenshotFormat::WebP,
                _ => ScreenshotFormat::Png,
            }
        };

        match action_type.as_str() {
            "navigate" => Ok(BrowserAction::Navigate {
                url: str_field("url")?,
                wait_seconds: u64_field("wait_seconds", 2),
            }),
            "click" => Ok(BrowserAction::Click {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 1),
            }),
            "type" | "type_text" => Ok(BrowserAction::Type {
                selector: str_field("selector")?,
                text: str_field("text")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "screenshot" | "take_screenshot" => Ok(BrowserAction::Screenshot {
                output_path: p.get("output_path").and_then(|v| v.as_str())
                    .unwrap_or("/tmp/batch_screenshot.png").to_string(),
                format: fmt_field("format"),
                quality: p.get("quality").and_then(|v| v.as_i64()),
            }),
            "execute_script" | "run_script" => Ok(BrowserAction::ExecuteScript {
                script: str_field("script").or_else(|_| str_field("code"))?,
            }),
            "evaluate" => Ok(BrowserAction::Evaluate {
                expression: str_field("expression").or_else(|_| str_field("script"))?,
            }),
            "get_html" | "html" => Ok(BrowserAction::GetHtml),
            "get_text" | "text" => Ok(BrowserAction::GetText {
                selector: str_field("selector")?,
            }),
            "find_elements" | "find" => Ok(BrowserAction::FindElements {
                selector: str_field("selector")?,
            }),
            "hover" => Ok(BrowserAction::Hover {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "scroll_to" | "scroll" => Ok(BrowserAction::ScrollTo {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "wait_for_element" | "wait_element" => Ok(BrowserAction::WaitForElement {
                selector: str_field("selector")?,
                timeout_seconds: u64_field("timeout_seconds", 10),
            }),
            "wait_for_navigation" | "wait_navigation" => Ok(BrowserAction::WaitForNavigation {
                timeout_seconds: u64_field("timeout_seconds", 30),
            }),
            "go_back" | "back" => Ok(BrowserAction::GoBack {
                wait_seconds: u64_field("wait_seconds", 1),
            }),
            "go_forward" | "forward" => Ok(BrowserAction::GoForward {
                wait_seconds: u64_field("wait_seconds", 1),
            }),
            "press_key" | "key" => Ok(BrowserAction::PressKey {
                key: str_field("key")?,
                selector: p.get("selector").and_then(|v| v.as_str()).map(|s| s.to_string()),
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "select_option" | "select" => Ok(BrowserAction::SelectOption {
                selector: str_field("selector")?,
                value: str_field("value")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "check" => Ok(BrowserAction::Check {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "uncheck" => Ok(BrowserAction::Uncheck {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "upload_file" | "upload" => Ok(BrowserAction::UploadFile {
                selector: str_field("selector")?,
                file_path: str_field("file_path").or_else(|_| str_field("path"))?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "right_click" => Ok(BrowserAction::RightClick {
                selector: str_field("selector")?,
                wait_seconds: u64_field("wait_seconds", 0),
            }),
            "get_cookies" | "cookies" => Ok(BrowserAction::GetCookies),
            "create_snapshot" | "snapshot" => Ok(BrowserAction::CreateSnapshot {
                snapshot_type: p.get("snapshot_type").and_then(|v| v.as_str())
                    .unwrap_or("ai").to_string(),
                output_path: p.get("output_path").and_then(|v| v.as_str()).map(|s| s.to_string()),
            }),
            other => Err(format!("Unsupported batch action type: '{}'", other)),
        }
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