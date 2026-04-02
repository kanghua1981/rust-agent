#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Browser configuration profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProfile {
    /// Profile name
    pub name: String,
    
    /// Whether to run browser in headless mode
    pub headless: bool,
    
    /// Viewport width
    pub viewport_width: u32,
    
    /// Viewport height
    pub viewport_height: u32,
    
    /// Device scale factor
    pub device_scale_factor: Option<f64>,
    
    /// Whether to emulate mobile device
    pub emulate_mobile: bool,
    
    /// Whether device has touch support
    pub has_touch: bool,
    
    /// Whether viewport is in landscape orientation
    pub is_landscape: bool,
    
    /// User agent string
    pub user_agent: Option<String>,
    
    /// Default navigation timeout in seconds
    pub navigation_timeout: u64,
    
    /// Default element wait timeout in seconds
    pub element_timeout: u64,
    
    /// Default screenshot quality (0-100)
    pub screenshot_quality: u8,
    
    /// Default screenshot format
    pub screenshot_format: ScreenshotFormat,
    
    /// Browser executable path (if not using default)
    pub browser_executable: Option<PathBuf>,
    
    /// Additional browser arguments
    pub browser_args: Vec<String>,
    
    /// Proxy settings
    pub proxy: Option<ProxyConfig>,
    
    /// Whether to ignore certificate errors
    pub ignore_certificate_errors: bool,
    
    /// Whether to enable JavaScript
    pub enable_javascript: bool,
    
    /// Whether to block images
    pub block_images: bool,
    
    /// Whether to block popups
    pub block_popups: bool,
    
    /// Default download directory
    pub download_directory: Option<PathBuf>,
    
    /// Cookies to set initially
    pub initial_cookies: Vec<Cookie>,
    
    /// Local storage data to set initially
    pub initial_local_storage: Vec<LocalStorageItem>,
    
    /// Session storage data to set initially
    pub initial_session_storage: Vec<SessionStorageItem>,
}

/// Screenshot format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScreenshotFormat {
    /// PNG format (lossless)
    Png,
    /// JPEG format (lossy)
    Jpeg,
    /// WebP format
    WebP,
}

/// Proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy server URL as string
    pub server: String,
    /// Proxy username (optional)
    pub username: Option<String>,
    /// Proxy password (optional)
    pub password: Option<String>,
    /// Bypass list (optional)
    pub bypass_list: Vec<String>,
}

/// Cookie definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name
    pub name: String,
    /// Cookie value
    pub value: String,
    /// Domain
    pub domain: String,
    /// Path
    pub path: String,
    /// Secure flag
    pub secure: bool,
    /// HTTP only flag
    pub http_only: bool,
    /// Same site setting
    pub same_site: SameSite,
    /// Expiration timestamp (optional)
    pub expires: Option<i64>,
}

/// Same site cookie setting
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SameSite {
    /// Strict same-site policy
    Strict,
    /// Lax same-site policy
    Lax,
    /// None (cross-site allowed)
    None,
}

/// Local storage item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageItem {
    /// Key
    pub key: String,
    /// Value
    pub value: String,
}

/// Session storage item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStorageItem {
    /// Key
    pub key: String,
    /// Value
    pub value: String,
}

impl Default for BrowserProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            headless: true,
            viewport_width: 1920,
            viewport_height: 1080,
            device_scale_factor: None,
            emulate_mobile: false,
            has_touch: false,
            is_landscape: false,
            user_agent: None,
            navigation_timeout: 30,
            element_timeout: 10,
            screenshot_quality: 90,
            screenshot_format: ScreenshotFormat::Png,
            browser_executable: None,
            browser_args: Vec::new(),
            proxy: None,
            ignore_certificate_errors: false,
            enable_javascript: true,
            block_images: false,
            block_popups: true,
            download_directory: None,
            initial_cookies: Vec::new(),
            initial_local_storage: Vec::new(),
            initial_session_storage: Vec::new(),
        }
    }
}

impl BrowserProfile {
    /// Create a new profile with the given name
    pub fn new(name: &str) -> Self {
        let mut profile = Self::default();
        profile.name = name.to_string();
        profile
    }
    
    /// Create a mobile profile
    pub fn mobile(name: &str) -> Self {
        let mut profile = Self::new(name);
        profile.viewport_width = 375;
        profile.viewport_height = 667;
        profile.emulate_mobile = true;
        profile.has_touch = true;
        profile.user_agent = Some("Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1".to_string());
        profile
    }
    
    /// Create a tablet profile
    pub fn tablet(name: &str) -> Self {
        let mut profile = Self::new(name);
        profile.viewport_width = 768;
        profile.viewport_height = 1024;
        profile.emulate_mobile = true;
        profile.has_touch = true;
        profile.user_agent = Some("Mozilla/5.0 (iPad; CPU OS 14_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1".to_string());
        profile
    }
    
    /// Create a desktop profile
    pub fn desktop(name: &str) -> Self {
        Self::new(name)
    }
    
    /// Create a headful (non-headless) profile
    pub fn headful(name: &str) -> Self {
        let mut profile = Self::new(name);
        profile.headless = false;
        profile
    }
    
    /// Validate the profile configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Profile name cannot be empty".to_string());
        }
        
        if self.viewport_width == 0 || self.viewport_height == 0 {
            return Err("Viewport dimensions must be positive".to_string());
        }
        
        if self.navigation_timeout == 0 {
            return Err("Navigation timeout must be positive".to_string());
        }
        
        if self.element_timeout == 0 {
            return Err("Element timeout must be positive".to_string());
        }
        
        if self.screenshot_quality > 100 {
            return Err("Screenshot quality must be between 0 and 100".to_string());
        }
        
        Ok(())
    }
}