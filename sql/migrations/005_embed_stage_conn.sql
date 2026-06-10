-- Migration 005: Embed connection info directly in workflow_stages
--
-- Instead of referencing presets via foreign key (which caused FK errors when
-- presets weren't saved), each stage now carries its own connection parameters.
-- The preset_id column is kept for backward compatibility but the FK is dropped.
--
-- Strategy: recreate workflow_stages without the preset FK, add new columns.

-- 1. Add new connection columns to the existing table
ALTER TABLE workflow_stages ADD COLUMN server_url  TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_stages ADD COLUMN workdir     TEXT;
ALTER TABLE workflow_stages ADD COLUMN model       TEXT;
ALTER TABLE workflow_stages ADD COLUMN agent_mode  TEXT NOT NULL DEFAULT 'auto';

-- 2. Recreate the table to drop the foreign key on preset_id
--    (SQLite doesn't support DROP FOREIGN KEY, so we rebuild)
CREATE TABLE workflow_stages_new (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    preset_id       TEXT,                              -- kept for backward compat, no FK
    stage_order     INTEGER NOT NULL,
    stage_group     TEXT DEFAULT 'default',
    input_template  TEXT NOT NULL DEFAULT '{{task}}',
    output_key      TEXT,
    condition       TEXT NOT NULL DEFAULT 'always',
    timeout_secs    INTEGER NOT NULL DEFAULT 300,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    auto_approve    INTEGER NOT NULL DEFAULT 0,
    server_url      TEXT NOT NULL DEFAULT '',
    workdir         TEXT,
    model           TEXT,
    agent_mode      TEXT NOT NULL DEFAULT 'auto',
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

-- 3. Copy all data (including newly added columns)
INSERT INTO workflow_stages_new
    (id, workflow_id, preset_id, stage_order, stage_group,
     input_template, output_key, condition, timeout_secs,
     retry_count, auto_approve, server_url, workdir, model, agent_mode)
SELECT
    id, workflow_id, preset_id, stage_order, stage_group,
    input_template, output_key, condition, timeout_secs,
    retry_count, auto_approve, server_url, workdir, model, agent_mode
FROM workflow_stages;

-- 4. Swap tables
DROP TABLE workflow_stages;
ALTER TABLE workflow_stages_new RENAME TO workflow_stages;

-- 5. Recreate indexes
CREATE INDEX IF NOT EXISTS idx_stages_workflow ON workflow_stages(workflow_id, stage_order);
