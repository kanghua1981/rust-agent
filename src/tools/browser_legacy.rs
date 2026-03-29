use super::{Tool, ToolDefinition, ToolResult};
use serde_json::Value;
use std::path::Path;

/// Browser tool (backward compatibility wrapper for the new BrowserTool)
pub struct BrowserTool {
    /// New browser tool instance
    inner: crate::tools::browser::BrowserTool,
}

impl BrowserTool {
    pub fn new() -> Self {
        Self {
            inner: crate::tools::browser::BrowserTool::new(),
        }
    }
}

#[async_trait::async_trait]
impl Tool for BrowserTool {
    fn definition(&self) -> ToolDefinition {
        // Use the same definition as the new browser tool
        self.inner.definition()
    }

    async fn execute(&self, input: &Value, project_dir: &Path) -> ToolResult {
        // Delegate to the new browser tool
        self.inner.execute(input, project_dir).await
    }
    
    async fn execute_with_path_manager(
        &self,
        input: &Value,
        path_manager: &crate::path_manager::PathManager,
    ) -> ToolResult {
        // Delegate to the new browser tool
        self.inner.execute_with_path_manager(input, path_manager).await
    }
}