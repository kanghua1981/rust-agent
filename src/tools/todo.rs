//! Todo list tool — persistent per-project task tracking.
//!
//! Stored in `.agent/todo.json` under the project directory.
//! Sub-agents identify themselves via the `owner` field so all agents
//! share a single file and can see each other's task state.
//!
//! Three tools:
//! - `todo_write`  — atomically replace the full task list
//! - `todo_update` — update a single item's status / active_form / owner
//! - `todo_read`   — return a formatted Markdown view of all tasks

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Tool, ToolDefinition, ToolResult};

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
    /// Which agent owns this task: None = manager, Some("node-gpu") = sub-agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn todo_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".agent").join("todo.json")
}

fn load_todos(project_dir: &Path) -> Vec<TodoItem> {
    let path = todo_path(project_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
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
fn assign_ids(items: &mut Vec<TodoItem>, existing: &[TodoItem]) {
    // Find the max existing numeric id
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

// ── Markdown renderer ─────────────────────────────────────────────────────────

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

// ── TodoWriteTool ─────────────────────────────────────────────────────────────

/// Atomically replace the entire todo list.
pub struct TodoWriteTool;

#[async_trait::async_trait]
impl Tool for TodoWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "todo_write".to_string(),
            description: r#"Write (replace) the full project todo list. Use this to create or reorganize tasks.
Each item needs at minimum a `content` field. Provide the COMPLETE list every time — existing items not included will be removed.
Use `todo_update` for quick single-item status changes during execution."#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "Full list of todo items to save.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Unique short id like 't1'. Auto-assigned if omitted."
                                },
                                "content": {
                                    "type": "string",
                                    "description": "Task description."
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed", "cancelled"],
                                    "description": "Task status. Defaults to 'pending'."
                                },
                                "priority": {
                                    "type": "string",
                                    "enum": ["high", "medium", "low"],
                                    "description": "Task priority. Defaults to 'medium'."
                                },
                                "owner": {
                                    "type": "string",
                                    "description": "Agent node name responsible for this task. Omit for the current agent."
                                },
                                "active_form": {
                                    "type": "string",
                                    "description": "Description of what is currently happening (only for in_progress items)."
                                }
                            },
                            "required": ["content"]
                        }
                    }
                },
                "required": ["items"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let raw_items = match input.get("items").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return ToolResult::error("Missing required parameter: items"),
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

        let md = render_markdown(&new_items);
        ToolResult::success(format!("Todo list saved ({} items).\n\n{md}", new_items.len()))
    }
}

// ── TodoUpdateTool ────────────────────────────────────────────────────────────

/// Update a single todo item's status, active_form, or owner.
pub struct TodoUpdateTool;

#[async_trait::async_trait]
impl Tool for TodoUpdateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "todo_update".to_string(),
            description: r#"Update a single todo item by id. Use this frequently during task execution to keep the list current:
- Set status to `in_progress` when starting a task
- Set `active_form` to describe what you're doing right now
- Set status to `completed` or `cancelled` when done"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The todo item id to update (e.g. 't1')."
                    },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed", "cancelled"],
                        "description": "New status for the item."
                    },
                    "active_form": {
                        "type": "string",
                        "description": "Current action description. Pass null to clear."
                    },
                    "owner": {
                        "type": "string",
                        "description": "Reassign this task to another agent node."
                    }
                },
                "required": ["id"]
            }),
        }
    }

    async fn execute(&self, input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let id = match input.get("id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::error("Missing required parameter: id"),
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

        // active_form: explicit null clears it, string sets it, absent = unchanged
        match input.get("active_form") {
            Some(serde_json::Value::Null) => item.active_form = None,
            Some(v) if v.is_string() => item.active_form = v.as_str().map(|s| s.to_string()),
            _ => {}
        }

        if let Some(owner) = input.get("owner").and_then(|v| v.as_str()) {
            item.owner = if owner.is_empty() { None } else { Some(owner.to_string()) };
        }

        // Auto-clear active_form when task is no longer in_progress
        if item.status != TodoStatus::InProgress {
            item.active_form = None;
        }

        if let Err(e) = save_todos(project_dir, &items) {
            return ToolResult::error(format!("Failed to save todo list: {e}"));
        }

        let updated = items.iter().find(|t| t.id == id).unwrap();
        ToolResult::success(format!(
            "Updated [{}] → status: {}{}",
            id,
            updated.status.as_str(),
            updated.active_form.as_deref().map(|a| format!(", active: {a}")).unwrap_or_default()
        ))
    }
}

// ── TodoReadTool ──────────────────────────────────────────────────────────────

/// Read the current todo list as formatted Markdown.
pub struct TodoReadTool;

#[async_trait::async_trait]
impl Tool for TodoReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "todo_read".to_string(),
            description: "Read the current project todo list. Call this at the start of a session or when you need to check task status.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, _input: &serde_json::Value, project_dir: &Path) -> ToolResult {
        let items = load_todos(project_dir);
        ToolResult::success(render_markdown(&items))
    }
}
