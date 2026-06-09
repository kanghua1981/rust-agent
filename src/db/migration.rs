//! Schema migration system.
//!
//! Runs at DB open time. Each migration is a numbered SQL script embedded
//! at compile time via `include_str!`. Migrations are applied inside a
//! transaction so a failure leaves the DB unchanged.

use rusqlite::Connection;

/// A single migration step.
struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

/// All migrations in order.  Add new entries at the end — never reorder.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: include_str!("../../sql/migrations/001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "nodes_table",
        sql: include_str!("../../sql/migrations/002_nodes.sql"),
    },
    Migration {
        version: 3,
        name: "peers_table",
        sql: include_str!("../../sql/migrations/003_peers.sql"),
    },
    Migration {
        version: 4,
        name: "preset_noderef",
        sql: include_str!("../../sql/migrations/004_preset_noderef.sql"),
    },
];

/// Run any pending migrations on `conn`.
///
/// Creates the `_migrations` table on first run, then applies each
/// migration whose version > current_version inside its own transaction.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // Ensure the version-tracking table exists (not managed by migrations)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version     INTEGER PRIMARY KEY,
            applied_at  TEXT NOT NULL DEFAULT (datetime('now')),
            description TEXT
        );"
    )?;

    let current = current_version(conn)?;

    for mig in MIGRATIONS {
        if mig.version > current {
            let tx = conn.unchecked_transaction()?;
            match tx.execute_batch(mig.sql) {
                Ok(()) => {
                    tx.execute(
                        "INSERT INTO _migrations (version, description) VALUES (?1, ?2)",
                        rusqlite::params![mig.version, mig.name],
                    )?;
                    tx.commit()?;
                    tracing::info!(
                        "DB migration {} ({}) applied successfully",
                        mig.version, mig.name
                    );
                }
                Err(e) => {
                    // Transaction is automatically rolled back on drop
                    tracing::error!(
                        "DB migration {} ({}) FAILED: {} — rolling back",
                        mig.version, mig.name, e
                    );
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

/// Return the highest migration version that has been applied, or 0 if none.
fn current_version(conn: &Connection) -> rusqlite::Result<i32> {
    match conn.query_row(
        "SELECT MAX(version) FROM _migrations",
        [],
        |row| row.get::<_, Option<i32>>(0),
    ) {
        Ok(Some(v)) => Ok(v),
        Ok(None) | Err(_) => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_applies_on_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let v = current_version(&conn).unwrap();
        assert!(v >= 1, "expected at least migration 1, got {}", v);

        // Spot-check that a table exists
        let cnt: i64 = conn
            .query_row("SELECT COUNT(*) FROM presets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 0);
    }
}
