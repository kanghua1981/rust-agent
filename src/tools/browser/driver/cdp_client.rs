use super::DriverError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};

/// CDP (Chrome DevTools Protocol) client
pub struct CdpClient {
    /// WebSocket URL
    ws_url: String,
    
    /// WebSocket connection
    connection: Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    
    /// Next message ID
    next_id: i32,
    
    /// Pending responses
    pending_responses: HashMap<i32, tokio::sync::oneshot::Sender<CdpResponse>>,
    
    /// Event handlers
    event_handlers: HashMap<String, Vec<Box<dyn Fn(serde_json::Value) + Send + Sync>>>,
}

/// CDP message
#[derive(Debug, Serialize, Deserialize)]
struct CdpMessage {
    /// Message ID
    id: i32,
    
    /// Method name
    method: String,
    
    /// Parameters
    params: serde_json::Value,
}

/// CDP response
#[derive(Debug, Serialize, Deserialize)]
pub struct CdpResponse {
    /// Response ID
    id: i32,
    
    /// Result
    result: Option<serde_json::Value>,
    
    /// Error
    error: Option<CdpError>,
}

/// CDP error
#[derive(Debug, Serialize, Deserialize)]
pub struct CdpError {
    /// Error code
    code: i32,
    
    /// Error message
    message: String,
    
    /// Error data
    data: Option<serde_json::Value>,
}

/// CDP event
#[derive(Debug, Serialize, Deserialize)]
struct CdpEvent {
    /// Method name
    method: String,
    
    /// Parameters
    params: serde_json::Value,
}

impl CdpClient {
    /// Create a new CDP client
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            connection: None,
            next_id: 1,
            pending_responses: HashMap::new(),
            event_handlers: HashMap::new(),
        }
    }
    
    /// Connect to WebSocket
    pub async fn connect(&mut self) -> Result<(), DriverError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.ws_url)
            .await
            .map_err(|e| DriverError::Connection(format!("Failed to connect to WebSocket: {}", e)))?;
        
        self.connection = Some(ws_stream);
        
        // Start message handler
        self.start_message_handler();
        
        Ok(())
    }
    
    /// Disconnect from WebSocket
    pub async fn disconnect(&mut self) -> Result<(), DriverError> {
        if let Some(mut connection) = self.connection.take() {
            connection.close(None).await
                .map_err(|e| DriverError::Connection(format!("Failed to close connection: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// Send a CDP command
    pub async fn send_command(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, DriverError> {
        let id = self.next_id;
        self.next_id += 1;
        
        let message = CdpMessage {
            id,
            method: method.to_string(),
            params,
        };
        
        let message_json = serde_json::to_string(&message)
            .map_err(|e| DriverError::Json(e))?;
        
        // Create channel for response
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_responses.insert(id, tx);
        
        // Send message
        if let Some(connection) = &mut self.connection {
            connection.send(Message::Text(message_json)).await
                .map_err(|e| DriverError::Protocol(format!("Failed to send message: {}", e)))?;
        } else {
            return Err(DriverError::Connection("Not connected".to_string()));
        }
        
        // Wait for response with timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| DriverError::Timeout("Response timeout".to_string()))?
            .map_err(|_| DriverError::Protocol("Response channel closed".to_string()))?;
        
        if let Some(error) = response.error {
            return Err(DriverError::Protocol(format!("CDP error: {} (code: {})", error.message, error.code)));
        }
        
        response.result.ok_or_else(|| DriverError::Protocol("No result in response".to_string()))
    }
    
    /// Start message handler
    fn start_message_handler(&mut self) {
        if let Some(connection) = &mut self.connection {
            // TODO: Implement message handler
            // For now, just log that handler would start
            println!("Message handler would start for connection");
        }
    }
    
    /// Add event handler
    pub fn add_event_handler<F>(&mut self, event: &str, handler: F)
    where
        F: Fn(serde_json::Value) + Send + Sync + 'static,
    {
        self.event_handlers
            .entry(event.to_string())
            .or_insert_with(Vec::new)
            .push(Box::new(handler));
    }
    
    /// Enable domain
    pub async fn enable_domain(&mut self, domain: &str) -> Result<(), DriverError> {
        self.send_command(&format!("{}.enable", domain), serde_json::json!({})).await?;
        Ok(())
    }
    
    /// Disable domain
    pub async fn disable_domain(&mut self, domain: &str) -> Result<(), DriverError> {
        self.send_command(&format!("{}.disable", domain), serde_json::json!({})).await?;
        Ok(())
    }
    
    /// Navigate to URL
    pub async fn navigate(&mut self, url: &str) -> Result<String, DriverError> {
        let result = self.send_command("Page.navigate", serde_json::json!({
            "url": url
        })).await?;
        
        let frame_id = result["frameId"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| DriverError::Protocol("No frameId in navigation response".to_string()))?;
        
        Ok(frame_id)
    }
    
    /// Evaluate JavaScript
    pub async fn evaluate(&mut self, expression: &str) -> Result<serde_json::Value, DriverError> {
        let result = self.send_command("Runtime.evaluate", serde_json::json!({
            "expression": expression,
            "returnByValue": true
        })).await?;
        
        Ok(result)
    }
    
    /// Take screenshot
    pub async fn take_screenshot(&mut self, format: &str, quality: Option<i32>) -> Result<String, DriverError> {
        let params = match format {
            "png" => serde_json::json!({
                "format": "png"
            }),
            "jpeg" => serde_json::json!({
                "format": "jpeg",
                "quality": quality.unwrap_or(80)
            }),
            _ => return Err(DriverError::Protocol(format!("Unsupported screenshot format: {}", format))),
        };
        
        let result = self.send_command("Page.captureScreenshot", params).await?;
        
        let data = result["data"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| DriverError::Protocol("No data in screenshot response".to_string()))?;
        
        Ok(data)
    }
    
    /// Get DOM tree
    pub async fn get_dom_tree(&mut self) -> Result<serde_json::Value, DriverError> {
        let result = self.send_command("DOM.getDocument", serde_json::json!({})).await?;
        Ok(result)
    }
    
    /// Get element by selector
    pub async fn get_element(&mut self, selector: &str) -> Result<i32, DriverError> {
        let document = self.get_dom_tree().await?;
        let root_node_id = document["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| DriverError::Protocol("No root nodeId in document".to_string()))? as i32;
        
        let result = self.send_command("DOM.querySelector", serde_json::json!({
            "nodeId": root_node_id,
            "selector": selector
        })).await?;
        
        let node_id = result["nodeId"]
            .as_i64()
            .ok_or_else(|| DriverError::Protocol("No nodeId in querySelector response".to_string()))? as i32;
        
        Ok(node_id)
    }
    
    /// Click element
    pub async fn click_element(&mut self, node_id: i32) -> Result<(), DriverError> {
        let box_model = self.send_command("DOM.getBoxModel", serde_json::json!({
            "nodeId": node_id
        })).await?;
        
        let content = box_model["model"]["content"]
            .as_array()
            .ok_or_else(|| DriverError::Protocol("No content in box model".to_string()))?;
        
        if content.len() < 6 {
            return Err(DriverError::Protocol("Invalid box model content".to_string()));
        }
        
        let x = (content[0].as_f64().unwrap_or(0.0) + content[2].as_f64().unwrap_or(0.0)) / 2.0;
        let y = (content[1].as_f64().unwrap_or(0.0) + content[5].as_f64().unwrap_or(0.0)) / 2.0;
        
        self.send_command("Input.dispatchMouseEvent", serde_json::json!({
            "type": "mousePressed",
            "x": x,
            "y": y,
            "button": "left",
            "clickCount": 1
        })).await?;
        
        self.send_command("Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseReleased",
            "x": x,
            "y": y,
            "button": "left",
            "clickCount": 1
        })).await?;
        
        Ok(())
    }
}