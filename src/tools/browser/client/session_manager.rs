use crate::tools::browser::error::{BrowserError, BrowserResult};
use crate::tools::browser::runtime::BrowserManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Session data for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionData {
    /// Instance name
    pub instance_name: String,
    /// Current page name
    pub current_page: Option<String>,
    /// Pages data
    pub pages: Vec<PageData>,
    /// Groups
    pub groups: HashMap<String, Vec<String>>,
    /// Timestamp
    pub timestamp: i64,
}

/// Page data for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageData {
    /// Page name
    pub name: String,
    /// Page ID
    pub id: u32,
    /// Page URL
    pub url: Option<String>,
    /// Page title
    pub title: Option<String>,
    /// Page group
    pub group: Option<String>,
    /// Navigation history
    pub history: Vec<String>,
    /// Current history position
    pub history_position: usize,
}

/// Session manager for saving and restoring browser sessions
pub struct SessionManager {
    /// Session directory
    session_dir: PathBuf,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(session_dir: PathBuf) -> Self {
        // Create directory if it doesn't exist
        if !session_dir.exists() {
            fs::create_dir_all(&session_dir).expect("Failed to create session directory");
        }
        
        Self { session_dir }
    }
    
    /// Save current session
    pub async fn save_session(
        &self,
        session_name: &str,
        manager: &Arc<BrowserManager>,
        current_instance: Option<&str>,
        current_page: Option<&str>,
    ) -> BrowserResult<()> {
        let session_data = self.collect_session_data(manager, current_instance, current_page).await?;
        
        let session_path = self.session_dir.join(format!("{}.json", session_name));
        let json_data = serde_json::to_string_pretty(&session_data)
            .map_err(|e| BrowserError::Operation(format!("Failed to serialize session: {}", e)))?;
        
        fs::write(&session_path, json_data)
            .map_err(|e| BrowserError::Operation(format!("Failed to save session: {}", e)))?;
        
        Ok(())
    }
    
    /// Load session
    pub async fn load_session(
        &self,
        session_name: &str,
        manager: &Arc<BrowserManager>,
    ) -> BrowserResult<(String, Option<String>)> {
        let session_path = self.session_dir.join(format!("{}.json", session_name));
        
        if !session_path.exists() {
            return Err(BrowserError::NotFound(format!("Session '{}' not found", session_name)));
        }
        
        let json_data = fs::read_to_string(&session_path)
            .map_err(|e| BrowserError::Operation(format!("Failed to read session: {}", e)))?;
        
        let session_data: BrowserSessionData = serde_json::from_str(&json_data)
            .map_err(|e| BrowserError::Operation(format!("Failed to parse session: {}", e)))?;
        
        // Restore browser instance
        if !manager.has_browser(&session_data.instance_name).await {
            // Create new instance with default profile
            manager.launch_browser(None, Some(&session_data.instance_name)).await?;
        }
        
        // Restore pages
        let browser_state = manager.get_browser(&session_data.instance_name).await?;
        let mut state = browser_state.lock().await;
        
        // Clear existing pages
        state.close_all_pages().await?;
        
        // Create pages from session data
        for page_data in &session_data.pages {
            let page_name = page_data.name.clone();
            let url = page_data.url.clone();
            
            // Create page
            state.create_page(url.as_deref()).await?;
            
            // Set page properties
            if let Some(page_state) = state.get_page(&page_name) {
                let tab_state = page_state.tab_state_mut();
                tab_state.title = page_data.title.clone();
                tab_state.url = page_data.url.clone();
                tab_state.group = page_data.group.clone();
                tab_state.history = page_data.history.clone();
                tab_state.history_position = page_data.history_position;
            }
        }
        
        Ok((session_data.instance_name, session_data.current_page))
    }
    
    /// List saved sessions
    pub fn list_sessions(&self) -> BrowserResult<Vec<String>> {
        let mut sessions = Vec::new();
        
        if let Ok(entries) = fs::read_dir(&self.session_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Some(stem) = entry.path().file_stem() {
                            sessions.push(stem.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        
        sessions.sort();
        Ok(sessions)
    }
    
    /// Delete session
    pub fn delete_session(&self, session_name: &str) -> BrowserResult<()> {
        let session_path = self.session_dir.join(format!("{}.json", session_name));
        
        if !session_path.exists() {
            return Err(BrowserError::NotFound(format!("Session '{}' not found", session_name)));
        }
        
        fs::remove_file(&session_path)
            .map_err(|e| BrowserError::Operation(format!("Failed to delete session: {}", e)))?;
        
        Ok(())
    }
    
    /// Collect session data from current state
    async fn collect_session_data(
        &self,
        manager: &Arc<BrowserManager>,
        current_instance: Option<&str>,
        current_page: Option<&str>,
    ) -> BrowserResult<BrowserSessionData> {
        let instance_name = current_instance
            .ok_or_else(|| BrowserError::Operation("No browser instance selected".to_string()))?;
        
        let browser_state = manager.get_browser(instance_name).await?;
        let state = browser_state.lock().await;
        
        let mut pages = Vec::new();
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        
        for page in state.list_pages() {
            let tab_state = page.tab_state();
            
            let page_data = PageData {
                name: page.name().to_string(),
                id: page.id(),
                url: tab_state.url.clone(),
                title: tab_state.title.clone(),
                group: tab_state.group.clone(),
                history: tab_state.history.clone(),
                history_position: tab_state.history_position,
            };
            
            pages.push(page_data);
            
            // Build groups map
            if let Some(group) = &tab_state.group {
                groups.entry(group.clone())
                    .or_insert_with(Vec::new)
                    .push(page.name().to_string());
            }
        }
        
        Ok(BrowserSessionData {
            instance_name: instance_name.to_string(),
            current_page: current_page.map(|s| s.to_string()),
            pages,
            groups,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}