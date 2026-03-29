use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::BrowserState;
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
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
        page.goto(url)
            .await
            .map_err(|e| BrowserError::Operation(format!("Failed to navigate to {}: {}", url, e)))?;
        
        // Wait for navigation
        sleep(Duration::from_secs(wait_seconds)).await;
        
        match page.wait_for_navigation().await {
            Ok(_) => Ok(format!("Navigated to {}", url)),
            Err(e) => Err(BrowserError::Operation(format!("Navigation completed but wait failed: {}", e))),
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
        // Set timeout
        let timeout = Duration::from_secs(timeout_seconds);
        
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
        let mut state = browser_state.lock().await;
        let page_state = state.get_page(page_name)
            .ok_or_else(|| BrowserError::NotFound(format!("Page '{}' not found", page_name)))?;
        
        let page = page_state.page_mut();
        
        // Set timeout
        let timeout = Duration::from_secs(timeout_seconds);
        
        // Poll for element
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
        // chromiumoxide doesn't have direct file upload method
        // We'll need to implement this using CDP
        // For now, return not implemented
        Err(BrowserError::Operation("File upload not yet implemented".to_string()))
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
}