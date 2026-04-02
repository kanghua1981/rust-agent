#![allow(dead_code)]

use crate::tools::browser::client::{ActionExecutor, BrowserAction, BrowserSession, ScreenshotFormat, SessionManager};
use crate::tools::browser::config::ConfigManager;
use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::BrowserManager;
use crate::tools::{Tool, ToolDefinition, ToolResult};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Main browser tool that provides the unified interface
pub struct BrowserTool {
    /// Browser manager
    manager: Arc<BrowserManager>,
    /// Config manager
    config_manager: Arc<ConfigManager>,
    /// Action executor
    action_executor: ActionExecutor,
    /// Current session
    session: Arc<Mutex<BrowserSession>>,
    /// Session manager
    session_manager: SessionManager,
}

impl BrowserTool {
    /// Create a new browser tool
    pub fn new() -> Self {
        // Create config manager
        let config_manager = Arc::new(
            ConfigManager::new(std::env::current_dir().unwrap().join(".agent/browser/profiles"))
                .expect("Failed to create config manager")
        );
        
        // Get default profile
        let default_profile = config_manager.get_default_profile().clone();
        
        // Create browser manager
        let manager = Arc::new(BrowserManager::new(default_profile));
        
        // Create session
        let session = Arc::new(Mutex::new(BrowserSession::new(manager.clone())));
        
        // Create session manager
        let session_manager = SessionManager::new(
            std::env::current_dir().unwrap().join(".agent/browser/sessions")
        );
        
        Self {
            manager,
            config_manager,
            action_executor: ActionExecutor,
            session,
            session_manager,
        }
    }
    
    /// Create a new browser tool with custom config directory
    pub fn with_config_dir(config_dir: std::path::PathBuf) -> Result<Self, BrowserError> {
        let config_manager = Arc::new(ConfigManager::new(config_dir.clone())?);
        let default_profile = config_manager.get_default_profile().clone();
        let manager = Arc::new(BrowserManager::new(default_profile));
        let session = Arc::new(Mutex::new(BrowserSession::new(manager.clone())));
        
        // Create session manager
        let session_manager = SessionManager::new(
            config_dir.parent()
                .unwrap_or(&config_dir)
                .join("sessions")
        );
        
        Ok(Self {
            manager,
            config_manager,
            action_executor: ActionExecutor,
            session,
            session_manager,
        })
    }
    
    /// Parse browser action from JSON input
    fn parse_action(&self, input: &Value) -> Result<BrowserAction, String> {
        let action = input.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: action")?;
        
        match action {
            "navigate" => {
                let url = input.get("url")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: url for navigate action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Navigate {
                    url: url.to_string(),
                    wait_seconds,
                })
            }
            
            "click" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for click action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Click {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "type" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for type action")?;
                let text = input.get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: text for type action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Type {
                    selector: selector.to_string(),
                    text: text.to_string(),
                    wait_seconds,
                })
            }
            
            "screenshot" => {
                let output_path = input.get("output_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("screenshot.png");
                let format = match input.get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("png")
                {
                    "jpeg" | "jpg" => ScreenshotFormat::Jpeg,
                    "webp" => ScreenshotFormat::WebP,
                    _ => ScreenshotFormat::Png,
                };
                let quality = input.get("quality")
                    .and_then(|v| v.as_u64())
                    .map(|q| q as i64);
                
                Ok(BrowserAction::Screenshot {
                    output_path: output_path.to_string(),
                    format,
                    quality,
                })
            }
            
            "execute_script" => {
                let script = input.get("script")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: script for execute_script action")?;
                
                Ok(BrowserAction::ExecuteScript {
                    script: script.to_string(),
                })
            }
            
            "get_html" => {
                Ok(BrowserAction::GetHtml)
            }
            
            "get_text" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for get_text action")?;
                
                Ok(BrowserAction::GetText {
                    selector: selector.to_string(),
                })
            }
            
            "find_elements" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for find_elements action")?;
                
                Ok(BrowserAction::FindElements {
                    selector: selector.to_string(),
                })
            }
            
            "evaluate" => {
                let expression = input.get("script")
                    .or_else(|| input.get("expression"))
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: script/expression for evaluate action")?;
                
                Ok(BrowserAction::Evaluate {
                    expression: expression.to_string(),
                })
            }
            
            "wait_for_navigation" => {
                let timeout_seconds = input.get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30);
                
                Ok(BrowserAction::WaitForNavigation {
                    timeout_seconds,
                })
            }
            
            "wait_for_element" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for wait_for_element action")?;
                let timeout_seconds = input.get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10);
                
                Ok(BrowserAction::WaitForElement {
                    selector: selector.to_string(),
                    timeout_seconds,
                })
            }
            
            "hover" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for hover action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Hover {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "scroll_to" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for scroll_to action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::ScrollTo {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "get_attributes" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for get_attributes action")?;
                let attributes = input.get("attributes")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_else(|| vec!["id".to_string(), "class".to_string()]);
                
                Ok(BrowserAction::GetAttributes {
                    selector: selector.to_string(),
                    attributes,
                })
            }
            
            "set_attribute" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for set_attribute action")?;
                let attribute = input.get("attribute")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: attribute for set_attribute action")?;
                let value = input.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: value for set_attribute action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::SetAttribute {
                    selector: selector.to_string(),
                    attribute: attribute.to_string(),
                    value: value.to_string(),
                    wait_seconds,
                })
            }
            
            "upload_file" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for upload_file action")?;
                let file_path = input.get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: file_path for upload_file action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::UploadFile {
                    selector: selector.to_string(),
                    file_path: file_path.to_string(),
                    wait_seconds,
                })
            }
            
            "select_option" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for select_option action")?;
                let value = input.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: value for select_option action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::SelectOption {
                    selector: selector.to_string(),
                    value: value.to_string(),
                    wait_seconds,
                })
            }
            
            "check" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for check action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Check {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "uncheck" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for uncheck action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::Uncheck {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "press_key" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str());
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for press_key action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::PressKey {
                    selector: selector.map(|s| s.to_string()),
                    key: key.to_string(),
                    wait_seconds,
                })
            }
            
            "drag_drop" => {
                let source_selector = input.get("source_selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: source_selector for drag_drop action")?;
                let target_selector = input.get("target_selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: target_selector for drag_drop action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::DragDrop {
                    source_selector: source_selector.to_string(),
                    target_selector: target_selector.to_string(),
                    wait_seconds,
                })
            }
            
            "right_click" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: selector for right_click action")?;
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::RightClick {
                    selector: selector.to_string(),
                    wait_seconds,
                })
            }
            
            "mouse_wheel" => {
                let selector = input.get("selector")
                    .and_then(|v| v.as_str());
                let delta_x = input.get("delta_x")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                    .unwrap_or(0);
                let delta_y = input.get("delta_y")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                    .unwrap_or(0);
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::MouseWheel {
                    selector: selector.map(|s| s.to_string()),
                    delta_x,
                    delta_y,
                    wait_seconds,
                })
            }
            
            "get_cookies" => {
                Ok(BrowserAction::GetCookies)
            }
            
            "set_cookie" => {
                let name = input.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: name for set_cookie action")?;
                let value = input.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: value for set_cookie action")?;
                let domain = input.get("domain")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let path = input.get("path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let secure = input.get("secure")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let http_only = input.get("http_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let same_site = input.get("same_site")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let expires = input.get("expires")
                    .and_then(|v| v.as_i64());
                
                Ok(BrowserAction::SetCookie {
                    name: name.to_string(),
                    value: value.to_string(),
                    domain,
                    path,
                    secure,
                    http_only,
                    same_site,
                    expires,
                })
            }
            
            "delete_cookie" => {
                let name = input.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: name for delete_cookie action")?;
                let domain = input.get("domain")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let path = input.get("path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                Ok(BrowserAction::DeleteCookie {
                    name: name.to_string(),
                    domain,
                    path,
                })
            }
            
            "get_local_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for get_local_storage action")?;
                
                Ok(BrowserAction::GetLocalStorage {
                    key: key.to_string(),
                })
            }
            
            "set_local_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for set_local_storage action")?;
                let value = input.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: value for set_local_storage action")?;
                
                Ok(BrowserAction::SetLocalStorage {
                    key: key.to_string(),
                    value: value.to_string(),
                })
            }
            
            "delete_local_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for delete_local_storage action")?;
                
                Ok(BrowserAction::DeleteLocalStorage {
                    key: key.to_string(),
                })
            }
            
            "get_session_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for get_session_storage action")?;
                
                Ok(BrowserAction::GetSessionStorage {
                    key: key.to_string(),
                })
            }
            
            "set_session_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for set_session_storage action")?;
                let value = input.get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: value for set_session_storage action")?;
                
                Ok(BrowserAction::SetSessionStorage {
                    key: key.to_string(),
                    value: value.to_string(),
                })
            }
            
            "delete_session_storage" => {
                let key = input.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: key for delete_session_storage action")?;
                
                Ok(BrowserAction::DeleteSessionStorage {
                    key: key.to_string(),
                })
            }
            
            "go_back" => {
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::GoBack {
                    wait_seconds,
                })
            }
            
            "go_forward" => {
                let wait_seconds = input.get("wait_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                
                Ok(BrowserAction::GoForward {
                    wait_seconds,
                })
            }
            
            "create_snapshot" => {
                let snapshot_type = input.get("snapshot_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ai");
                let output_path = input.get("output_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                Ok(BrowserAction::CreateSnapshot {
                    snapshot_type: snapshot_type.to_string(),
                    output_path,
                })
            }
            
            "execute_batch" => {
                let operations = input.get("operations")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing required parameter: operations for execute_batch action")?;
                let parallel = input.get("parallel")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                
                Ok(BrowserAction::ExecuteBatch {
                    operations: operations.clone(),
                    parallel,
                })
            }
            
            "get_driver_info" => {
                Ok(BrowserAction::GetDriverInfo)
            }
            
            "send_protocol_message" => {
                let message_type = input.get("message_type")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: message_type for send_protocol_message action")?;
                let data = input.get("data")
                    .ok_or("Missing required parameter: data for send_protocol_message action")?;
                
                Ok(BrowserAction::SendProtocolMessage {
                    message_type: message_type.to_string(),
                    data: data.clone(),
                })
            }
            
            _ => Err(format!("Unknown action: {}", action)),
        }
    }
    
    /// Handle special actions (quit, create_instance, etc.)
    async fn handle_special_action(&self, action: &str, input: &Value) -> BrowserResult<String> {
        match action {
            "quit" => {
                let mut session = self.session.lock().await;
                if session.is_connected() {
                    session.close_current_instance().await?;
                    Ok("Browser closed successfully".to_string())
                } else {
                    Ok("No browser session to close".to_string())
                }
            }
            
            "create_instance" => {
                let instance_name = input.get("instance_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let profile_name = input.get("profile")
                    .and_then(|v| v.as_str());
                
                let profile = if let Some(profile_name) = profile_name {
                    self.config_manager.get_profile(profile_name)
                        .ok_or_else(|| BrowserError::NotFound(format!("Profile '{}' not found", profile_name)))?
                        .clone()
                } else {
                    self.config_manager.get_default_profile().clone()
                };
                
                self.manager.launch_browser(Some(&profile), Some(instance_name)).await?;
                
                let mut session = self.session.lock().await;
                session.connect(instance_name).await?;
                
                Ok(format!("Browser instance '{}' created with profile '{}'", instance_name, profile.name))
            }
            
            "connect" => {
                let instance_name = input.get("instance_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                
                let mut session = self.session.lock().await;
                session.connect(instance_name).await?;
                
                Ok(format!("Connected to browser instance '{}'", instance_name))
            }
            
            "create_page" => {
                let url = input.get("url")
                    .and_then(|v| v.as_str());
                
                let mut session = self.session.lock().await;
                let page_name = session.create_page(url).await?;
                
                Ok(format!("Created page '{}' {}", page_name, 
                    url.map(|u| format!("with URL {}", u)).unwrap_or_default()))
            }
            
            "switch_page" => {
                let page_name = input.get("page_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| BrowserError::Validation("Missing required parameter: page_name".to_string()))?;
                
                let mut session = self.session.lock().await;
                session.switch_page(page_name).await?;
                
                Ok(format!("Switched to page '{}'", page_name))
            }
            
            "close_page" => {
                let mut session = self.session.lock().await;
                session.close_current_page().await?;
                
                Ok("Current page closed".to_string())
            }
            
            "list_instances" => {
                let session = self.session.lock().await;
                let instances = session.list_instances().await?;
                
                if instances.is_empty() {
                    Ok("No browser instances available".to_string())
                } else {
                    Ok(format!("Available instances:\n{}", instances.join("\n")))
                }
            }
            
            "list_pages" => {
                let session = self.session.lock().await;
                let pages = session.list_pages().await?;
                
                if pages.is_empty() {
                    Ok("No pages available in current instance".to_string())
                } else {
                    Ok(format!("Available pages:\n{}", pages.join("\n")))
                }
            }
            
            "current_state" => {
                let session = self.session.lock().await;
                let instance = session.current_instance()
                    .unwrap_or("[none]");
                let page = session.current_page_name()
                    .unwrap_or("[none]");
                
                Ok(format!("Current instance: {}\nCurrent page: {}", instance, page))
            }
            
            _ => Err(BrowserError::Operation(format!("Unknown special action: {}", action))),
        }
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
                        "enum": [
                            "navigate", "click", "type", "screenshot", "execute_script", 
                            "get_html", "get_text", "find_elements", "evaluate", "quit",
                            "create_instance", "connect", "create_page", "switch_page",
                            "close_page", "list_instances", "list_pages", "current_state",
                            "wait_for_navigation", "wait_for_element", "hover", "scroll_to",
                            "get_attributes", "set_attribute", "upload_file", "select_option",
                            "check", "uncheck", "press_key", "drag_drop", "right_click", 
                            "mouse_wheel", "go_back", "go_forward", "get_cookies", "set_cookie",
                            "delete_cookie", "get_local_storage", "set_local_storage", 
                            "delete_local_storage", "get_session_storage", "set_session_storage",
                            "delete_session_storage", "create_snapshot", "execute_batch",
                            "get_driver_info", "send_protocol_message"
                        ],
                        "description": "The browser action to perform"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL to navigate to (required for navigate action)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for element (required for click, type, get_text, find_elements, etc.)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into element (required for type action)"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Path to save screenshot (optional for screenshot action)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["png", "jpeg", "jpg", "webp"],
                        "description": "Screenshot format (optional, default: png)"
                    },
                    "quality": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 100,
                        "description": "Screenshot quality for JPEG/WebP (optional, default: 90)"
                    },
                    "script": {
                        "type": "string",
                        "description": "JavaScript to execute (required for execute_script/evaluate actions)"
                    },
                    "expression": {
                        "type": "string",
                        "description": "CDP expression to evaluate (alternative to script)"
                    },
                    "headless": {
                        "type": "boolean",
                        "description": "Whether to run browser in headless mode (default: true)"
                    },
                    "wait_seconds": {
                        "type": "integer",
                        "description": "Seconds to wait after action (default: 2)"
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "description": "Seconds to wait for timeout (for wait actions)"
                    },
                    "instance_name": {
                        "type": "string",
                        "description": "Browser instance name (for create_instance/connect actions)"
                    },
                    "profile": {
                        "type": "string",
                        "description": "Browser profile name (for create_instance action)"
                    },
                    "page_name": {
                        "type": "string",
                        "description": "Page name (for switch_page action)"
                    },
                    "attributes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Attribute names to get (for get_attributes action)"
                    },
                    "attribute": {
                        "type": "string",
                        "description": "Attribute name to set (for set_attribute action)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to set (for set_attribute/select_option actions)"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "File path to upload (for upload_file action)"
                    },
                    "key": {
                        "type": "string",
                        "description": "Key to press (for press_key action)"
                    }
                },
                "required": ["action"]
            }),
        }
    }
    
    async fn execute(&self, input: &Value, _project_dir: &Path) -> ToolResult {
        match self.execute_internal(input).await {
            Ok(result) => ToolResult::success(result),
            Err(e) => ToolResult::error(format!("Browser error: {}", e)),
        }
    }
    
    async fn execute_with_path_manager(
        &self,
        input: &Value,
        path_manager: &crate::path_manager::PathManager,
    ) -> ToolResult {
        // Handle screenshot path resolution
        if let Some(action) = input.get("action").and_then(|v| v.as_str()) {
            if action == "screenshot" {
                if let Some(output_path) = input.get("output_path").and_then(|v| v.as_str()) {
                    // Check write permission for screenshot path
                    if let Err(e) = path_manager.check_write_permission(output_path) {
                        return ToolResult::error(format!("Permission denied for screenshot path: {}", e));
                    }
                    
                    // Create a new input with resolved path
                    let mut new_input = input.clone();
                    let resolved_path = path_manager.resolve(output_path);
                    if let Some(obj) = new_input.as_object_mut() {
                        obj.insert("output_path".to_string(), Value::String(resolved_path.display().to_string()));
                    }
                    
                    return self.execute(&new_input, path_manager.working_dir()).await;
                }
            }
        }
        
        self.execute(input, path_manager.working_dir()).await
    }
}

impl BrowserTool {
    /// Internal execution method that returns BrowserResult
    async fn execute_internal(&self, input: &Value) -> BrowserResult<String> {
        let action = input.get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrowserError::Validation("Missing required parameter: action".to_string()))?;
        
        // Handle special actions
        let special_actions = [
            "quit", "create_instance", "connect", "create_page", "switch_page",
            "close_page", "list_instances", "list_pages", "current_state"
        ];
        
        if special_actions.contains(&action) {
            return self.handle_special_action(action, input).await;
        }
        
        // Parse and execute regular action
        let browser_action = self.parse_action(input)
            .map_err(|e| BrowserError::Validation(e))?;
        
        // Get current session
        let session = self.session.lock().await;
        
        // Get current browser and page
        let (browser_state, page_name) = session.current_page().await?;
        
        // Execute action
        self.action_executor.execute(browser_state, &page_name, browser_action).await
    }
}