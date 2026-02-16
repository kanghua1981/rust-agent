//! Conversation persistence - save and restore conversation sessions.
//!
//! Conversations are saved as JSON files in the data directory,
//! allowing the user to resume previous sessions.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::conversation::{Conversation, Message};

/// Metadata for a saved session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
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
            if text.len() > 80 {
                format!("{}...", &text[..77])
            } else {
                text
            }
        })
        .unwrap_or_else(|| "(empty)".to_string());

    let now = now_string();

    let session = SavedSession {
        meta: SessionMeta {
            id: id.clone(),
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
#[allow(dead_code)]
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
