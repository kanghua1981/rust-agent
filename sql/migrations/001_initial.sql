-- Migration 001: Initial schema
-- Creates all tables for the global database.

-- ═══════════════════════════════════════════════════════════════
-- 迁移版本管理
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS _migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT
);

-- ═══════════════════════════════════════════════════════════════
-- 预设配置
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS presets (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    server_url      TEXT NOT NULL,
    workdir         TEXT,
    model           TEXT,
    auto_approve    INTEGER NOT NULL DEFAULT 0,
    agent_mode      TEXT NOT NULL DEFAULT 'auto',
    isolation       TEXT NOT NULL DEFAULT 'container',
    new_session     INTEGER NOT NULL DEFAULT 0,
    icon            TEXT DEFAULT '🔧',
    color           TEXT,
    tags            TEXT DEFAULT '[]',
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_presets_name ON presets(name);

-- ═══════════════════════════════════════════════════════════════
-- 工作流定义
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT DEFAULT '',
    enabled         INTEGER NOT NULL DEFAULT 1,
    default_timeout INTEGER NOT NULL DEFAULT 600,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════════════════════
-- 工作流阶段
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflow_stages (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    preset_id       TEXT,
    stage_order     INTEGER NOT NULL,
    stage_group     TEXT DEFAULT 'default',
    input_template  TEXT NOT NULL DEFAULT '{{task}}',
    output_key      TEXT,
    condition       TEXT NOT NULL DEFAULT 'always',
    timeout_secs    INTEGER NOT NULL DEFAULT 300,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    auto_approve    INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
    FOREIGN KEY (preset_id)   REFERENCES presets(id)   ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_stages_workflow ON workflow_stages(workflow_id, stage_order);

-- ═══════════════════════════════════════════════════════════════
-- 执行历史
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS workflow_runs (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    workflow_name   TEXT NOT NULL,
    trigger         TEXT NOT NULL DEFAULT 'manual',
    status          TEXT NOT NULL DEFAULT 'pending',
    task            TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT,
    total_tokens    INTEGER DEFAULT 0,
    error_message   TEXT,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_workflow ON workflow_runs(workflow_id);
CREATE INDEX IF NOT EXISTS idx_runs_status   ON workflow_runs(status);
CREATE INDEX IF NOT EXISTS idx_runs_started  ON workflow_runs(started_at DESC);

-- ═══════════════════════════════════════════════════════════════
-- 阶段执行结果
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS stage_results (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    stage_id        TEXT NOT NULL,
    stage_order     INTEGER NOT NULL,
    preset_name     TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    input_prompt    TEXT,
    output_text     TEXT,
    output_summary  TEXT,
    tokens_used     INTEGER DEFAULT 0,
    tool_calls      TEXT DEFAULT '[]',
    started_at      TEXT,
    finished_at     TEXT,
    error_message   TEXT,
    retry_attempt   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (run_id)  REFERENCES workflow_runs(id)  ON DELETE CASCADE,
    FOREIGN KEY (stage_id) REFERENCES workflow_stages(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_sr_run ON stage_results(run_id, stage_order);

-- ═══════════════════════════════════════════════════════════════
-- 用户偏好
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS user_preferences (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO user_preferences (key, value) VALUES
    ('language', 'zh'),
    ('theme', 'dark'),
    ('max_history', '50');
