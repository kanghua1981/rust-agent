---
name: Browser CDP Implementation
description: How to implement and use browser automation with Chrome DevTools Protocol (CDP) in Rust Agent
---

# Browser CDP Implementation

# Browser CDP Implementation

This skill covers how to implement and use browser automation with Chrome DevTools Protocol (CDP) in the Rust Agent project.

## Overview

The browser tool enables web automation using Chrome DevTools Protocol (CDP) through the `chromiumoxide` crate. It provides a stateful browser session that can be controlled via the agent's tool system.

## Dependencies

Add to `Cargo.toml`:
```toml
# Browser automation with CDP
chromiumoxide = "0.6"
```

## Implementation Details

### Key Components

1. **BrowserTool**: Main tool struct that manages browser state
2. **BrowserState**: Internal struct holding browser and page instances
3. **Async Mutex**: Uses `tokio::sync::Mutex` for thread-safe state management

### Supported Actions

- `navigate`: Navigate to a URL
- `click`: Click on an element by CSS selector
- `type`: Type text into an element
- `screenshot`: Take a screenshot of the page
- `execute_script`: Execute JavaScript and return result
- `get_html`: Get page HTML content
- `get_text`: Get text content of an element
- `find_elements`: Find multiple elements and get their text
- `evaluate`: Generic CDP evaluation (same as execute_script)
- `quit`: Close the browser session

### State Management

The browser session is managed as a shared state using `Arc<AsyncMutex<Option<BrowserState>>>`:
- Lazy initialization: Browser is only launched when first needed
- Single session: Only one browser instance per tool instance
- Proper cleanup: Browser is closed when `quit` action is called

## Usage Examples

### Via Agent CLI

```bash
# Start the agent
./target/release/agent

# Navigate to a website
browser navigate --url https://www.rust-lang.org

# Get page title via JavaScript
browser execute_script --script "return document.title"

# Take a screenshot
browser screenshot --output_path screenshot.png

# Close the browser
browser quit
```

### Via JSON (Stdio Mode)

```json
{"action": "navigate", "url": "https://www.rust-lang.org"}
{"action": "get_html"}
{"action": "screenshot", "output_path": "screenshot.png"}
{"action": "execute_script", "script": "return document.title"}
{"action": "quit"}
```

## Configuration Options

- `headless`: Whether to run browser in headless mode (default: true)
- `wait_seconds`: Seconds to wait after actions like click/type (default: 2)
- `output_path`: Path to save screenshots (default: "screenshot.png")

## Error Handling

The tool provides detailed error messages for common failures:
- Browser launch failures
- Navigation timeouts
- Element not found errors
- JavaScript execution errors
- Screenshot save failures

## Integration with Agent System

1. **Tool Registration**: Added to `ToolExecutor::new()` in `src/tools/mod.rs`
2. **Read-Only Tool**: Included in `readonly_definitions()` for planning phase
3. **JSON Schema**: Full parameter validation via OpenAPI-style schema

## Testing

Run the test script:
```bash
./test_browser_cdp.sh
```

Or manually test:
```bash
# Build the project
cargo build --release

# Run agent and test commands
./target/release/agent
```

## Limitations and Notes

1. **Chrome/Chromium Required**: Requires Chrome/Chromium browser installed
2. **Single Session**: Only one browser instance per agent instance
3. **Memory Usage**: Browser instances consume significant memory
4. **Async Operations**: All browser operations are async and non-blocking
5. **Error Recovery**: Browser crashes require restarting the agent

## Future Enhancements

Potential improvements:
1. Multiple browser tabs support
2. CDP event listening (network requests, console logs)
3. Browser profiling and performance monitoring
4. PDF generation from pages
5. Custom CDP command execution
6. Browser extension support
