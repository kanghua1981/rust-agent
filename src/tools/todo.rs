//! Todo list tool — persistent per-project task tracking.
//!
//! Stored in `.agent/todo.json`. The main agent and its sub-agents share the
//! file, so `owner` records which one is responsible for an item.
//!
//! One tool with three actions:
//! - `write`  — atomically replace the full task list
//! - `update` — change one item's status / active_form / owner
//! - `read`   — return a formatted Markdown view
//!
//! The list is also injected into every turn's input (see [`current_context`]),
//! so the agent does not have to re-read it to know what it is working on.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Tool, ToolContext, ToolDefinition, ToolResult};

/// Maximum items rendered into the per-turn context block.
///
/// The block is rebuilt every turn, so it stays bounded; in-progress and pending
/// items are kept ahead of finished ones.
const MAX_CONTEXT_ITEMS: usize = 20;

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "pending",
            TodoStatus::InProgress => "in_progress",
            TodoStatus::Completed => "completed",
            TodoStatus::Cancelled => "cancelled",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TodoStatus::Pending),
            "in_progress" => Some(TodoStatus::InProgress),
            "completed" => Some(TodoStatus::Completed),
            "cancelled" => Some(TodoStatus::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "high" => Some(Priority::High),
            "medium" => Some(Priority::Medium),
            "low" => Some(Priority::Low),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Short unique identifier like "t1", "t2".
    pub id: String,
    /// Human-readable task description.
    pub content: String,
    /// Optional description of *current* action (shown while in_progress).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
    pub status: TodoStatus,
    pub priority: Priority,
    /// Which agent owns this task; `None` means the main agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn todo_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".agent").join("todo.json")
}

fn load_todos(project_dir: &Path) -> Vec<TodoItem> {
    let path = todo_path(project_dir);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str(&content) {
        Ok(items) => items,
        Err(e) => {
            // Keep the unreadable file: the next save would otherwise destroy
            // the task list without a trace.
            let backup = path.with_extension(format!("corrupt-{}", now_millis()));
            let _ = std::fs::rename(&path, &backup);
            tracing::warn!("Unreadable {} ({}); kept a copy at {} and started empty",
                path.display(), e, backup.display());
            Vec::new()
        }
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn save_todos(project_dir: &Path, items: &[TodoItem]) -> std::io::Result<()> {
    let path = todo_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(items).unwrap_or_default();
    std::fs::write(&path, json)
}

/// Auto-assign sequential ids to items that don't have one yet.
fn assign_ids(items: &mut [TodoItem], existing: &[TodoItem]) {
    let mut max_id: u32 = existing
        .iter()
        .chain(items.iter())
        .filter_map(|t| t.id.strip_prefix('t').and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);

    for item in items.iter_mut() {
        if item.id.is_empty() {
            max_id += 1;
            item.id = format!("t{}", max_id);
        }
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render_markdown(items: &[TodoItem]) -> String {
    if items.is_empty() {
        return "## Todo List\n\n*(empty)*\n".to_string();
    }

    let mut out = String::from("## Todo List\n");

    let sections = [
        (TodoStatus::InProgress, "🔵 In Progress"),
        (TodoStatus::Pending, "⭕ Pending"),
        (TodoStatus::Completed, "✅ Completed"),
        (TodoStatus::Cancelled, "🚫 Cancelled"),
    ];

    for (status, header) in &sections {
        let group: Vec<&TodoItem> = items.iter().filter(|t| &t.status == status).collect();
        if group.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&format!("### {header}\n"));
        for t in group {
            let priority_mark = match t.priority {
                Priority::High => " ❗",
                Priority::Medium => "",
                Priority::Low => " ↓",
            };
            let owner_part = t
                .owner
                .as_deref()
                .map(|o| format!(" (owner: {o})"))
                .unwrap_or_default();
            let active_part = t
                .active_form
                .as_deref()
                .map(|a| format!(" → {a}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- [{}]{priority_mark} {}{owner_part}{active_part}\n",
                t.id, t.content
            ));
        }
    }

    out
}

/// Render at most `max` items, keeping active work ahead of finished work.
fn render_capped(items: &[TodoItem], max: usize) -> String {
    if items.len() <= max {
        return render_markdown(items);
    }
    let mut ordered: Vec<&TodoItem> = items.iter().collect();
    ordered.sort_by_key(|t| match t.status {
        TodoStatus::InProgress => 0,
        TodoStatus::Pending => 1,
        _ => 2,
    });
    let kept: Vec<TodoItem> = ordered.into_iter().take(max).cloned().collect();
    format!("{}\n*({} more items omitted)*\n",
        render_markdown(&kept), items.len() - max)
}

/// Render the current list for per-turn context injection.
///
/// Returns `None` when the list is empty, so an agent that never uses todos
/// carries no context cost.
pub fn current_context(project_dir: &Path) -> Option<String> {
    let items = load_todos(project_dir);
    if items.is_empty() {
        return None;
    }
    Some(render_capped(&items, MAX_CONTEXT_ITEMS))
}

// ── TodoTool ──────────────────────────────────────────────────────────────────

/// Read, replace, or update the project todo list.
pub struct TodoTool;

impl TodoTool {
    fn write(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let raw_items = match input.get("items").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return ToolResult::error("write requires 'items'"),
        };

        let existing = load_todos(project_dir);
        let mut new_items: Vec<TodoItem> = raw_items
            .iter()
            .filter_map(|v| {
                let content = v.get("content")?.as_str()?.to_string();
                Some(TodoItem {
                    id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    content,
                    active_form: v.get("active_form").and_then(|x| x.as_str()).map(|s| s.to_string()),
                    status: v.get("status")
                        .and_then(|x| x.as_str())
                        .and_then(TodoStatus::from_str)
                        .unwrap_or(TodoStatus::Pending),
                    priority: v.get("priority")
                        .and_then(|x| x.as_str())
                        .and_then(Priority::from_str)
                        .unwrap_or(Priority::Medium),
                    owner: v.get("owner").and_then(|x| x.as_str()).map(|s| s.to_string()),
                })
            })
            .collect();

        assign_ids(&mut new_items, &existing);
        if let Err(e) = save_todos(project_dir, &new_items) {
            return ToolResult::error(format!("Failed to save todo list: {e}"));
        }
        ToolResult::success(format!("Todo list saved ({} items).\n\n{}",
            new_items.len(), render_markdown(&new_items)))
    }

    fn update(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let id = match input.get("id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::error("update requires 'id'"),
        };

        let mut items = load_todos(project_dir);
        let item = match items.iter_mut().find(|t| t.id == id) {
            Some(t) => t,
            None => return ToolResult::error(format!("Todo item '{id}' not found")),
        };

        if let Some(status_str) = input.get("status").and_then(|v| v.as_str()) {
            match TodoStatus::from_str(status_str) {
                Some(s) => item.status = s,
                None => return ToolResult::error(format!("Invalid status: '{status_str}'")),
            }
        }

        // active_form: explicit null clears it, a string sets it, absent = unchanged
        match input.get("active_form") {
            Some(serde_json::Value::Null) => item.active_form = None,
            Some(v) if v.is_string() => item.active_form = v.as_str().map(|s| s.to_string()),
            _ => {}
        }

        if let Some(owner) = input.get("owner").and_then(|v| v.as_str()) {
            item.owner = if owner.is_empty() { None } else { Some(owner.to_string()) };
        }

        // Auto-clear active_form when the task is no longer in_progress.
        if item.status != TodoStatus::InProgress {
            item.active_form = None;
        }

        if let Err(e) = save_todos(project_dir, &items) {
            return ToolResult::error(format!("Failed to save todo list: {e}"));
        }

        let updated = items.iter().find(|t| t.id == id).unwrap();
        ToolResult::success(format!("Updated [{}] → status: {}\n\n{}",
            id, updated.status.as_str(), render_markdown(&items)))
    }
}

#[async_trait::async_trait]
impl Tool for TodoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "todo".to_string(),
            description: r#"Manage the project todo list. It is persisted in .agent/todo.json and injected into your context on every turn, so you always see the current plan.

Actions:
- `write`  — replace the whole list. Pass `items` with the COMPLETE list; omitted items are removed.
- `update` — change one item by `id`: `status` (pending/in_progress/completed/cancelled), `active_form` (what you are doing right now; null clears it), or `owner`.
- `read`   — return the current list.

Every action returns the full list. Keep at most one item in_progress while you work, and mark it completed before moving on."#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["write", "update", "read"],
                        "description": "Which operation to perform."
                    },
                    "items": {
                        "type": "array",
                        "description": "Full list of todo items (action=write).",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Unique short id like 't1'. Auto-assigned if omitted." },
                                "content": { "type": "string", "description": "Task description." },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"], "description": "Defaults to 'pending'." },
                                "priority": { "type": "string", "enum": ["high", "medium", "low"], "description": "Defaults to 'medium'." },
                                "owner": { "type": "string", "description": "Sub-agent responsible for this task. Omit for the main agent." },
                                "active_form": { "type": "string", "description": "What is happening right now (in_progress items)." }
                            },
                            "required": ["content"]
                        }
                    },
                    "id": { "type": "string", "description": "Item id to update (action=update)." },
                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"], "description": "New status (action=update)." },
                    "active_form": { "type": ["string", "null"], "description": "Current action; null clears it (action=update)." },
                    "owner": { "type": "string", "description": "Reassign the item to a sub-agent (action=update)." }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, ctx: &ToolContext<'_>) -> ToolResult {
        let project_dir = ctx.project_dir();
        match input.get("action").and_then(|v| v.as_str()) {
            Some("write") => self.write(input, project_dir),
            Some("update") => self.update(input, project_dir),
            Some("read") => ToolResult::success(render_markdown(&load_todos(project_dir))),
            Some(other) => ToolResult::error(format!(
                "Unknown action '{other}'. Valid actions: write, update, read.")),
            None => ToolResult::error("Missing required parameter: action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_list(dir: &Path, items: &serde_json::Value) -> ToolResult {
        let tool = TodoTool;
        let input = json!({ "action": "write", "items": items });
        futures::executor::block_on(tool.execute(&input, &ToolContext::new(dir, None)))
    }

    #[test]
    fn write_update_read_round_trip() {
        let dir = tempdir().unwrap();
        let out = write_list(dir.path(), &json!([
            { "content": "first task" },
            { "content": "second task", "status": "in_progress" }
        ]));
        assert!(!out.is_error);
        assert!(out.output.contains("first task"));

        let tool = TodoTool;
        let out = futures::executor::block_on(tool.execute(
            &json!({ "action": "update", "id": "t2", "status": "completed" }),
            &ToolContext::new(dir.path(), None),
        ));
        assert!(!out.is_error, "{}", out.output);
        assert!(out.output.contains("completed"));

        let out = futures::executor::block_on(tool.execute(
            &json!({ "action": "read" }), &ToolContext::new(dir.path(), None)));
        assert!(out.output.contains("first task"));
    }

    #[test]
    fn a_corrupt_list_is_backed_up_instead_of_silently_emptied() {
        let dir = tempdir().unwrap();
        write_list(dir.path(), &json!([{ "content": "task" }]));
        std::fs::write(todo_path(dir.path()), "{ not json").unwrap();

        assert!(load_todos(dir.path()).is_empty());
        let kept = std::fs::read_dir(dir.path().join(".agent")).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("todo.corrupt-"))
            .count();
        assert_eq!(kept, 1, "the unreadable file must be kept, not overwritten");
    }

    #[test]
    fn unknown_action_is_rejected() {
        let dir = tempdir().unwrap();
        let tool = TodoTool;
        let out = futures::executor::block_on(tool.execute(
            &json!({ "action": "delete" }), &ToolContext::new(dir.path(), None)));
        assert!(out.is_error);
    }

    #[test]
    fn context_is_absent_until_something_is_written() {
        let dir = tempdir().unwrap();
        assert!(current_context(dir.path()).is_none());

        write_list(dir.path(), &json!([{ "content": "only task" }]));
        let ctx = current_context(dir.path()).expect("non-empty list must be injected");
        assert!(ctx.contains("only task"));
    }

    #[test]
    fn context_keeps_active_work_when_truncating() {
        let dir = tempdir().unwrap();
        let mut items: Vec<serde_json::Value> = (0..MAX_CONTEXT_ITEMS + 5)
            .map(|i| json!({ "content": format!("task {i}") }))
            .collect();
        items.push(json!({ "content": "the active one", "status": "in_progress" }));
        write_list(dir.path(), &serde_json::Value::Array(items));

        let ctx = current_context(dir.path()).unwrap();
        assert!(ctx.contains("the active one"), "in-progress work must survive truncation");
        assert!(ctx.contains("more items omitted"));
    }
}
