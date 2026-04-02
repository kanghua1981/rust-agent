#![allow(unused_imports, dead_code)]

//! Protocol module for handling browser communication protocols
//! 
//! This module provides implementations for various browser protocols
//! including CDP (Chrome DevTools Protocol).

mod cdp;
mod messages;
mod serializer;

pub use cdp::{CdpProtocol, CdpSession, CdpEvent};
pub use messages::{ProtocolMessage, MessageType, MessageHeader};
pub use serializer::{ProtocolSerializer, ProtocolDeserializer};

/// Protocol error
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    
    #[error("Protocol violation: {0}")]
    Violation(String),
    
    #[error("Version mismatch: {0}")]
    Version(String),
    
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Protocol trait
pub trait Protocol {
    /// Get protocol name
    fn name(&self) -> &str;
    
    /// Get protocol version
    fn version(&self) -> &str;
    
    /// Check if protocol supports feature
    fn supports_feature(&self, feature: &str) -> bool;
    
    /// Encode message
    fn encode(&self, message: &ProtocolMessage) -> Result<Vec<u8>, ProtocolError>;
    
    /// Decode message
    fn decode(&self, data: &[u8]) -> Result<ProtocolMessage, ProtocolError>;
    
    /// Validate message
    fn validate(&self, message: &ProtocolMessage) -> Result<(), ProtocolError>;
}

/// Protocol factory
pub trait ProtocolFactory {
    /// Create protocol instance
    fn create_protocol(&self, version: &str) -> Result<Box<dyn Protocol>, ProtocolError>;
    
    /// Get supported versions
    fn supported_versions(&self) -> Vec<String>;
    
    /// Get default version
    fn default_version(&self) -> String;
}