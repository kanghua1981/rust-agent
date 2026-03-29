use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Tool for uploading images to the conversation
pub struct UploadImageTool;

#[derive(Debug, Serialize, Deserialize)]
struct UploadImageInput {
    /// Path to the image file
    path: String,
    /// Optional description of the image
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl UploadImageTool {
    /// Detect MIME type from file extension
    fn detect_mime_type(path: &Path) -> Option<&'static str> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "bmp" => Some("image/bmp"),
            "tiff" | "tif" => Some("image/tiff"),
            _ => None,
        }
    }

    /// Read image file and convert to base64
    fn read_image_to_base64(path: &Path) -> Result<(String, String)> {
        // Read file
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read image file: {}", path.display()))?;

        // Detect MIME type
        let mime_type = Self::detect_mime_type(path)
            .with_context(|| format!("Unsupported image format: {}", path.display()))?;

        // Encode to base64
        let base64_data = STANDARD.encode(&data);

        Ok((mime_type.to_string(), base64_data))
    }
}

#[async_trait::async_trait]
impl Tool for UploadImageTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "upload_image".to_string(),
            description: "Upload an image file to the conversation. The image will be converted to base64 format and can be sent to vision-capable LLMs.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the image file (supports PNG, JPEG, GIF, WebP, BMP, TIFF)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description of the image"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let input: UploadImageInput = match serde_json::from_value(input.clone()) {
            Ok(input) => input,
            Err(e) => {
                return ToolResult::error(format!("Invalid input parameters: {}", e));
            }
        };

        // Resolve path relative to project directory (old method)
        let path = if input.path.starts_with('/') {
            Path::new(&input.path).to_path_buf()
        } else {
            project_dir.join(&input.path)
        };

        // Check if file exists
        if !path.exists() {
            return ToolResult::error(format!("Image file not found: {}", path.display()));
        }

        // Read and encode image
        let (mime_type, base64_data) = match Self::read_image_to_base64(&path) {
            Ok(result) => result,
            Err(e) => {
                return ToolResult::error(format!("Failed to process image: {}", e));
            }
        };

        // Get file size info
        let file_size = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        };

        // Create output message
        let mut output = format!(
            "Image uploaded successfully:\n\
             - Path: {}\n\
             - MIME type: {}\n\
             - File size: {} bytes\n\
             - Base64 size: {} characters",
            path.display(),
            mime_type,
            file_size,
            base64_data.len()
        );

        if let Some(description) = &input.description {
            output.push_str(&format!("\n- Description: {}", description));
        }

        // Note: The actual image data is not included in the tool result output
        // because it's too large. Instead, we'll create a ContentBlock::Image
        // in the agent layer when processing the tool result.
        output.push_str("\n\nNote: The image has been added to the conversation and will be sent to the LLM if it supports vision.");

        ToolResult::success(output)
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let input: UploadImageInput = match serde_json::from_value(input.clone()) {
            Ok(input) => input,
            Err(e) => {
                return ToolResult::error(format!("Invalid input parameters: {}", e));
            }
        };

        // Check if path is allowed (for sandbox mode)
        if !path_manager.is_path_allowed(&input.path) {
            return ToolResult::error(format!(
                "Access denied: '{}' is outside the allowed directory.",
                input.path
            ));
        }

        let resolved_path = path_manager.resolve(&input.path);
        
        // Check if file exists
        if !resolved_path.exists() {
            return ToolResult::error(format!("Image file not found: {}", resolved_path.display()));
        }

        // Read and encode image
        let (mime_type, base64_data) = match Self::read_image_to_base64(&resolved_path) {
            Ok(result) => result,
            Err(e) => {
                return ToolResult::error(format!("Failed to process image: {}", e));
            }
        };

        // Get file size info
        let file_size = match std::fs::metadata(&resolved_path) {
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        };

        // Create output message
        let mut output = format!(
            "Image uploaded successfully:\n\
             - Path: {}\n\
             - MIME type: {}\n\
             - File size: {} bytes\n\
             - Base64 size: {} characters",
            resolved_path.display(),
            mime_type,
            file_size,
            base64_data.len()
        );

        if let Some(description) = &input.description {
            output.push_str(&format!("\n- Description: {}", description));
        }

        // Note: The actual image data is not included in the tool result output
        // because it's too large. Instead, we'll create a ContentBlock::Image
        // in the agent layer when processing the tool result.
        output.push_str("\n\nNote: The image has been added to the conversation and will be sent to the LLM if it supports vision.");

        ToolResult::success(output)
    }
}