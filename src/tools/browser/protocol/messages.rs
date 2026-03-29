use serde::{Deserialize, Serialize};

/// Protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    /// Message ID (for requests and responses)
    pub id: Option<i64>,
    
    /// Message type (command, event, response)
    pub message_type: String,
    
    /// Method name (for commands and events)
    pub method: Option<String>,
    
    /// Parameters (for commands and events)
    pub params: serde_json::Value,
    
    /// Result (for responses)
    pub result: Option<serde_json::Value>,
    
    /// Error (for responses)
    pub error: Option<MessageError>,
    
    /// Session ID
    pub session_id: Option<String>,
    
    /// Timestamp
    pub timestamp: i64,
    
    /// Message headers
    pub headers: MessageHeader,
}

/// Message error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageError {
    /// Error code
    pub code: i32,
    
    /// Error message
    pub message: String,
    
    /// Error data
    pub data: Option<serde_json::Value>,
}

/// Message header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Protocol version
    pub protocol_version: String,
    
    /// Message format version
    pub format_version: String,
    
    /// Compression type
    pub compression: Option<String>,
    
    /// Content type
    pub content_type: String,
    
    /// Content encoding
    pub content_encoding: Option<String>,
    
    /// Message size in bytes
    pub content_length: usize,
    
    /// Message priority
    pub priority: Option<u8>,
    
    /// Message TTL (time to live) in seconds
    pub ttl: Option<u64>,
    
    /// Custom headers
    pub custom: std::collections::HashMap<String, String>,
}

/// Message type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Command message (request)
    Command,
    
    /// Event message (notification)
    Event,
    
    /// Response message
    Response,
    
    /// Error message
    Error,
    
    /// Heartbeat message
    Heartbeat,
}

impl ProtocolMessage {
    /// Create a new command message
    pub fn new_command(id: i64, method: &str, params: serde_json::Value) -> Self {
        Self {
            id: Some(id),
            message_type: "command".to_string(),
            method: Some(method.to_string()),
            params,
            result: None,
            error: None,
            session_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            headers: MessageHeader::default(),
        }
    }
    
    /// Create a new event message
    pub fn new_event(method: &str, params: serde_json::Value) -> Self {
        Self {
            id: None,
            message_type: "event".to_string(),
            method: Some(method.to_string()),
            params,
            result: None,
            error: None,
            session_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            headers: MessageHeader::default(),
        }
    }
    
    /// Create a new response message
    pub fn new_response(id: i64, result: serde_json::Value) -> Self {
        Self {
            id: Some(id),
            message_type: "response".to_string(),
            method: None,
            params: serde_json::Value::Null,
            result: Some(result),
            error: None,
            session_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            headers: MessageHeader::default(),
        }
    }
    
    /// Create a new error message
    pub fn new_error(id: Option<i64>, code: i32, message: &str, data: Option<serde_json::Value>) -> Self {
        Self {
            id,
            message_type: "error".to_string(),
            method: None,
            params: serde_json::Value::Null,
            result: None,
            error: Some(MessageError {
                code,
                message: message.to_string(),
                data,
            }),
            session_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            headers: MessageHeader::default(),
        }
    }
    
    /// Create a new heartbeat message
    pub fn new_heartbeat() -> Self {
        Self {
            id: None,
            message_type: "heartbeat".to_string(),
            method: None,
            params: serde_json::Value::Null,
            result: None,
            error: None,
            session_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            headers: MessageHeader::default(),
        }
    }
    
    /// Get message type enum
    pub fn get_message_type(&self) -> MessageType {
        match self.message_type.as_str() {
            "command" => MessageType::Command,
            "event" => MessageType::Event,
            "response" => MessageType::Response,
            "error" => MessageType::Error,
            "heartbeat" => MessageType::Heartbeat,
            _ => MessageType::Error,
        }
    }
    
    /// Check if message is a command
    pub fn is_command(&self) -> bool {
        self.get_message_type() == MessageType::Command
    }
    
    /// Check if message is an event
    pub fn is_event(&self) -> bool {
        self.get_message_type() == MessageType::Event
    }
    
    /// Check if message is a response
    pub fn is_response(&self) -> bool {
        self.get_message_type() == MessageType::Response
    }
    
    /// Check if message is an error
    pub fn is_error(&self) -> bool {
        self.get_message_type() == MessageType::Error
    }
    
    /// Check if message is a heartbeat
    pub fn is_heartbeat(&self) -> bool {
        self.get_message_type() == MessageType::Heartbeat
    }
    
    /// Set session ID
    pub fn set_session_id(&mut self, session_id: &str) {
        self.session_id = Some(session_id.to_string());
    }
    
    /// Get session ID
    pub fn get_session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    
    /// Set header value
    pub fn set_header(&mut self, key: &str, value: &str) {
        self.headers.custom.insert(key.to_string(), value.to_string());
    }
    
    /// Get header value
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.custom.get(key)
    }
    
    /// Validate message
    pub fn validate(&self) -> Result<(), String> {
        match self.get_message_type() {
            MessageType::Command => {
                if self.id.is_none() {
                    return Err("Command message must have ID".to_string());
                }
                if self.method.is_none() {
                    return Err("Command message must have method".to_string());
                }
            }
            MessageType::Event => {
                if self.method.is_none() {
                    return Err("Event message must have method".to_string());
                }
            }
            MessageType::Response => {
                if self.id.is_none() {
                    return Err("Response message must have ID".to_string());
                }
                if self.result.is_none() && self.error.is_none() {
                    return Err("Response message must have result or error".to_string());
                }
            }
            MessageType::Error => {
                if self.error.is_none() {
                    return Err("Error message must have error".to_string());
                }
            }
            MessageType::Heartbeat => {
                // Heartbeat messages have no specific requirements
            }
        }
        
        Ok(())
    }
    
    /// Get message size in bytes
    pub fn size(&self) -> usize {
        // Estimate size by serializing to JSON
        serde_json::to_string(self)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

impl Default for MessageHeader {
    fn default() -> Self {
        Self {
            protocol_version: "1.0".to_string(),
            format_version: "1.0".to_string(),
            compression: None,
            content_type: "application/json".to_string(),
            content_encoding: None,
            content_length: 0,
            priority: None,
            ttl: None,
            custom: std::collections::HashMap::new(),
        }
    }
}

impl MessageError {
    /// Create a new message error
    pub fn new(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            data: None,
        }
    }
    
    /// Create a new message error with data
    pub fn new_with_data(code: i32, message: &str, data: serde_json::Value) -> Self {
        Self {
            code,
            message: message.to_string(),
            data: Some(data),
        }
    }
    
    /// Check if error is fatal
    pub fn is_fatal(&self) -> bool {
        self.code >= 500
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        self.code < 500
    }
    
    /// Get error category
    pub fn category(&self) -> ErrorCategory {
        match self.code {
            0..=99 => ErrorCategory::Protocol,
            100..=199 => ErrorCategory::Validation,
            200..=299 => ErrorCategory::Execution,
            300..=399 => ErrorCategory::Resource,
            400..=499 => ErrorCategory::Client,
            500..=599 => ErrorCategory::Server,
            _ => ErrorCategory::Unknown,
        }
    }
}

/// Error category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Protocol errors
    Protocol,
    
    /// Validation errors
    Validation,
    
    /// Execution errors
    Execution,
    
    /// Resource errors
    Resource,
    
    /// Client errors
    Client,
    
    /// Server errors
    Server,
    
    /// Unknown errors
    Unknown,
}