//! Ebook reading tool: extract text from MOBI, EPUB, AZW3, and other ebook formats.
//!
//! Uses a tiered strategy:
//!   1. `ebook-convert` (Calibre) — best quality, supports 20+ formats.
//!   2. `pandoc` — good for EPUB (limited format support).
//!
//! Supported extensions (via Calibre):
//!   mobi, epub, azw, azw3, kfx, fb2, lit, lrf, pdb, rb, snb, tcr, cbz, cbr, djvu, docx, rtf, odt, htmlz

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Recognized ebook extensions (Calibre supports all of these as input)
const EBOOK_EXTENSIONS: &[&str] = &[
    "mobi", "epub", "azw", "azw3", "azw4", "kfx",
    "fb2", "lit", "lrf", "pdb", "rb", "snb", "tcr",
    "cbz", "cbr", "djvu",
    "docx", "rtf", "odt", "htmlz", "txtz",
];

pub struct ReadEbookTool;

#[async_trait::async_trait]
impl Tool for ReadEbookTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_ebook".to_string(),
            description: r#"Extract text content from ebook files (MOBI, EPUB, AZW3, FB2, DOCX, etc.). Uses Calibre's ebook-convert to produce clean plain text. Supports 20+ ebook formats."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the ebook file"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Optional: maximum characters to return (default: 50000). Truncates from the end if exceeded."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path"),
        };

        let resolved = resolve_path(path, project_dir);

        if !resolved.exists() {
            return ToolResult::error(format!("File not found: {}", resolved.display()));
        }

        // Check extension
        let ext = resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "pdf" {
            return ToolResult::error(
                "This is a PDF file. Use the read_pdf tool instead."
            );
        }

        if !EBOOK_EXTENSIONS.contains(&ext.as_str()) {
            return ToolResult::error(format!(
                "Unsupported ebook format '.{}'. Supported: {}",
                ext,
                EBOOK_EXTENSIONS.join(", ")
            ));
        }

        let max_chars = input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(50000) as usize;

        let resolved_str = resolved.display().to_string();

        // Strategy 1: ebook-convert (Calibre) — best, widest format support
        if which_exists("ebook-convert").await {
            match extract_with_calibre(&resolved_str).await {
                Ok(text) if !text.trim().is_empty() => {
                    return make_result(path, &text, max_chars, "ebook-convert (Calibre)");
                }
                Ok(_) => {
                    tracing::debug!("ebook-convert returned empty output, falling back");
                }
                Err(e) => {
                    tracing::debug!("ebook-convert failed: {}, falling back", e);
                }
            }
        }

        // Strategy 2: pandoc — works for EPUB (and a few others)
        if which_exists("pandoc").await && matches!(ext.as_str(), "epub" | "docx" | "odt" | "rtf" | "fb2") {
            match extract_with_pandoc(&resolved_str).await {
                Ok(text) if !text.trim().is_empty() => {
                    return make_result(path, &text, max_chars, "pandoc");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("pandoc failed: {}", e);
                }
            }
        }

        // Nothing worked
        ToolResult::error(format!(
            "No ebook conversion tool found. Please install one of:\n\
             • apt install calibre    (ebook-convert — supports all ebook formats)\n\
             • apt install pandoc     (supports EPUB, DOCX, ODT, RTF)\n\n\
             Alternatively, use run_command to convert '{}' manually.",
            path
        ))
    }
}

/// Convert ebook to TXT using Calibre's ebook-convert.
/// Writes to a temp file, then reads the result.
async fn extract_with_calibre(path: &str) -> Result<String, String> {
    let tmp_file = std::env::temp_dir().join(format!(
        "agent_ebook_{}.txt",
        std::process::id()
    ));
    let tmp_str = tmp_file.display().to_string();

    // Remove old temp file if it exists
    let _ = tokio::fs::remove_file(&tmp_file).await;

    let output = Command::new("ebook-convert")
        .arg(path)
        .arg(&tmp_str)
        // Useful options for clean text output
        .args(["--enable-heuristics"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("ebook-convert spawn failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Clean up
        let _ = tokio::fs::remove_file(&tmp_file).await;
        return Err(format!("ebook-convert failed: {}", stderr));
    }

    // Read the converted text
    let text = tokio::fs::read_to_string(&tmp_file)
        .await
        .map_err(|e| format!("Failed to read converted output: {}", e))?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&tmp_file).await;

    Ok(text)
}

/// Convert EPUB/DOCX/ODT/FB2 to plain text using pandoc.
async fn extract_with_pandoc(path: &str) -> Result<String, String> {
    let output = Command::new("pandoc")
        .arg(path)
        .args(["-t", "plain", "--wrap=auto"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("pandoc spawn failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pandoc failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Build the final result with optional truncation.
fn make_result(path: &str, text: &str, max_chars: usize, method: &str) -> ToolResult {
    let char_count = text.chars().count();

    let content = if char_count > max_chars {
        let truncated = crate::ui::truncate_str(text, max_chars);
        format!(
            "{}\n\n... (truncated: showing {}/{} chars)",
            truncated, max_chars, char_count
        )
    } else {
        text.to_string()
    };

    ToolResult::success(format!(
        "File: {} (extracted via {}, {} chars)\n\n{}",
        path, method, char_count, content
    ))
}

/// Resolve a path (relative to project_dir if not absolute).
fn resolve_path(path: &str, project_dir: &Path) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        project_dir.join(p)
    }
}

/// Check if a command exists on the system.
async fn which_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}
