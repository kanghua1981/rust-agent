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
//! let presets = db.list_presets()?;
//! db.save_preset(&Preset { name: "test".into(), ..Default::default() })?;
//! ```

pub mod models;
pub mod migration;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, params};

pub use models::*;

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

    // ── Preset CRUD ───────────────────────────────────────────────────

    pub fn list_presets(&self) -> rusqlite::Result<Vec<Preset>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, server_url, workdir, model, auto_approve,
                    agent_mode, isolation, new_session, icon, color,
                    tags, sort_order, node_ref, created_at, updated_at
             FROM presets ORDER BY sort_order, name"
        )?;
        let rows: Vec<Preset> = stmt.query_map([], |row| {
            let tags_str: String = row.get(11)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Preset {
                id:           row.get(0)?,
                name:         row.get(1)?,
                server_url:   row.get(2)?,
                workdir:      row.get(3)?,
                model:        row.get(4)?,
                auto_approve: row.get::<_, i32>(5)? != 0,
                agent_mode:   row.get(6)?,
                isolation:    row.get(7)?,
                new_session:  row.get::<_, i32>(8)? != 0,
                icon:         row.get(9)?,
                color:        row.get(10)?,
                tags,
                sort_order:   row.get(12)?,
                node_ref:     row.get(13)?,
                created_at:   row.get(14)?,
                updated_at:   row.get(15)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_preset(&self, id: &str) -> rusqlite::Result<Option<Preset>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, server_url, workdir, model, auto_approve,
                    agent_mode, isolation, new_session, icon, color,
                    tags, sort_order, node_ref, created_at, updated_at
             FROM presets WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            let tags_str: String = row.get(11)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Preset {
                id:           row.get(0)?,
                name:         row.get(1)?,
                server_url:   row.get(2)?,
                workdir:      row.get(3)?,
                model:        row.get(4)?,
                auto_approve: row.get::<_, i32>(5)? != 0,
                agent_mode:   row.get(6)?,
                isolation:    row.get(7)?,
                new_session:  row.get::<_, i32>(8)? != 0,
                icon:         row.get(9)?,
                color:        row.get(10)?,
                tags,
                sort_order:   row.get(12)?,
                node_ref:     row.get(13)?,
                created_at:   row.get(14)?,
                updated_at:   row.get(15)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn save_preset(&self, preset: &Preset) -> rusqlite::Result<()> {
        let tags_json = serde_json::to_string(&preset.tags).unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO presets (id, name, server_url, workdir, model, auto_approve,
                     agent_mode, isolation, new_session, icon, color, tags, sort_order,
                     node_ref, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,
                         ?14, datetime('now'), datetime('now'))
                 ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name, server_url=excluded.server_url,
                     workdir=excluded.workdir, model=excluded.model,
                     auto_approve=excluded.auto_approve, agent_mode=excluded.agent_mode,
                     isolation=excluded.isolation, new_session=excluded.new_session,
                     icon=excluded.icon, color=excluded.color, tags=excluded.tags,
                     sort_order=excluded.sort_order, node_ref=excluded.node_ref,
                     updated_at=datetime('now')",
                params![
                    preset.id, preset.name, preset.server_url,
                    preset.workdir, preset.model,
                    preset.auto_approve as i32, preset.agent_mode,
                    preset.isolation, preset.new_session as i32,
                    preset.icon, preset.color, tags_json,
                    preset.sort_order,
                    preset.node_ref,
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_preset(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| conn.execute("DELETE FROM presets WHERE id = ?1", params![id]))?;
        Ok(())
    }

    // ── Node CRUD (server-managed workspaces) ───────────────────────────

    pub fn list_nodes(&self) -> rusqlite::Result<Vec<Node>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, workdir, description, isolation,
                    sandbox, exec_mode, tags, created_at, updated_at
             FROM nodes ORDER BY name"
        )?;
        let rows: Vec<Node> = stmt.query_map([], |row| {
            let tags_str: String = row.get(7)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Node {
                id:          row.get(0)?,
                name:        row.get(1)?,
                workdir:     row.get(2)?,
                description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                isolation:   row.get(4)?,
                sandbox:     row.get::<_, i32>(5)? != 0,
                exec_mode:   row.get(6)?,
                tags,
                created_at:  row.get(8)?,
                updated_at:  row.get(9)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn save_node(&self, node: &Node) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let tags_json = serde_json::to_string(&node.tags).unwrap_or_default();
        with_retry(|| {
            conn.execute(
                "INSERT INTO nodes (id, name, workdir, description, isolation,
                 sandbox, exec_mode, tags, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    workdir = excluded.workdir,
                    description = excluded.description,
                    isolation = excluded.isolation,
                    sandbox = excluded.sandbox,
                    exec_mode = excluded.exec_mode,
                    tags = excluded.tags,
                    updated_at = excluded.updated_at",
                params![
                    node.id, node.name, node.workdir, node.description,
                    node.isolation, node.sandbox as i32, node.exec_mode,
                    tags_json, node.created_at, node.updated_at
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| conn.execute("DELETE FROM nodes WHERE id = ?1", params![id]))?;
        Ok(())
    }

    // ── Peer CRUD (remote agent servers for discovery) ─────────────────

    pub fn list_peers(&self) -> rusqlite::Result<Vec<Peer>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, token, tags, enabled, created_at, updated_at
             FROM peers ORDER BY name"
        )?;
        let rows: Vec<Peer> = stmt.query_map([], |row| {
            let tags_str: String = row.get(4)?;
            Ok(Peer {
                id:         row.get(0)?,
                name:       row.get(1)?,
                url:        row.get(2)?,
                token:      row.get(3)?,
                tags:       serde_json::from_str(&tags_str).unwrap_or_default(),
                enabled:    row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// List only enabled peers (used by probe loop).
    pub fn list_enabled_peers(&self) -> rusqlite::Result<Vec<Peer>> {
        let all = self.list_peers()?;
        Ok(all.into_iter().filter(|p| p.enabled).collect())
    }

    pub fn save_peer(&self, peer: &Peer) -> rusqlite::Result<()> {
        let tags_json = serde_json::to_string(&peer.tags).unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO peers (id, name, url, token, tags, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                     name=?2, url=?3, token=?4, tags=?5, enabled=?6, updated_at=?8",
                params![
                    peer.id,
                    peer.name,
                    peer.url,
                    peer.token,
                    tags_json,
                    peer.enabled as i32,
                    peer.created_at,
                    peer.updated_at,
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_peer(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| conn.execute("DELETE FROM peers WHERE id = ?1", params![id]))?;
        Ok(())
    }

    // ── Workflow CRUD ─────────────────────────────────────────────────

    pub fn list_workflows(&self) -> rusqlite::Result<Vec<Workflow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, enabled, default_timeout, created_at, updated_at
             FROM workflows ORDER BY name"
        )?;
        let workflows: Vec<Workflow> = stmt.query_map([], |row| {
            Ok(Workflow {
                id:              row.get(0)?,
                name:            row.get(1)?,
                description:     row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                enabled:         row.get::<_, i32>(3)? != 0,
                default_timeout: row.get(4)?,
                created_at:      row.get(5)?,
                updated_at:      row.get(6)?,
                stages:          vec![],
            })
        })?.collect::<rusqlite::Result<_>>()?;
        Ok(workflows)
    }

    pub fn get_workflow(&self, id: &str) -> rusqlite::Result<Option<Workflow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, enabled, default_timeout, created_at, updated_at
             FROM workflows WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Workflow {
                id:              row.get(0)?,
                name:            row.get(1)?,
                description:     row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                enabled:         row.get::<_, i32>(3)? != 0,
                default_timeout: row.get(4)?,
                created_at:      row.get(5)?,
                updated_at:      row.get(6)?,
                stages:          vec![],
            })
        })?;
        let mut wf = match rows.next().transpose()? {
            Some(w) => w,
            None => return Ok(None),
        };
        // Load stages within the same lock scope (avoids Mutex reentrancy deadlock)
        let mut stage_stmt = conn.prepare(
            "SELECT id, workflow_id, preset_id, stage_order, stage_group,
                    input_template, output_key, condition, timeout_secs,
                    retry_count, auto_approve
             FROM workflow_stages WHERE workflow_id = ?1 ORDER BY stage_order"
        )?;
        let stage_rows: Vec<WorkflowStage> = stage_stmt.query_map(params![id], |row| {
            Ok(WorkflowStage {
                id:             row.get(0)?,
                workflow_id:    row.get(1)?,
                preset_id:      row.get(2)?,
                stage_order:    row.get(3)?,
                stage_group:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                input_template: row.get(5)?,
                output_key:     row.get(6)?,
                condition:      row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "always".into()),
                timeout_secs:   row.get(8)?,
                retry_count:    row.get(9)?,
                auto_approve:   row.get::<_, i32>(10)? != 0,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        wf.stages = stage_rows;
        Ok(Some(wf))
    }

    pub fn save_workflow(&self, wf: &Workflow) -> rusqlite::Result<()> {
        // Acquire the lock once for the whole save (insert wf + replace stages)
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO workflows (id, name, description, enabled, default_timeout,
                     created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5, datetime('now'), datetime('now'))
                 ON CONFLICT(id) DO UPDATE SET
                     name=excluded.name, description=excluded.description,
                     enabled=excluded.enabled, default_timeout=excluded.default_timeout,
                     updated_at=datetime('now')",
                params![wf.id, wf.name, wf.description, wf.enabled as i32, wf.default_timeout],
            )?;
            // Delete old stages and re-insert (within same locked scope)
            conn.execute("DELETE FROM workflow_stages WHERE workflow_id = ?1", params![wf.id])?;
            for s in &wf.stages {
                conn.execute(
                    "INSERT INTO workflow_stages (id, workflow_id, preset_id, stage_order,
                         stage_group, input_template, output_key, condition, timeout_secs,
                         retry_count, auto_approve)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![
                        s.id, wf.id, s.preset_id, s.stage_order,
                        s.stage_group, s.input_template, s.output_key,
                        s.condition, s.timeout_secs, s.retry_count,
                        s.auto_approve as i32,
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_workflow(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| conn.execute("DELETE FROM workflows WHERE id = ?1", params![id]))?;
        Ok(())
    }

    // ── Stage helpers ─────────────────────────────────────────────────

    fn load_stages_for(&self, workflow_id: &str) -> rusqlite::Result<Vec<WorkflowStage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, preset_id, stage_order, stage_group,
                    input_template, output_key, condition, timeout_secs,
                    retry_count, auto_approve
             FROM workflow_stages WHERE workflow_id = ?1 ORDER BY stage_order"
        )?;
        let rows: Vec<WorkflowStage> = stmt.query_map(params![workflow_id], |row| {
            Ok(WorkflowStage {
                id:             row.get(0)?,
                workflow_id:    row.get(1)?,
                preset_id:      row.get(2)?,
                stage_order:    row.get(3)?,
                stage_group:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                input_template: row.get(5)?,
                output_key:     row.get(6)?,
                condition:      row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "always".into()),
                timeout_secs:   row.get(8)?,
                retry_count:    row.get(9)?,
                auto_approve:   row.get::<_, i32>(10)? != 0,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ── Preferences ───────────────────────────────────────────────────

    pub fn create_run(&self, run: &WorkflowRun) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO workflow_runs (id, workflow_id, workflow_name, trigger,
                     status, task, started_at)
                 VALUES (?1,?2,?3,?4,?5,?6, datetime('now'))",
                params![run.id, run.workflow_id, run.workflow_name,
                        run.trigger, run.status, run.task],
            )
        })?;
        Ok(())
    }

    pub fn update_run_status(&self, id: &str, status: &str, error: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "UPDATE workflow_runs SET status = ?1, error_message = ?2,
                     finished_at = CASE WHEN ?1 IN ('success','failed','cancelled')
                     THEN datetime('now') ELSE finished_at END
                 WHERE id = ?3",
                params![status, error, id],
            )
        })?;
        Ok(())
    }

    pub fn save_stage_result(&self, sr: &StageResult) -> rusqlite::Result<()> {
        let tool_json = serde_json::to_string(&sr.tool_calls).unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO stage_results (id, run_id, stage_id, stage_order,
                     preset_name, status, input_prompt, output_text, output_summary,
                     tokens_used, tool_calls, started_at, finished_at,
                     error_message, retry_attempt)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,
                         datetime('now'), datetime('now'), ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                     status=excluded.status, output_text=excluded.output_text,
                     output_summary=excluded.output_summary,
                     tokens_used=excluded.tokens_used, tool_calls=excluded.tool_calls,
                     finished_at=datetime('now'),
                     error_message=excluded.error_message,
                     retry_attempt=excluded.retry_attempt",
                params![
                    sr.id, sr.run_id, sr.stage_id, sr.stage_order,
                    sr.preset_name, sr.status, sr.input_prompt,
                    sr.output_text, sr.output_summary,
                    sr.tokens_used, tool_json,
                    sr.error_message, sr.retry_attempt,
                ],
            )
        })?;
        Ok(())
    }

    pub fn list_runs(&self, limit: usize) -> rusqlite::Result<Vec<WorkflowRun>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, workflow_name, trigger, status, task,
                    started_at, finished_at, total_tokens, error_message
             FROM workflow_runs ORDER BY started_at DESC LIMIT ?1"
        )?;
        let rows: Vec<WorkflowRun> = stmt.query_map(params![limit as i64], |row| {
            Ok(WorkflowRun {
                id:            row.get(0)?,
                workflow_id:   row.get(1)?,
                workflow_name: row.get(2)?,
                trigger:       row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                status:        row.get(4)?,
                task:          row.get(5)?,
                started_at:    row.get(6)?,
                finished_at:   row.get(7)?,
                total_tokens:  row.get(8)?,
                error_message: row.get(9)?,
                stage_results: vec![],
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn load_stage_results(&self, run_id: &str) -> rusqlite::Result<Vec<StageResult>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, stage_id, stage_order, preset_name, status,
                    input_prompt, output_text, output_summary, tokens_used,
                    tool_calls, started_at, finished_at, error_message, retry_attempt
             FROM stage_results WHERE run_id = ?1 ORDER BY stage_order"
        )?;
        let rows: Vec<StageResult> = stmt.query_map(params![run_id], |row| {
            let tools_str: String = row.get::<_, Option<String>>(10)?.unwrap_or_default();
            let tool_calls: Vec<String> = serde_json::from_str(&tools_str).unwrap_or_default();
            Ok(StageResult {
                id:             row.get(0)?,
                run_id:         row.get(1)?,
                stage_id:       row.get(2)?,
                stage_order:    row.get(3)?,
                preset_name:    row.get(4)?,
                status:         row.get(5)?,
                input_prompt:   row.get(6)?,
                output_text:    row.get(7)?,
                output_summary: row.get(8)?,
                tokens_used:    row.get(9)?,
                tool_calls,
                started_at:     row.get(11)?,
                finished_at:    row.get(12)?,
                error_message:  row.get(13)?,
                retry_attempt:  row.get(14)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ── Preferences ───────────────────────────────────────────────────

    pub fn get_pref(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row(
            "SELECT value FROM user_preferences WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_pref(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        with_retry(|| {
            conn.execute(
                "INSERT INTO user_preferences (key, value, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value,
                     updated_at=datetime('now')",
                params![key, value],
            )
        })?;
        Ok(())
    }

    // ── Convenience: list workflows with stage counts ──────────────────

    pub fn list_workflow_infos(&self) -> rusqlite::Result<Vec<WorkflowInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w.id, w.name, w.description,
                    (SELECT COUNT(*) FROM workflow_stages WHERE workflow_id = w.id) as stage_count,
                    (SELECT status FROM workflow_runs
                     WHERE workflow_id = w.id ORDER BY started_at DESC LIMIT 1) as last_status
             FROM workflows w ORDER BY w.name"
        )?;
        let infos: Vec<WorkflowInfo> = stmt.query_map([], |row| {
            Ok(WorkflowInfo {
                id:              row.get(0)?,
                name:            row.get(1)?,
                description:     row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                stage_count:     row.get::<_, i64>(3)? as usize,
                last_run_status: row.get(4)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(infos)
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
    fn test_open_and_migrate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = GlobalDb::open(&path).unwrap();
        assert!(path.exists());

        // Presets table should exist and be empty
        let presets = db.list_presets().unwrap();
        assert!(presets.is_empty());
    }

    #[test]
    fn test_preset_crud() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = GlobalDb::open(&path).unwrap();

        let mut p = Preset {
            id: "test-1".into(),
            name: "Test Preset".into(),
            server_url: "ws://localhost:9527".into(),
            agent_mode: "auto".into(),
            isolation: "container".into(),
            icon: "🧪".into(),
            tags: vec!["test".into(), "dev".into()],
            ..Default::default()
        };

        // Create
        db.save_preset(&p).unwrap();
        let list = db.list_presets().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Preset");
        assert_eq!(list[0].tags, vec!["test", "dev"]);

        // Update
        p.name = "Updated".into();
        p.tags = vec!["prod".into()];
        db.save_preset(&p).unwrap();
        let reloaded = db.get_preset("test-1").unwrap().unwrap();
        assert_eq!(reloaded.name, "Updated");
        assert_eq!(reloaded.tags, vec!["prod"]);

        // Delete
        db.delete_preset("test-1").unwrap();
        assert!(db.list_presets().unwrap().is_empty());
    }

    #[test]
    fn test_workflow_crud() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = GlobalDb::open(&path).unwrap();

        // Create a dummy preset so FK constraints are satisfied
        db.save_preset(&Preset {
            id: "p1".into(),
            name: "Dummy".into(),
            server_url: "ws://localhost:1".into(),
            ..Default::default()
        }).unwrap();

        let mut wf = Workflow {
            id: "wf-1".into(),
            name: "Test Workflow".into(),
            description: "A test".into(),
            stages: vec![
                WorkflowStage {
                    id: "s1".into(),
                    workflow_id: "wf-1".into(),
                    preset_id: Some("p1".into()),
                    stage_order: 0,
                    input_template: "Plan: {{task}}".into(),
                    output_key: Some("plan".into()),
                    ..Default::default()
                },
                WorkflowStage {
                    id: "s2".into(),
                    workflow_id: "wf-1".into(),
                    stage_order: 1,
                    input_template: "Execute: {{stage.s1.output}}".into(),
                    condition: "on_success".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Create
        db.save_workflow(&wf).unwrap();
        let reloaded = db.get_workflow("wf-1").unwrap().unwrap();
        assert_eq!(reloaded.name, "Test Workflow");
        assert_eq!(reloaded.stages.len(), 2);
        assert_eq!(reloaded.stages[0].output_key.as_deref(), Some("plan"));
        assert_eq!(reloaded.stages[1].condition, "on_success");

        // Update
        wf.name = "Updated WF".into();
        wf.stages.remove(1); // only 1 stage now
        db.save_workflow(&wf).unwrap();
        let reloaded = db.get_workflow("wf-1").unwrap().unwrap();
        assert_eq!(reloaded.name, "Updated WF");
        assert_eq!(reloaded.stages.len(), 1);

        // Delete
        db.delete_workflow("wf-1").unwrap();
        assert!(db.get_workflow("wf-1").unwrap().is_none());
    }

    #[test]
    fn test_run_history() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = GlobalDb::open(&path).unwrap();

        // Create a dummy workflow + stage so FK constraints are satisfied
        db.save_workflow(&Workflow {
            id: "wf-1".into(),
            name: "Dummy".into(),
            stages: vec![WorkflowStage {
                id: "s1".into(),
                workflow_id: "wf-1".into(),
                stage_order: 0,
                ..Default::default()
            }],
            ..Default::default()
        }).unwrap();

        let run = WorkflowRun {
            id: "run-1".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "Test".into(),
            status: "running".into(),
            task: "Do something".into(),
            ..Default::default()
        };

        db.create_run(&run).unwrap();
        db.update_run_status("run-1", "success", None).unwrap();

        let runs = db.list_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");

        // Save stage result
        db.save_stage_result(&StageResult {
            id: "sr-1".into(),
            run_id: "run-1".into(),
            stage_id: "s1".into(),
            stage_order: 0,
            preset_name: Some("Test Preset".into()),
            status: "success".into(),
            output_summary: Some("Done".into()),
            tool_calls: vec!["read_file".into(), "write_file".into()],
            ..Default::default()
        }).unwrap();

        let srs = db.load_stage_results("run-1").unwrap();
        assert_eq!(srs.len(), 1);
        assert_eq!(srs[0].status, "success");
        assert_eq!(srs[0].tool_calls.len(), 2);
    }

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
