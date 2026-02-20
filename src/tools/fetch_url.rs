//! URL fetching tool: download a web page and extract readable text content.
//!
//! Strategy (tiered):
//!   1. Fetch raw HTML via `reqwest` (already a dependency).
//!   2. Extract readable content:
//!      a. If `readability-cli` is installed → best quality (Mozilla Readability).
//!      b. If `pandoc` is installed → convert HTML to plain text.
//!      c. Otherwise → built-in regex-based tag stripping (good enough for most pages).

use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct FetchUrlTool;

#[async_trait::async_trait]
impl Tool for FetchUrlTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fetch_url".to_string(),
            description: r#"Fetch the content of a web page and extract its main text. Useful for reading documentation, GitHub issues, Stack Overflow answers, API references, etc. Returns clean text (not raw HTML)."#.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch (must start with http:// or https://)"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Optional: maximum characters to return (default: 30000)"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, _project_dir: &Path) -> ToolResult {
        let url = match input.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("Missing required parameter: url"),
        };

        // Basic validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::error("URL must start with http:// or https://");
        }

        let max_chars = input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000) as usize;

        // Step 1: Fetch the page
        let html = match fetch_html(url).await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(format!("Failed to fetch '{}': {}", url, e)),
        };

        if html.trim().is_empty() {
            return ToolResult::error(format!("Empty response from '{}'", url));
        }

        // Step 2: Extract readable text (tiered strategy)
        let text = extract_text(&html, url).await;

        if text.trim().is_empty() {
            return ToolResult::error(format!(
                "Could not extract text content from '{}' (page may be JS-rendered or empty)",
                url
            ));
        }

        // Step 3: Truncate if needed
        let char_count = text.chars().count();
        let content = if char_count > max_chars {
            let truncated = crate::ui::truncate_str(&text, max_chars);
            format!(
                "{}\n\n... (truncated: showing {}/{} chars)",
                truncated, max_chars, char_count
            )
        } else {
            text.clone()
        };

        ToolResult::success(format!(
            "URL: {} ({} chars extracted)\n\n{}",
            url, char_count, content
        ))
    }
}

/// Fetch raw HTML from a URL using reqwest.
async fn fetch_html(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Mozilla/5.0 (compatible; RustAgent/0.2; +https://github.com)")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("error")));
    }

    // Check content type — only process text-like responses
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("application/pdf") {
        return Err("URL points to a PDF file. Use read_pdf tool instead.".to_string());
    }

    // Limit download size to 5MB
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if bytes.len() > 5 * 1024 * 1024 {
        return Err("Response too large (>5MB). Try a more specific URL.".to_string());
    }

    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Extract readable text from HTML, using the best available method.
async fn extract_text(html: &str, _url: &str) -> String {
    // Strategy 1: readable (readability-cli) — best quality
    if which_exists("readable").await {
        if let Ok(text) = extract_with_readable(html).await {
            if !text.trim().is_empty() {
                return text;
            }
        }
    }

    // Strategy 2: pandoc — good quality
    if which_exists("pandoc").await {
        if let Ok(text) = extract_with_pandoc(html).await {
            if !text.trim().is_empty() {
                return text;
            }
        }
    }

    // Strategy 3: Built-in regex stripping — works for most pages
    strip_html(html)
}

/// Use `readable` CLI (readability-cli npm package) for best extraction.
async fn extract_with_readable(html: &str) -> Result<String, String> {
    let mut child = Command::new("readable")
        .arg("--quiet")
        .arg("--low-confidence=force")
        .arg("-p")  // plain text output
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("readable spawn failed: {}", e))?;

    // Write HTML to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(html.as_bytes()).await.ok();
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("readable failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err("readable returned non-zero exit code".to_string())
    }
}

/// Use `pandoc` to convert HTML → plain text.
async fn extract_with_pandoc(html: &str) -> Result<String, String> {
    let mut child = Command::new("pandoc")
        .args(["-f", "html", "-t", "plain", "--wrap=auto"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("pandoc spawn failed: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(html.as_bytes()).await.ok();
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("pandoc failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err("pandoc returned non-zero exit code".to_string())
    }
}

/// Built-in HTML → text extraction using regex.
/// Not perfect, but handles most documentation / article pages well enough.
fn strip_html(html: &str) -> String {
    // Remove script and style blocks entirely
    let re_script = regex::Regex::new(r"(?is)<(script|style|noscript|svg|head)[\s>].*?</\1>").unwrap();
    let text = re_script.replace_all(html, " ");

    // Remove HTML comments
    let re_comment = regex::Regex::new(r"(?s)<!--.*?-->").unwrap();
    let text = re_comment.replace_all(&text, " ");

    // Replace <br>, <p>, <div>, <li>, <tr>, headings with newlines
    let re_block = regex::Regex::new(r"(?i)<(br|/p|/div|/li|/tr|/h[1-6]|/blockquote)[^>]*>").unwrap();
    let text = re_block.replace_all(&text, "\n");

    // Replace <li> with bullet
    let re_li = regex::Regex::new(r"(?i)<li[^>]*>").unwrap();
    let text = re_li.replace_all(&text, "\n• ");

    // Replace heading tags with markdown-style headers
    let re_h = regex::Regex::new(r"(?i)<h([1-6])[^>]*>").unwrap();
    let text = re_h.replace_all(&text, |caps: &regex::Captures| {
        let level: usize = caps[1].parse().unwrap_or(1);
        format!("\n{} ", "#".repeat(level))
    });

    // Strip all remaining HTML tags
    let re_tags = regex::Regex::new(r"<[^>]+>").unwrap();
    let text = re_tags.replace_all(&text, "");

    // Decode common HTML entities
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®")
        .replace("&trade;", "™");

    // Collapse excessive whitespace (but preserve newlines)
    let re_spaces = regex::Regex::new(r"[^\S\n]+").unwrap();
    let text = re_spaces.replace_all(&text, " ");

    // Collapse 3+ consecutive newlines into 2
    let re_newlines = regex::Regex::new(r"\n{3,}").unwrap();
    let text = re_newlines.replace_all(&text, "\n\n");

    text.trim().to_string()
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
