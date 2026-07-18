-- Migration 006: Drop presets table
-- Presets have been replaced by Project-First Architecture.
-- The FK from workflow_stages was already dropped in migration 005.

DROP TABLE IF EXISTS presets;
DROP INDEX IF EXISTS idx_presets_name;
