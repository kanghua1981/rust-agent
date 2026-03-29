use thiserror::Error;

/// Main error type for browser automation operations
#[derive(Debug, Error)]
pub enum BrowserError {
    /// Validation errors (invalid parameters, etc.)
    #[error("Validation error: {0}")]
    Validation(String),
    
    /// Configuration errors (invalid config, missing settings, etc.)
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// Connection errors (failed to connect to browser, CDP issues, etc.)
    #[error("Connection error: {0}")]
    Connection(String),
    
    /// Timeout errors (operations taking too long)
    #[error("Timeout error: {0}")]
    Timeout(String),
    
    /// Resource not found errors (elements, pages, etc.)
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    /// Operation errors (general operation failures)
    #[error("Operation failed: {0}")]
    Operation(String),
    
    /// Browser-specific errors (browser crashes, etc.)
    #[error("Browser error: {0}")]
    Browser(String),
    
    /// Protocol errors (CDP protocol violations, etc.)
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    /// IO errors (file operations, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    /// URL parsing errors
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
}

/// Result type for browser operations
pub type BrowserResult<T> = Result<T, BrowserError>;

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Warning - operation can continue
    Warning,
    /// Error - operation failed but can be retried
    Error,
    /// Critical - unrecoverable error
    Critical,
}

impl BrowserError {
    /// Get the severity level of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            BrowserError::Validation(_) => ErrorSeverity::Error,
            BrowserError::Configuration(_) => ErrorSeverity::Error,
            BrowserError::Connection(_) => ErrorSeverity::Critical,
            BrowserError::Timeout(_) => ErrorSeverity::Error,
            BrowserError::NotFound(_) => ErrorSeverity::Error,
            BrowserError::Operation(_) => ErrorSeverity::Error,
            BrowserError::Browser(_) => ErrorSeverity::Critical,
            BrowserError::Protocol(_) => ErrorSeverity::Critical,
            BrowserError::Io(_) => ErrorSeverity::Error,
            BrowserError::Json(_) => ErrorSeverity::Error,
            BrowserError::Url(_) => ErrorSeverity::Error,
        }
    }
    
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Warning | ErrorSeverity::Error => true,
            ErrorSeverity::Critical => false,
        }
    }
}