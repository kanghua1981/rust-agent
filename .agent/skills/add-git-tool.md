---
name: Add Git Tool
description: Steps to add a Git tool for branch creation and version control operations
---

# Add Git Tool

## Overview
This skill documents how to add a Git tool to the Rust Coding Agent, allowing the LLM to perform Git operations like creating branches, checking status, and managing version control.

## Implementation Steps

### 1. Create the Git Tool File
Create `src/tools/git.rs` with the following structure:

```rust
use super::{Tool, ToolDefinition, ToolResult};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct GitTool;

#[async_trait::async_trait]
impl Tool for GitTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "git".to_string(),
            description: "Execute Git operations. Use this for creating branches, switching branches, checking status, viewing commit history, and other Git commands.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "The Git operation to perform. Supported operations: status, branch, checkout, commit, log, diff, add, push, pull, fetch, remote, clone, init",
                        "enum": ["status", "branch", "checkout", "commit", "log", "diff", "add", "push", "pull", "fetch", "remote", "clone", "init"]
                    },
                    "args": {
                        "type": "string",
                        "description": "Optional arguments for the Git operation. For example: branch name for checkout, commit message for commit, etc."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional: working directory for the Git command (defaults to current directory)"
                    }
                },
                "required": ["operation"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        // Implementation details...
    }
}
```

### 2. Register the Module
Add the module declaration to `src/tools/mod.rs`:
```rust
pub mod git;
```

### 3. Register the Tool
Add the tool registration in `ToolExecutor::new()`:
```rust
executor.register(Box::new(git::GitTool));
```

### 4. Add to Read-Only Tools (Optional)
If you want the planner to be able to use Git for exploration, add "git" to the `READONLY_TOOLS` array in the `readonly_definitions()` method.

## Supported Operations
The Git tool supports the following operations:
- `status` - Check repository status
- `branch` - Create or list branches
- `checkout` - Switch branches
- `commit` - Commit changes
- `log` - View commit history
- `diff` - Show changes
- `add` - Stage files
- `push`/`pull`/`fetch` - Remote operations
- `remote` - Manage remotes
- `clone`/`init` - Repository setup

## Usage Examples

### Create a new branch:
```json
{
  "operation": "branch",
  "args": "feature/new-feature"
}
```

### Switch to a branch:
```json
{
  "operation": "checkout",
  "args": "feature/new-feature"
}
```

### Check status:
```json
{
  "operation": "status"
}
```

### Commit changes:
```json
{
  "operation": "commit",
  "args": "-m \"Add new feature\""
}
```

## Best Practices
1. **Branch before modifying**: Always create a new branch before making code changes
2. **Check status first**: Use `git status` to understand the current state
3. **Commit frequently**: Make small, focused commits with clear messages
4. **Use descriptive branch names**: Follow naming conventions like `feature/`, `bugfix/`, `hotfix/`

## Integration with LLM Workflow
The Git tool enables the LLM to:
1. Create isolated branches for different tasks
2. Check the current state before making changes
3. Commit changes with appropriate messages
4. Switch between branches as needed
5. View history to understand project evolution
