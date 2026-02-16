# Skill: Add a New Tool

## Steps
1. Create `src/tools/<tool_name>.rs`
2. Define a unit struct: `pub struct MyTool;`
3. Implement the `Tool` trait with two methods:
   - `definition()` → return a `ToolDefinition` with JSON Schema for parameters
   - `execute()` → async logic, return `ToolResult::success()` or `ToolResult::error()`
4. Add `pub mod <tool_name>;` at the top of `src/tools/mod.rs`
5. Register in `ToolExecutor::new()`:
   ```rust
   executor.register(Box::new(tool_name::MyTool));
   ```

## Conventions
- Use `resolve_path()` for all file path parameters (handles relative/absolute)
- All file I/O must use `tokio::fs`, never `std::fs`
- Extract parameters with `input.get("param").and_then(|v| v.as_str())`
- Return early with `ToolResult::error(...)` on missing/invalid params

## Reference
- Simplest example: `src/tools/read_file.rs`
- Complex parameters: `src/tools/edit_file.rs`
