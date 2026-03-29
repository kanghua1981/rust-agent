use super::types::{BrowserError, ErrorSeverity};
use std::fmt;

/// Error handler for browser operations
pub struct ErrorHandler {
    /// Maximum number of retries for recoverable errors
    max_retries: u32,
    /// Current retry count
    current_retries: u32,
    /// Whether to log errors
    log_errors: bool,
}

impl ErrorHandler {
    /// Create a new error handler with default settings
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            current_retries: 0,
            log_errors: true,
        }
    }
    
    /// Create a new error handler with custom settings
    pub fn with_settings(max_retries: u32, log_errors: bool) -> Self {
        Self {
            max_retries,
            current_retries: 0,
            log_errors,
        }
    }
    
    /// Handle an error, potentially retrying the operation
    pub fn handle_error<T, F>(&mut self, error: BrowserError, operation: F) -> Result<T, BrowserError>
    where
        F: Fn() -> Result<T, BrowserError>,
    {
        if self.log_errors {
            self.log_error(&error);
        }
        
        if error.is_recoverable() && self.current_retries < self.max_retries {
            self.current_retries += 1;
            tracing::info!("Retrying operation (attempt {}/{})", self.current_retries, self.max_retries);
            operation()
        } else {
            Err(error)
        }
    }
    
    /// Log an error with appropriate severity
    fn log_error(&self, error: &BrowserError) {
        match error.severity() {
            ErrorSeverity::Warning => {
                tracing::warn!("Browser warning: {}", error);
            }
            ErrorSeverity::Error => {
                tracing::error!("Browser error: {}", error);
            }
            ErrorSeverity::Critical => {
                tracing::error!("Critical browser error: {}", error);
            }
        }
    }
    
    /// Reset the retry counter
    pub fn reset_retries(&mut self) {
        self.current_retries = 0;
    }
    
    /// Get the current retry count
    pub fn current_retries(&self) -> u32 {
        self.current_retries
    }
    
    /// Get the maximum retry count
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to convert string errors to BrowserError
pub fn to_browser_error<T: fmt::Display>(error: T) -> BrowserError {
    BrowserError::Operation(error.to_string())
}

/// Helper function to create validation errors
pub fn validation_error<T: fmt::Display>(message: T) -> BrowserError {
    BrowserError::Validation(message.to_string())
}

/// Helper function to create configuration errors
pub fn configuration_error<T: fmt::Display>(message: T) -> BrowserError {
    BrowserError::Configuration(message.to_string())
}

/// Helper function to create connection errors
pub fn connection_error<T: fmt::Display>(message: T) -> BrowserError {
    BrowserError::Connection(message.to_string())
}