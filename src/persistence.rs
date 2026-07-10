//! Conversation persistence - save and restore conversation sessions.
//!
//! ## Multi-session support (per-project)
//!
//! Local sessions are stored under `.agent/sessions/<name>.json`.
//! The file `.agent/sessions/_active` holds the name of the most-recently-used
//! session so workers and the CLI can auto-resume.
//!
//! Old single-file `.agent/session.json` is migrated to
//! `.agent/sessions/default.json` on first access.
//!
//! Global sessions are stored in the user data directory
//! (`~/.local/share/rust_agent/sessions/<id>.json`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::conversation::{Conversation, Message};

/// Maximum number of messages kept in each local session file.
/// Older messages are rotated to `.agent/archive/YYYY-MM.jsonl`.
const LOCAL_MAX_MESSAGES: usize = 100;

/// Default session name when none is specified.
const DEFAULT_SESSION_NAME: &str = "default";

/// Metadata for a saved session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    /// Local session name (only for local/named sessions).
    #[serde(default)]
    pub session_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub summary: String, // First user message as summary
    pub working_dir: String,
}

/// A saved session
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSession {
    pub meta: SessionMeta,
    pub system_prompt: String,
    pub messages: Vec<Message>,
}

/// Get the sessions directory
fn sessions_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("rust_agent").join("sessions"))
}

/// Generate a timestamp string
fn now_string() -> String {
    // Simple timestamp without chrono dependency
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Convert to readable format (rough but good enough)
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let remaining_days = days % 365;
    let months = remaining_days / 30 + 1;
    let day = remaining_days % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    let sec = secs % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        years, months, day, hour, min, sec
    )
}

/// Generate a short session ID
fn generate_session_id() -> String {
    let uuid = uuid::Uuid::new_v4().to_string();
    uuid[..8].to_string()
}

/// Derive a stable, per-workdir session ID so global sessions are
/// overwritten in place rather than accumulating indefinitely.
/// The ID is a 12-hex-char hash of the canonical workdir path.
pub fn workdir_to_session_id(workdir: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    workdir.hash(&mut hasher);
    format!("{:012x}", hasher.finish())
}

/// Save conversation to the global sessions directory, keyed by workdir.
/// This is a stable upsert — calling again for the same workdir overwrites.
pub fn save_session_for_workdir(conversation: &Conversation, workdir: &Path) -> Result<()> {
    let id = workdir_to_session_id(workdir);
    save_session(conversation, Some(&id), workdir)?;
    Ok(())
}

/// Save a conversation to disk
pub fn save_session(conversation: &Conversation, session_id: Option<&str>, project_dir: &std::path::Path) -> Result<String> {
    let dir = sessions_dir().context("Cannot determine data directory")?;
    std::fs::create_dir_all(&dir)?;

    let id = session_id
        .map(|s| s.to_string())
        .unwrap_or_else(generate_session_id);

    let summary = conversation
        .messages
        .iter()
        .find(|m| m.role == crate::conversation::Role::User)
        .map(|m| {
            let text = m.text_content();
            crate::ui::truncate_str(&text, 80)
        })
        .unwrap_or_else(|| "(empty)".to_string());

    let now = now_string();

    let session = SavedSession {
        meta: SessionMeta {
            id: id.clone(),
            session_name: None,
            created_at: now.clone(),
            updated_at: now,
            message_count: conversation.messages.len(),
            summary,
            working_dir: project_dir.display().to_string(),
        },
        system_prompt: conversation.system_prompt.clone(),
        messages: conversation.messages.clone(),
    };

    let path = dir.join(format!("{}.json", id));
    let json = serde_json::to_string_pretty(&session)?;
    std::fs::write(&path, json)?;

    Ok(id)
}

/// Load a conversation from disk
pub fn load_session(session_id: &str) -> Result<SavedSession> {
    let dir = sessions_dir().context("Cannot determine data directory")?;
    let path = dir.join(format!("{}.json", session_id));

    if !path.exists() {
        // Try partial match
        let entries = std::fs::read_dir(&dir)?;
        let mut matches = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(session_id) && name.ends_with(".json") {
                matches.push(entry.path());
            }
        }

        match matches.len() {
            0 => anyhow::bail!("Session '{}' not found", session_id),
            1 => {
                let json = std::fs::read_to_string(&matches[0])?;
                let session: SavedSession = serde_json::from_str(&json)?;
                return Ok(session);
            }
            _ => anyhow::bail!(
                "Ambiguous session ID '{}', {} matches found",
                session_id,
                matches.len()
            ),
        }
    }

    let json = std::fs::read_to_string(&path)?;
    let session: SavedSession = serde_json::from_str(&json)?;
    Ok(session)
}

/// List all saved sessions
pub fn list_sessions() -> Result<Vec<SessionMeta>> {
    let dir = match sessions_dir() {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(json) = std::fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<SavedSession>(&json) {
                    sessions.push(session.meta);
                }
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// Delete a session
pub fn delete_session(session_id: &str) -> Result<()> {
    let dir = sessions_dir().context("Cannot determine data directory")?;
    let path = dir.join(format!("{}.json", session_id));

    if path.exists() {
        std::fs::remove_file(&path)?;
    } else {
        anyhow::bail!("Session '{}' not found", session_id);
    }

    Ok(())
}

/// Path to the local sessions directory: `<workdir>/.agent/sessions/`
pub fn local_sessions_dir(workdir: &Path) -> PathBuf {
    workdir.join(".agent").join("sessions")
}

/// Path to the old single-file session: `<workdir>/.agent/session.json`
pub fn local_session_path(workdir: &Path) -> PathBuf {
    workdir.join(".agent").join("session.json")
}

/// Path to the active-session marker: `<workdir>/.agent/sessions/_active`
fn active_session_marker(workdir: &Path) -> PathBuf {
    local_sessions_dir(workdir).join("_active")
}

/// Derive a year-month string from the current unix timestamp, e.g. "2026-03"
fn year_month_string() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let remaining_days = days % 365;
    let months = remaining_days / 30 + 1;
    format!("{:04}-{:02}", years, months)
}

/// Save conversation to a named local session file.
///
/// File: `<workdir>/.agent/sessions/<name>.json`
///
/// If the conversation grows past `LOCAL_MAX_MESSAGES`, the oldest excess
/// messages are appended to `.agent/archive/YYYY-MM.jsonl` and dropped
/// from the active file to keep it lean.
pub fn save_local_named_session(
    name: &str,
    conversation: &Conversation,
    workdir: &Path,
) -> Result<()> {
    let sessions_dir = local_sessions_dir(workdir);
    let agent_dir = workdir.join(".agent");
    std::fs::create_dir_all(&sessions_dir)?;

    let mut messages = conversation.messages.clone();

    // Rotate overflow to archive
    if messages.len() > LOCAL_MAX_MESSAGES {
        let overflow_count = messages.len() - LOCAL_MAX_MESSAGES;
        let overflow: Vec<Message> = messages.drain(..overflow_count).collect();

        let archive_dir = agent_dir.join("archive");
        std::fs::create_dir_all(&archive_dir)?;
        let archive_path = archive_dir.join(format!("{}.jsonl", year_month_string()));

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&archive_path)?;

        use std::io::Write;
        for msg in &overflow {
            let line = serde_json::to_string(msg)?;
            writeln!(file, "{}", line)?;
        }
    }

    let now = now_string();
    let summary = messages
        .iter()
        .find(|m| m.role == crate::conversation::Role::User)
        .map(|m| crate::ui::truncate_str(&m.text_content(), 80))
        .unwrap_or_else(|| "(empty)".to_string());

    let session = SavedSession {
        meta: SessionMeta {
            id: name.to_string(),
            session_name: Some(name.to_string()),
            created_at: now.clone(),
            updated_at: now,
            message_count: messages.len(),
            summary,
            working_dir: workdir.display().to_string(),
        },
        system_prompt: conversation.system_prompt.clone(),
        messages,
    };

    let path = sessions_dir.join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&session)?;
    std::fs::write(&path, json)?;

    Ok(())
}

/// Load a named local session from `<workdir>/.agent/sessions/<name>.json`.
/// Returns `None` if the file does not exist.
pub fn load_local_named_session(name: &str, workdir: &Path) -> Result<Option<SavedSession>> {
    let path = local_sessions_dir(workdir).join(format!("{}.json", name));
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)?;
    let session: SavedSession = serde_json::from_str(&json)?;
    Ok(Some(session))
}

/// List all local named sessions (`.agent/sessions/*.json`).
/// Skips the `_active` marker file.
pub fn list_local_sessions(workdir: &Path) -> Result<Vec<SessionMeta>> {
    let dir = local_sessions_dir(workdir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Skip non-json files and the _active marker
        if !file_name.ends_with(".json") || file_name == "_active" {
            continue;
        }
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(session) = serde_json::from_str::<SavedSession>(&json) {
                sessions.push(session.meta);
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// Delete a named local session file.
pub fn delete_local_named_session(name: &str, workdir: &Path) -> Result<()> {
    let path = local_sessions_dir(workdir).join(format!("{}.json", name));
    if path.exists() {
        std::fs::remove_file(&path)?;
        Ok(())
    } else {
        anyhow::bail!("Local session '{}' not found", name)
    }
}

/// Rename a local session file.
pub fn rename_local_named_session(old_name: &str, new_name: &str, workdir: &Path) -> Result<()> {
    let sessions_dir = local_sessions_dir(workdir);
    let old_path = sessions_dir.join(format!("{}.json", old_name));
    let new_path = sessions_dir.join(format!("{}.json", new_name));

    if !old_path.exists() {
        anyhow::bail!("Local session '{}' not found", old_name);
    }
    if new_path.exists() {
        anyhow::bail!("Local session '{}' already exists", new_name);
    }

    // Also update the session metadata inside the file
    let json = std::fs::read_to_string(&old_path)?;
    let mut session: SavedSession = serde_json::from_str(&json)?;
    session.meta.id = new_name.to_string();
    session.meta.session_name = Some(new_name.to_string());
    session.meta.updated_at = now_string();
    let new_json = serde_json::to_string_pretty(&session)?;
    std::fs::write(&new_path, new_json)?;
    std::fs::remove_file(&old_path)?;

    Ok(())
}

/// Read the name of the last-active local session from `.agent/sessions/_active`.
pub fn read_active_session_name(workdir: &Path) -> Option<String> {
    let marker = active_session_marker(workdir);
    if !marker.exists() {
        return None;
    }
    std::fs::read_to_string(&marker)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Write the active session name to `.agent/sessions/_active`.
pub fn write_active_session_name(workdir: &Path, name: &str) -> Result<()> {
    let dir = local_sessions_dir(workdir);
    std::fs::create_dir_all(&dir)?;
    let marker = active_session_marker(workdir);
    std::fs::write(&marker, name)?;
    Ok(())
}

/// Migrate the old single-file `.agent/session.json` to the new
/// `.agent/sessions/default.json` layout.  Called once at startup.
///
/// Returns `Some("default")` if migration happened, `None` if nothing to do.
pub fn migrate_old_local_session(workdir: &Path) -> Result<Option<String>> {
    let old_path = local_session_path(workdir);
    if !old_path.exists() {
        return Ok(None);
    }

    let sessions_dir = local_sessions_dir(workdir);
    let new_path = sessions_dir.join("default.json");

    // If the new file already exists, the old one is stale — just remove it.
    if new_path.exists() {
        tracing::info!(
            "persistence: removing stale session.json (sessions/default.json already exists)"
        );
        let _ = std::fs::remove_file(&old_path);
        return Ok(None);
    }

    // Read old, write to new location
    let json = std::fs::read_to_string(&old_path)?;
    let mut session: SavedSession = serde_json::from_str(&json)?;

    // Patch metadata for the new layout
    session.meta.id = DEFAULT_SESSION_NAME.to_string();
    session.meta.session_name = Some(DEFAULT_SESSION_NAME.to_string());
    session.meta.updated_at = now_string();

    std::fs::create_dir_all(&sessions_dir)?;
    let new_json = serde_json::to_string_pretty(&session)?;
    std::fs::write(&new_path, new_json)?;

    // Remove old file after successful migration
    let _ = std::fs::remove_file(&old_path);

    tracing::info!(
        "persistence: migrated session.json → sessions/default.json"
    );

    Ok(Some(DEFAULT_SESSION_NAME.to_string()))
}

/// Resolve the session name to use at startup, given an optional CLI/URL override.
///
/// Priority:
/// 1. `cli_override` (from `--session-name` or `?session=` in URL)
/// 2. `.agent/sessions/_active` (last-used session)
/// 3. Fallback to `DEFAULT_SESSION_NAME`
pub fn resolve_session_name(workdir: &Path, cli_override: Option<&str>) -> String {
    if let Some(name) = cli_override.filter(|n| !n.is_empty()) {
        return name.to_string();
    }
    read_active_session_name(workdir).unwrap_or_else(|| DEFAULT_SESSION_NAME.to_string())
}

/// Save conversation to local session (backward-compatible wrapper).
///
/// Delegates to `save_local_named_session("default", ...)`.
pub fn save_local_session(conversation: &Conversation, workdir: &Path) -> Result<()> {
    // Try migration on first access
    let _ = migrate_old_local_session(workdir);
    save_local_named_session(DEFAULT_SESSION_NAME, conversation, workdir)?;
    let _ = write_active_session_name(workdir, DEFAULT_SESSION_NAME);
    Ok(())
}

/// Load the local session (backward-compatible wrapper).
///
/// Tries the new `.agent/sessions/default.json` first, then falls back to
/// the old single-file `.agent/session.json`.  Old file is auto-migrated.
pub fn load_local_session(workdir: &Path) -> Result<Option<SavedSession>> {
    // Try new layout first
    if let Some(session) = load_local_named_session(DEFAULT_SESSION_NAME, workdir)? {
        return Ok(Some(session));
    }
    // Fall back to old single-file layout → migrate on load
    let old_path = local_session_path(workdir);
    if old_path.exists() {
        let json = std::fs::read_to_string(&old_path)?;
        let session: SavedSession = serde_json::from_str(&json)?;
        let _ = migrate_old_local_session(workdir);
        return Ok(Some(session));
    }
    Ok(None)
}

/// Restore a saved session into a Conversation
pub fn restore_conversation(session: &SavedSession) -> Conversation {
    let mut conv = Conversation {
        messages: Vec::new(),
        system_prompt: String::new(),
    };
    conv.system_prompt = session.system_prompt.clone();
    conv.messages = session.messages.clone();
    conv
}
