use super::{Tool, ToolDefinition, ToolResult};
use chromiumoxide::browser::{Browser, BrowserConfig, HeadlessMode};
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, CaptureScreenshotParams};
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::page::Page;
use futures::StreamExt;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// Browser state that holds the browser and page
struct BrowserState {
    browser: Browser,
    page: Page,
}

impl BrowserState {
    /// Create a new browser instance
    async fn new(headless: bool) -> Result<Self, String> {
        let config = BrowserConfig::builder()
            .viewport(Viewport {
                width: 1920,
                height: 1080,
                device_scale_factor: None,
                emulating_mobile: false,
                has_touch: false,
                is_landscape: false,
            })
            .headless_mode(if headless { HeadlessMode::True } else { HeadlessMode::False })
            .build()
            .map_err(|e| format!("Failed to build browser config: {}", e))?;
        
        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| format!("Failed to launch browser: {}", e))?;
        
        // Spawn browser handler task
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                match event {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Browser handler error: {}", e);
                        break;
                    }
                }
            }
        });
        
        // Create new page
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| format!("Failed to create page: {}", e))?;
        
        Ok(BrowserState { browser, page })
    }
    
    /// Close the browser
    async fn close(mut self) -> Result<(), String> {
        self.browser
            .close()
            .await
            .map_err(|e| format!("Failed to close browser: {}", e))?;
        Ok(())
    }
}

/// Browser tool that manages a browser session
pub struct BrowserTool {
    state: Arc<AsyncMutex<Option<BrowserState>>>,
}

impl BrowserTool {
    pub fn new() -> Self {
        BrowserTool {
            state: Arc::new(AsyncMutex::new(None)),
        }
    }
    
    /// Ensure browser is initialized
    async fn ensure_browser(&self, headless: bool) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if state.is_none() {
            *state = Some(BrowserState::new(headless).await?);
        }
        Ok(())
    }
    
    /// Get browser state (must be called after ensure_browser)
    async fn get_state(&self) -> Result<BrowserStateHandle, String> {
        let state = self.state.lock().await;
        if let Some(_) = &*state {
            Ok(BrowserStateHandle { _lock: state })
        } else {
            Err("Browser not initialized".to_string())
        }
    }
}

/// Handle that holds a lock to the browser state
struct BrowserStateHandle<'a> {
    _lock: tokio::sync::MutexGuard<'a, Option<BrowserState>>,
}

impl<'a> BrowserStateHandle<'a> {
    /// Get mutable reference to browser state
    fn state_mut(&mut self) -> &mut BrowserState {
        self._lock.as_mut().unwrap()
    }
}

#[async_trait::async_trait]
impl Tool for BrowserTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser".to_string(),
            description: "Control a web browser for automation tasks using Chrome DevTools Protocol (CDP). Supports navigation, clicking, form filling, screenshot capture, and more.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["navigate", "click", "type", "screenshot", "execute_script", "get_html", "get_text", "find_elements", "quit", "evaluate"],
                        "description": "The browser action to perform"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL to navigate to (required for navigate action)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for element (required for click, type, get_text, find_elements actions)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into element (required for type action)"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Path to save screenshot (optional for screenshot action)"
                    },
                    "script": {
                        "type": "string",
                        "description": "JavaScript to execute (required for execute_script action)"
                    },
                    "headless": {
                        "type": "boolean",
                        "description": "Whether to run browser in headless mode (default: true)"
                    },
                    "wait_seconds": {
                        "type": "integer",
                        "description": "Seconds to wait after action (default: 2)"
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, input: &Value, _project_dir: &Path) -> ToolResult {
        let action = match input.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::error("Missing required parameter: action"),
        };
        
        let headless = input
            .get("headless")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        
        let wait_seconds = input
            .get("wait_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(2);
        
        // Handle quit action
        if action == "quit" {
            let mut state = self.state.lock().await;
            if let Some(browser_state) = state.take() {
                match browser_state.close().await {
                    Ok(_) => return ToolResult::success("Browser closed successfully"),
                    Err(e) => return ToolResult::error(format!("Failed to close browser: {}", e)),
                }
            } else {
                return ToolResult::success("No browser session to close");
            }
        }
        
        // Ensure browser is initialized
        if let Err(e) = self.ensure_browser(headless).await {
            return ToolResult::error(format!("Failed to initialize browser: {}", e));
        }
        
        // Get browser state handle
        let mut handle = match self.get_state().await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(e),
        };
        
        let state = handle.state_mut();
        
        match action {
            "navigate" => {
                let url = match input.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u,
                    None => return ToolResult::error("Missing required parameter: url for navigate action"),
                };
                
                match state.page.goto(url).await {
                    Ok(_) => {
                        // Wait for navigation
                        tokio::time::sleep(tokio::time::Duration::from_secs(wait_seconds)).await;
                        match state.page.wait_for_navigation().await {
                            Ok(_) => ToolResult::success(format!("Navigated to {}", url)),
                            Err(e) => ToolResult::error(format!("Navigation completed but wait failed: {}", e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to navigate to {}: {}", url, e)),
                }
            }
            
            "click" => {
                let selector = match input.get("selector").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: selector for click action"),
                };
                
                match state.page.find_element(selector).await {
                    Ok(element) => {
                        match element.click().await {
                            Ok(_) => {
                                tokio::time::sleep(tokio::time::Duration::from_secs(wait_seconds)).await;
                                ToolResult::success(format!("Clicked element: {}", selector))
                            }
                            Err(e) => ToolResult::error(format!("Failed to click element '{}': {}", selector, e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to find element '{}': {}", selector, e)),
                }
            }
            
            "type" => {
                let selector = match input.get("selector").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: selector for type action"),
                };
                
                let text = match input.get("text").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => return ToolResult::error("Missing required parameter: text for type action"),
                };
                
                match state.page.find_element(selector).await {
                    Ok(element) => {
                        match element.type_str(text).await {
                            Ok(_) => {
                                tokio::time::sleep(tokio::time::Duration::from_secs(wait_seconds)).await;
                                ToolResult::success(format!("Typed '{}' into element: {}", text, selector))
                            }
                            Err(e) => ToolResult::error(format!("Failed to type text into element '{}': {}", selector, e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to find element '{}': {}", selector, e)),
                }
            }
            
            "screenshot" => {
                let output_path = input
                    .get("output_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("screenshot.png");
                
                let params = CaptureScreenshotParams {
                    format: Some(CaptureScreenshotFormat::Png),
                    quality: None,
                    clip: None,
                    from_surface: Some(true),
                    capture_beyond_viewport: None,
                };
                
                match state.page.screenshot(params).await {
                    Ok(screenshot) => {
                        match std::fs::write(output_path, screenshot) {
                            Ok(_) => ToolResult::success(format!("Screenshot saved to {}", output_path)),
                            Err(e) => ToolResult::error(format!("Failed to save screenshot to {}: {}", output_path, e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to take screenshot: {}", e)),
                }
            }
            
            "execute_script" => {
                let script = match input.get("script").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: script for execute_script action"),
                };
                
                match state.page.evaluate(script).await {
                    Ok(result) => {
                        match result.into_value::<serde_json::Value>() {
                            Ok(value) => ToolResult::success(format!("Script executed successfully. Result: {:?}", value)),
                            Err(e) => ToolResult::error(format!("Failed to deserialize script result: {}", e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to execute script: {}", e)),
                }
            }
            
            "get_html" => {
                match state.page.content().await {
                    Ok(html) => {
                        // Truncate HTML if too long
                        let truncated = if html.len() > 5000 {
                            format!("{}... (truncated, total {} chars)", &html[..5000], html.len())
                        } else {
                            html
                        };
                        ToolResult::success(truncated)
                    }
                    Err(e) => ToolResult::error(format!("Failed to get page HTML: {}", e)),
                }
            }
            
            "get_text" => {
                let selector = match input.get("selector").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: selector for get_text action"),
                };
                
                match state.page.find_element(selector).await {
                    Ok(element) => {
                        match element.inner_text().await {
                            Ok(Some(text)) => ToolResult::success(text),
                            Ok(None) => ToolResult::success("Element has no text".to_string()),
                            Err(e) => ToolResult::error(format!("Failed to get text from element '{}': {}", selector, e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to find element '{}': {}", selector, e)),
                }
            }
            
            "find_elements" => {
                let selector = match input.get("selector").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: selector for find_elements action"),
                };
                
                match state.page.find_elements(selector).await {
                    Ok(elements) => {
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
                        ToolResult::success(results.join("\n"))
                    }
                    Err(e) => ToolResult::error(format!("Failed to find elements '{}': {}", selector, e)),
                }
            }
            
            "evaluate" => {
                // Generic CDP evaluation - same as execute_script for now
                let script = match input.get("script").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::error("Missing required parameter: script for evaluate action"),
                };
                
                match state.page.evaluate(script).await {
                    Ok(result) => {
                        match result.into_value::<serde_json::Value>() {
                            Ok(value) => ToolResult::success(format!("CDP evaluation successful. Result: {:?}", value)),
                            Err(e) => ToolResult::error(format!("Failed to deserialize evaluation result: {}", e)),
                        }
                    }
                    Err(e) => ToolResult::error(format!("Failed to evaluate script: {}", e)),
                }
            }
            
            _ => ToolResult::error(format!("Unknown action: {}", action)),
        }
    }
}