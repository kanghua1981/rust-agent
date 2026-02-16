use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use tokio::fs;

pub struct ListDirTool;

#[async_trait::async_trait]
impl Tool for ListDirTool {
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

    async fn execute(&self, input: &serde_json::Value) -> ToolResult {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = resolve_path(path);

        if !path.exists() {
            return ToolResult::error(format!("Directory '{}' does not exist", path.display()));
        }

        if !path.is_dir() {
            return ToolResult::error(format!("'{}' is not a directory", path.display()));
        }

        let max_depth = if recursive { 3 } else { 1 };
        let mut entries = Vec::new();

        if let Err(e) = list_dir_recursive(&path, &path, 0, max_depth, &mut entries).await {
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

fn resolve_path(path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}
