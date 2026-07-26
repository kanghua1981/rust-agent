use super::{Tool, ToolDefinition, ToolResult};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Structured directory entry for JSON serialization (Web UI directory tree).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DirEntry>,
}

pub struct ListDirTool;

#[async_trait::async_trait]
impl Tool for ListDirTool {
    fn toolset(&self) -> Option<super::Toolset> {
        Some(super::Toolset::Search)
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_directory".to_string(),
            description: "List the contents of a directory, showing files and subdirectories with their types and sizes.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the directory to list (defaults to current directory)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Optional: whether to list recursively (default: false, max depth: 3)"
                    }
                },
                "required": []
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = resolve_path_old(path, project_dir);

        self.list_dir_internal(&path, recursive).await
    }
    
    async fn execute_with_path_manager(
        &self, 
        input: &serde_json::Value, 
        path_manager: &crate::path_manager::PathManager
    ) -> ToolResult {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Check if path is allowed (for sandbox mode)
        if !path_manager.is_path_allowed(path) {
            return ToolResult::error(format!(
                "Access denied: '{}' is outside the allowed directory.",
                path
            ));
        }

        let resolved_path = path_manager.resolve(path);
        self.list_dir_internal(&resolved_path, recursive).await
    }
}

async fn list_dir_recursive(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    entries: &mut Vec<String>,
) -> std::io::Result<()> {
    if depth >= max_depth {
        return Ok(());
    }

    let mut read_dir = fs::read_dir(dir).await?;
    let mut items = Vec::new();

    while let Some(entry) = read_dir.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and common ignored directories
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }

        let metadata = entry.metadata().await?;
        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(&entry.path())
            .display()
            .to_string();

        if metadata.is_dir() {
            items.push((format!("{}📁 {}/", "  ".repeat(depth), relative), true, entry.path()));
        } else {
            let size = format_size(metadata.len());
            items.push((
                format!("{}📄 {} ({})", "  ".repeat(depth), relative, size),
                false,
                entry.path(),
            ));
        }
    }

    // Sort: directories first, then files
    items.sort_by(|a, b| {
        if a.1 == b.1 {
            a.0.cmp(&b.0)
        } else if a.1 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    for (display, is_dir, path) in items {
        entries.push(display);
        if is_dir {
            Box::pin(list_dir_recursive(base, &path, depth + 1, max_depth, entries)).await?;
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

impl ListDirTool {
    async fn list_dir_internal(&self, path: &Path, recursive: bool) -> ToolResult {
        if !path.exists() {
            return ToolResult::error(format!("Directory '{}' does not exist", path.display()));
        }

        if !path.is_dir() {
            return ToolResult::error(format!("'{}' is not a directory", path.display()));
        }

        let max_depth = if recursive { 3 } else { 1 };
        let mut entries = Vec::new();

        if let Err(e) = list_dir_recursive(path, path, 0, max_depth, &mut entries).await {
            return ToolResult::error(format!(
                "Failed to list directory '{}': {}",
                path.display(),
                e
            ));
        }

        if entries.is_empty() {
            return ToolResult::success(format!("Directory '{}' is empty", path.display()));
        }

        let result = format!(
            "Contents of '{}':\n\n{}",
            path.display(),
            entries.join("\n")
        );

        ToolResult::success(result)
    }

    /// Structured directory list for the Web UI file explorer.
    /// Returns a tree of `DirEntry` nodes suitable for JSON serialization.
    /// Skips hidden files, `target/`, and `node_modules/` — same policy as the LLM tool.
    pub async fn list_structured(path: &Path, project_dir: &Path) -> Result<Vec<DirEntry>, String> {
        let resolved = resolve_path(path, project_dir);
        if !resolved.exists() {
            return Err(format!("Directory '{}' does not exist", resolved.display()));
        }
        if !resolved.is_dir() {
            return Err(format!("'{}' is not a directory", resolved.display()));
        }
        let children = list_entries(project_dir, &resolved).await
            .map_err(|e| format!("Failed to list directory: {}", e))?;
        Ok(children)
    }
}

/// List a single directory level, returning structured entries.
/// Directories first, then files. Skips hidden, target/, node_modules/.
async fn list_entries(base: &Path, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
    let mut read_dir = fs::read_dir(dir).await?;
    let mut items: Vec<DirEntry> = Vec::new();

    while let Some(entry) = read_dir.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and common ignored directories
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }

        let metadata = entry.metadata().await?;
        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(&entry.path())
            .display()
            .to_string();

        if metadata.is_dir() {
            items.push(DirEntry {
                name,
                path: relative,
                is_dir: true,
                size: None,
                children: Vec::new(),
            });
        } else {
            items.push(DirEntry {
                name,
                path: relative,
                is_dir: false,
                size: Some(metadata.len()),
                children: Vec::new(),
            });
        }
    }

    // Sort: directories first, then by name
    items.sort_by(|a, b| {
        if a.is_dir == b.is_dir {
            a.name.cmp(&b.name)
        } else if a.is_dir {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    Ok(items)
}

fn resolve_path(path: &Path, project_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    }
}

// Keep old resolve_path for backward compatibility
fn resolve_path_old(path: &str, project_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}
