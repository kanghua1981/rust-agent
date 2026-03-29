use super::state::BrowserState;
use crate::tools::browser::config::BrowserProfile;
use crate::tools::browser::error::{BrowserError, connection_error};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::handler::viewport::Viewport;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Manages browser instances and their lifecycle
pub struct BrowserManager {
    /// Active browser instances
    browsers: Arc<RwLock<HashMap<String, Arc<Mutex<BrowserState>>>>>,
    /// Default profile
    default_profile: BrowserProfile,
}

impl BrowserManager {
    /// Create a new browser manager
    pub fn new(default_profile: BrowserProfile) -> Self {
        Self {
            browsers: Arc::new(RwLock::new(HashMap::new())),
            default_profile,
        }
    }
    
    /// Launch a new browser instance
    pub async fn launch_browser(
        &self,
        profile: Option<&BrowserProfile>,
        instance_name: Option<&str>,
    ) -> Result<String, BrowserError> {
        let profile = profile.unwrap_or(&self.default_profile);
        let instance_name = instance_name.unwrap_or("default").to_string();
        
        // Check if instance already exists
        {
            let browsers = self.browsers.read().await;
            if browsers.contains_key(&instance_name) {
                return Err(connection_error(format!("Browser instance '{}' already exists", instance_name)));
            }
        }
        
        // Build browser configuration
        let mut builder = BrowserConfig::builder()
            .viewport(Viewport {
                width: profile.viewport_width,
                height: profile.viewport_height,
                device_scale_factor: profile.device_scale_factor,
                emulating_mobile: profile.emulate_mobile,
                has_touch: profile.has_touch,
                is_landscape: profile.is_landscape,
            });
        
        if !profile.headless {
            builder = builder.with_head();
        }
        
        // Add browser arguments
        for arg in &profile.browser_args {
            builder = builder.arg(arg.as_str());
        }
        
        // Set browser executable if specified
        if let Some(executable) = &profile.browser_executable {
            builder = builder.chrome_executable(executable);
        }
        
        let config = builder
            .build()
            .map_err(|e| connection_error(format!("Failed to build browser config: {}", e)))?;
        
        // Launch browser
        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| connection_error(format!("Failed to launch browser: {}", e)))?;
        
        // Spawn browser handler task
        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                match event {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Browser handler error: {}", e);
                        break;
                    }
                }
            }
        });
        
        // Create browser state
        let browser_state = BrowserState::new(
            browser,
            handler_task,
            profile.clone(),
            instance_name.clone(),
        );
        
        let browser_state = Arc::new(Mutex::new(browser_state));
        
        // Store browser instance
        {
            let mut browsers = self.browsers.write().await;
            browsers.insert(instance_name.clone(), browser_state);
        }
        
        Ok(instance_name)
    }
    
    /// Get a browser instance by name
    pub async fn get_browser(&self, instance_name: &str) -> Result<Arc<Mutex<BrowserState>>, BrowserError> {
        let browsers = self.browsers.read().await;
        browsers.get(instance_name)
            .cloned()
            .ok_or_else(|| connection_error(format!("Browser instance '{}' not found", instance_name)))
    }
    
    /// Close a browser instance
    pub async fn close_browser(&self, instance_name: &str) -> Result<(), BrowserError> {
        let browser_state = {
            let mut browsers = self.browsers.write().await;
            browsers.remove(instance_name)
        };
        
        if let Some(browser_state) = browser_state {
            let mut state = browser_state.lock().await;
            state.close().await?;
        }
        
        Ok(())
    }
    
    /// Close all browser instances
    pub async fn close_all(&self) -> Result<(), BrowserError> {
        let instance_names: Vec<String> = {
            let browsers = self.browsers.read().await;
            browsers.keys().cloned().collect()
        };
        
        for instance_name in instance_names {
            self.close_browser(&instance_name).await?;
        }
        
        Ok(())
    }
    
    /// List all browser instances
    pub async fn list_browsers(&self) -> Vec<String> {
        let browsers = self.browsers.read().await;
        browsers.keys().cloned().collect()
    }
    
    /// Check if a browser instance exists
    pub async fn has_browser(&self, instance_name: &str) -> bool {
        let browsers = self.browsers.read().await;
        browsers.contains_key(instance_name)
    }
    
    /// Get the default profile
    pub fn default_profile(&self) -> &BrowserProfile {
        &self.default_profile
    }
    
    /// Set the default profile
    pub fn set_default_profile(&mut self, profile: BrowserProfile) {
        self.default_profile = profile;
    }
}