-- Migration 003: Peers table
-- 
-- Replaces peers.toml. Peer entries configure remote agent servers
-- that this server probes for virtual nodes. All managed through
-- the WebSocket API and UI — no filesystem config needed.
--
-- Key columns:
--   enabled     = 1 → actively probed; 0 → paused (kept in DB but ignored)
--   tags        = JSON array of strings for routing (e.g. '["gpu","large"]')

CREATE TABLE IF NOT EXISTS peers (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    url             TEXT NOT NULL,
    token           TEXT DEFAULT NULL,
    tags            TEXT DEFAULT '[]',
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_peers_name ON peers(name);
CREATE INDEX IF NOT EXISTS idx_peers_enabled ON peers(enabled);
