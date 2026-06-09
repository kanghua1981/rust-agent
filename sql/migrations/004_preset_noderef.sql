-- Migration 004: Add node_ref column to presets table.
-- Allows a preset to reference a server-side Node for its workdir/isolation/exec_mode.

ALTER TABLE presets ADD COLUMN node_ref TEXT;
