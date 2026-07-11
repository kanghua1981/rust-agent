// ═══════════════════════════════════════════════════════════════════
//  Project-First 迁移工具
//  将旧的 localStorage 数据（presets / connectionHistory）
//  一次性转换为 ProjectDefinition[] 格式。
//
//  幂等：设置标记 _migrated_project_first_v1，重复执行无副作用。
//  安全：永远不删除旧 key，只写新数据 + 标记。
// ═══════════════════════════════════════════════════════════════════

import type { ProjectDefinition } from '../types/agent';

const MIGRATION_KEY = '_migrated_project_first_v1';
const LEGACY_CONFIG_KEY = 'rust-agent-config';

export interface MigrationResult {
  projects: ProjectDefinition[];
  migrated: boolean;
}

/**
 * Check if migration has already been performed.
 */
export function isMigrationDone(): boolean {
  try {
    return localStorage.getItem(MIGRATION_KEY) === '1';
  } catch {
    return false;
  }
}

/**
 * Run the one-time migration.  Idempotent — safe to call on every page load.
 * Returns the list of ProjectDefinitions that were converted.
 */
export function runMigration(): MigrationResult {
  if (isMigrationDone()) {
    return { projects: [], migrated: false };
  }

  try {
    const raw = localStorage.getItem(LEGACY_CONFIG_KEY);
    if (!raw) {
      // No legacy data — mark as migrated so we don't try again
      localStorage.setItem(MIGRATION_KEY, '1');
      return { projects: [], migrated: false };
    }

    const parsed = JSON.parse(raw);
    const data = parsed.state ?? parsed;
    const projects: ProjectDefinition[] = [];
    const seen = new Set<string>();

    // ── Source 1: ConfigPreset[] → ProjectDefinition[] ──────────────
    const presets: any[] = data.presets || [];
    for (const p of presets) {
      if (!p.id || seen.has(p.id)) continue;
      const workdir = p.workdir || '';
      projects.push({
        id: p.id,
        label: p.name || workdir.split('/').filter(Boolean).pop() || p.serverUrl || '未命名项目',
        serverUrl: p.serverUrl || '',
        workdir,
        isolation: p.isolation || 'normal',
        agentMode: p.agentMode || 'auto',
        autoApprove: p.autoApprove ?? false,
        newSessionOnConnect: p.newSessionOnConnect ?? p.newSession ?? false,
        createdAt: p.createdAt || new Date().toISOString(),
        updatedAt: p.updatedAt || p.createdAt || new Date().toISOString(),
      });
      seen.add(p.id);
    }

    // ── Source 2: ConnectionHistory[] → ProjectDefinition[] ─────────
    const history: any[] = data.connectionHistory || [];
    for (const h of history) {
      const id = h.projectId || h.id;
      if (!id || seen.has(id)) continue;
      const workdir = h.workdir || '';
      projects.push({
        id,
        label: workdir.split('/').filter(Boolean).pop() || h.serverUrl || '未命名项目',
        serverUrl: h.serverUrl || '',
        workdir,
        isolation: 'normal',
        agentMode: 'auto',
        autoApprove: false,
        newSessionOnConnect: false,
        createdAt: h.connectedAt ? new Date(h.connectedAt).toISOString() : new Date().toISOString(),
        updatedAt: h.lastConnectedAt ? new Date(h.lastConnectedAt).toISOString() : new Date().toISOString(),
      });
      seen.add(id);
    }

    // Write migration marker (do NOT delete old key)
    localStorage.setItem(MIGRATION_KEY, '1');
    console.log(
      `[migration] Converted ${projects.length} legacy presets/history entries to projects`
    );
    return { projects, migrated: projects.length > 0 };

  } catch (e) {
    console.warn('[migration] Failed:', e);
    return { projects: [], migrated: false };
  }
}
