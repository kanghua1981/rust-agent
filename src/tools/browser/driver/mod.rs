#![allow(unused_imports, dead_code)]

//! Browser driver module for low-level browser control
//! 
//! This module provides abstractions for different browser drivers
//! and connection management.

mod chromium;
mod cdp_client;
mod connection_pool;

pub use chromium::ChromiumDriver;
pub use cdp_client::CdpClient;
pub use connection_pool::{ConnectionPool, PoolStatistics, PooledConnectionHandle};

/// Connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Maximum connection age in seconds
    pub max_connection_age_seconds: u64,
    /// Maximum idle time in seconds
    pub max_idle_time_seconds: u64,
    /// Whether to validate connection on checkout
    pub validate_on_checkout: bool,
    /// Connection timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connection_age_seconds: 3600,
            max_idle_time_seconds: 300,
            validate_on_checkout: true,
            timeout_seconds: 30,
        }
    }
}

/// Connection trait for browser connections
pub trait Connection: Send {
    /// Get connection ID
    fn id(&self) -> &str;
    
    /// Check if connection is valid
    fn is_valid(&self) -> bool;
    
    /// Close the connection
    fn close(&mut self) -> Result<(), DriverError>;
}

/// Connection factory trait for creating connections
pub trait ConnectionFactory: Send + Sync {
    /// Create a new connection
    fn create_connection(&self, config: &ConnectionConfig) -> Result<Box<dyn Connection>, DriverError>;
}

/// Browser driver trait
pub trait BrowserDriver {
    /// Connect to browser
    fn connect(&mut self) -> Result<(), DriverError>;
    
    /// Disconnect from browser
    fn disconnect(&mut self) -> Result<(), DriverError>;
    
    /// Check if connected
    fn is_connected(&self) -> bool;
    
    /// Get browser version
    fn get_version(&self) -> Result<String, DriverError>;
    
    /// Create new page/tab
    fn create_page(&mut self) -> Result<String, DriverError>;
    
    /// Close page/tab
    fn close_page(&mut self, page_id: &str) -> Result<(), DriverError>;
    
    /// Navigate to URL
    fn navigate(&mut self, page_id: &str, url: &str) -> Result<(), DriverError>;
    
    /// Execute JavaScript
    fn execute_script(&mut self, page_id: &str, script: &str) -> Result<String, DriverError>;
    
    /// Take screenshot
    fn take_screenshot(&mut self, page_id: &str, format: &str) -> Result<Vec<u8>, DriverError>;
}

/// Driver error
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("Timeout error: {0}")]
    Timeout(String),
    
    #[error("Browser error: {0}")]
    Browser(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}
