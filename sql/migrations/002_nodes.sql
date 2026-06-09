-- Migration 002: Nodes table
-- 
-- Replaces presets as the server-side managed workspace list.
-- Nodes are merged with workspaces.toml [[node]] entries and
-- sent to clients as virtual_nodes in the ready event.
--
-- Key difference from presets:
--   Node = server-declared workspace (no serverUrl, scoped to this machine)
--   Preset = client-side connection bookmark (moved to localStorage only)

CREATE TABLE IF NOT EXISTS nodes (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    workdir         TEXT NOT NULL,
    description     TEXT DEFAULT '',
    isolation       TEXT DEFAULT NULL,
    sandbox         INTEGER NOT NULL DEFAULT 0,
    exec_mode       TEXT DEFAULT NULL,
    tags            TEXT DEFAULT '[]',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);
