//! Global SQLite database — WAL mode, multi-process safe.
//!
//! # Architecture
//!
//! Every worker process opens its own `Connection` to the same file.  WAL
//! journal mode + `busy_timeout` make concurrent reads and occasional writes
//! safe across processes.
//!
//! # Usage
//!
//! ```ignore
//! let db = GlobalDb::open_or_create()?;

pub mod migration;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;

/// Handle to the global database.
///
/// Internally uses a `Mutex<Connection>` so a single process never contends
/// with itself.  Inter-process contention is handled by WAL + busy_timeout.
pub struct GlobalDb {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl GlobalDb {
    // ── Lifecycle ──────────────────────────────────────────────────────

    /// Open (or create) the global database at the default path.
    ///
    /// Default path: `~/.config/rust_agent/global.db`.  Creates parent
    /// directories automatically.
    pub fn open_or_create() -> rusqlite::Result<Self> {
        let path = default_db_path();
        Self::open(&path)
    }

    /// Open (or create) the database at a specific path.
    pub fn open(path: &PathBuf) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path)?;

        // ── Multi-process safety pragmas ──────────────────────────────
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;"
        )?;

        // Run schema migrations
        migration::migrate(&conn)?;

        tracing::info!("Global DB opened: {}", path.display());

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.clone(),
        })
    }

    /// Return the database file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    // ── Node CRUD (server-managed workspaces) ───────────────────────────
    #[cfg(test)]
    pub fn get_pref(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row(
            "SELECT value FROM user_preferences WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[cfg(test)]
    pub fn set_pref(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO user_preferences (key, value, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value,
                     updated_at=datetime('now')",
                rusqlite::params![key, value],
            )
        })?;
        Ok(())
    }

}

// ── Helpers ───────────────────────────────────────────────────────────

/// Default location: `~/.config/rust_agent/global.db`
fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("rust_agent")
        .join("global.db")
}

/// Retry a closure on SQLITE_BUSY, with exponential backoff.
fn with_retry<T, F>(mut f: F) -> rusqlite::Result<T>
where
    F: FnMut() -> rusqlite::Result<T>,
{
    const MAX_RETRIES: u32 = 3;
    let mut attempts = 0;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DatabaseBusy
                    && attempts < MAX_RETRIES =>
            {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(200 * attempts as u64));
            }
            Err(e) => return Err(e),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_preferences() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = GlobalDb::open(&path).unwrap();

        // Default prefs from migration
        assert_eq!(db.get_pref("language").unwrap().as_deref(), Some("zh"));

        db.set_pref("language", "en").unwrap();
        assert_eq!(db.get_pref("language").unwrap().as_deref(), Some("en"));

        assert_eq!(db.get_pref("nonexistent").unwrap(), None);
    }
}
