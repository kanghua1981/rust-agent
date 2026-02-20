//! PDF reading tool: extract text from PDF files.
//!
//! Uses a tiered strategy:
//!   1. `marker_single` (best quality — Markdown + LaTeX for math formulas)
//!   2. `pdftotext` (poppler-utils — good for plain text PDFs)
//!   3. `mutool convert` (mupdf fallback)
//!
//! If none of the above are installed, returns an error with install hints.

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct ReadPdfTool;

#[async_trait::async_trait]
impl Tool for ReadPdfTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_pdf".to_string(),
            description: r#"Extract text content from a PDF file. Supports optional page range selection. For PDFs with mathematical formulas, uses marker-pdf (if installed) to output Markdown with LaTeX; otherwise falls back to pdftotext for plain text extraction."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the PDF file to read"
                    },
                    "start_page": {
                        "type": "integer",
                        "description": "Optional: first page to extract (1-based, default: 1)"
                    },
                    "end_page": {
                        "type": "integer",
                        "description": "Optional: last page to extract (inclusive, default: all pages)"
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
        if ext != "pdf" {
            return ToolResult::error(format!(
                "'{}' does not appear to be a PDF file (extension: .{})",
                path, ext
            ));
        }

        let start_page = input
            .get("start_page")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let end_page = input
            .get("end_page")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_chars = input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(50000) as usize;

        let resolved_str = resolved.display().to_string();

        // Strategy 1: marker_single (best for math / academic PDFs)
        if which_exists("marker_single").await {
            match extract_with_marker(&resolved_str, start_page, end_page).await {
                Ok(text) if !text.trim().is_empty() => {
                    return make_result(path, &text, max_chars, "marker (Markdown+LaTeX)");
                }
                Ok(_) => {
                    tracing::debug!("marker returned empty output, falling back");
                }
                Err(e) => {
                    tracing::debug!("marker failed: {}, falling back", e);
                }
            }
        }

        // Strategy 2: pdftotext (poppler-utils)
        if which_exists("pdftotext").await {
            match extract_with_pdftotext(&resolved_str, start_page, end_page).await {
                Ok(text) if !text.trim().is_empty() => {
                    return make_result(path, &text, max_chars, "pdftotext");
                }
                Ok(_) => {
                    tracing::debug!("pdftotext returned empty output, falling back");
                }
                Err(e) => {
                    tracing::debug!("pdftotext failed: {}, falling back", e);
                }
            }
        }

        // Strategy 3: mutool (mupdf)
        if which_exists("mutool").await {
            match extract_with_mutool(&resolved_str, start_page, end_page).await {
                Ok(text) if !text.trim().is_empty() => {
                    return make_result(path, &text, max_chars, "mutool");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("mutool failed: {}", e);
                }
            }
        }

        // Nothing worked
        ToolResult::error(format!(
            "No PDF extraction tool found. Please install one of:\n\
             • pip install marker-pdf   (best quality, supports math formulas)\n\
             • apt install poppler-utils (pdftotext, good for plain text)\n\
             • apt install mupdf-tools  (mutool, lightweight fallback)\n\n\
             Alternatively, use run_command with a tool of your choice to extract '{}'.",
            path
        ))
    }
}

/// Extract using marker_single → Markdown output with LaTeX math
async fn extract_with_marker(
    path: &str,
    start_page: Option<usize>,
    end_page: Option<usize>,
) -> Result<String, String> {
    // marker_single writes output to a directory; we use a temp dir
    let tmp_dir = std::env::temp_dir().join(format!("agent_marker_{}", std::process::id()));
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| format!("failed to create temp dir: {}", e))?;

    let mut args = vec![
        path.to_string(),
        tmp_dir.display().to_string(),
        "--output_format".to_string(),
        "markdown".to_string(),
    ];

    if let Some(start) = start_page {
        args.push("--page_range".to_string());
        let end = end_page.unwrap_or(9999);
        args.push(format!("{}-{}", start.saturating_sub(1), end.saturating_sub(1)));
    }

    let output = Command::new("marker_single")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("marker_single exec failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Clean up temp dir
        tokio::fs::remove_dir_all(&tmp_dir).await.ok();
        return Err(format!("marker_single exited with error: {}", stderr));
    }

    // marker_single outputs a .md file in the target directory
    let mut md_content = String::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&tmp_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(content) = tokio::fs::read_to_string(&p).await {
                    md_content = content;
                    break;
                }
            }
        }
    }

    // Clean up
    tokio::fs::remove_dir_all(&tmp_dir).await.ok();

    Ok(md_content)
}

/// Extract using pdftotext (poppler-utils)
async fn extract_with_pdftotext(
    path: &str,
    start_page: Option<usize>,
    end_page: Option<usize>,
) -> Result<String, String> {
    let mut args: Vec<String> = Vec::new();

    // pdftotext uses -f (first page) and -l (last page), 1-based
    if let Some(start) = start_page {
        args.push("-f".to_string());
        args.push(start.to_string());
    }
    if let Some(end) = end_page {
        args.push("-l".to_string());
        args.push(end.to_string());
    }

    // -layout preserves the original layout (better for tables / columns)
    args.push("-layout".to_string());

    // Input file
    args.push(path.to_string());

    // Output to stdout
    args.push("-".to_string());

    let output = Command::new("pdftotext")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("pdftotext exec failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pdftotext error: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Extract using mutool convert (mupdf)
async fn extract_with_mutool(
    path: &str,
    start_page: Option<usize>,
    end_page: Option<usize>,
) -> Result<String, String> {
    let tmp_file = std::env::temp_dir().join(format!("agent_mutool_{}.txt", std::process::id()));

    let mut args = vec![
        "convert".to_string(),
        "-F".to_string(),
        "text".to_string(),
        "-o".to_string(),
        tmp_file.display().to_string(),
    ];

    // mutool uses page range like "1-5"
    if start_page.is_some() || end_page.is_some() {
        let start = start_page.unwrap_or(1);
        let end_str = end_page
            .map(|e| e.to_string())
            .unwrap_or_else(|| "N".to_string());
        args.push(path.to_string());
        args.push(format!("{}-{}", start, end_str));
    } else {
        args.push(path.to_string());
    }

    let output = Command::new("mutool")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("mutool exec failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tokio::fs::remove_file(&tmp_file).await.ok();
        return Err(format!("mutool error: {}", stderr));
    }

    let content = tokio::fs::read_to_string(&tmp_file)
        .await
        .map_err(|e| format!("failed to read mutool output: {}", e))?;

    tokio::fs::remove_file(&tmp_file).await.ok();
    Ok(content)
}

/// Build the final ToolResult with optional truncation
fn make_result(path: &str, text: &str, max_chars: usize, method: &str) -> ToolResult {
    let char_count = text.len();
    let line_count = text.lines().count();

    let content = if char_count > max_chars {
        // Truncate, keeping the beginning
        let mut end = max_chars;
        while end < char_count && !text.is_char_boundary(end) {
            end += 1;
        }
        format!(
            "{}\n\n... (truncated: showing {}/{} chars, use start_page/end_page to read specific sections)",
            &text[..end], max_chars, char_count
        )
    } else {
        text.to_string()
    };

    ToolResult::success(format!(
        "PDF: {} ({} chars, {} lines, extracted via {})\n\n{}",
        path, char_count, line_count, method, content
    ))
}

/// Check if a command exists on the system (async)
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

fn resolve_path(path: &str, project_dir: &Path) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}
