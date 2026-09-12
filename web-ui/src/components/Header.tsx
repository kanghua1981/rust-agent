import React, { useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useShallow } from 'zustand/react/shallow';
import { TokenUsageBadge } from './TokenUsageBadge';
import type { TokenUsage } from '../types/agent';

// ── Tauri frameless window helpers ───────────────────────────────────
// Only functional inside a Tauri desktop app; silently ignored in browsers.
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function tauriWindowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  try {
    const internals = (window as any).__TAURI_INTERNALS__;
    if (!internals) return;
    const label = internals.metadata?.currentWindow?.label ?? 'main';
    const cmd: Record<string, string> = {
      minimize: 'plugin:window|minimize',
      toggleMaximize: 'plugin:window|toggle_maximize',
      close: 'plugin:window|close',
    };
    await internals.invoke(cmd[action], { label });
  } catch { /* not in Tauri — no-op */ }
}

interface HeaderProps {
  activeProjectId: string | null;
  onOpenConnect: () => void;
  onDisconnect: () => void;
}

// Slot snapshot interface: all header-relevant fields, guaranteed non-null.
interface SlotSnapshot {
  connectionStatus: string;
  serverUrl: string;
  workdir: string | undefined;
  isProcessing: boolean;
  sandboxBackend: string;
  pendingChanges: number;
  msgCount: number;
  toolCallCount: number;
  pendingConfCount: number;
  tokenUsage: TokenUsage | null;
}

const emptySlot: SlotSnapshot = {
  connectionStatus: 'disconnected',
  serverUrl: '',
  workdir: undefined,
  isProcessing: false,
  sandboxBackend: 'disabled',
  pendingChanges: 0,
  msgCount: 0,
  toolCallCount: 0,
  pendingConfCount: 0,
  tokenUsage: null,
};

const statusConfig: Record<string, { color: string; label: string; dot: string }> = {
  disconnected: { color: '#6b7280', label: '未连接', dot: '#374151' },
  connecting:   { color: '#f59e0b', label: '连接中…', dot: '#f59e0b' },
  connected:    { color: '#10b981', label: '已连接',  dot: '#10b981' },
  error:        { color: '#ef4444', label: '连接错误', dot: '#ef4444' },
};

export const Header: React.FC<HeaderProps> = ({ activeProjectId, onOpenConnect, onDisconnect }) => {
  const winMinimize = useCallback(() => { tauriWindowAction('minimize'); }, []);
  const winToggleMax = useCallback(() => { tauriWindowAction('toggleMaximize'); }, []);
  const winClose = useCallback(() => { tauriWindowAction('close'); }, []);

  // Read directly from the ACTIVE slot (same pattern as ChatArea): immune to
  // setActiveConnection swaps during inactive-tab event processing.
  const slot = useAgentStore(
    useShallow((s) => {
      const id = activeProjectId;
      if (!id || !s.projectSlots[id]) return emptySlot;
      const c = s.projectSlots[id];
      return {
        connectionStatus: c.connectionStatus,
        serverUrl: c.serverUrl,
        workdir: c.workdir,
        isProcessing: c.isProcessing,
        sandboxBackend: c.sandboxBackend,
        pendingChanges: c.pendingChanges,
        msgCount: c.messages?.length ?? 0,
        toolCallCount: c.toolCalls?.length ?? 0,
        pendingConfCount: c.pendingConfirmations?.length ?? 0,
        tokenUsage: c.tokenUsage ?? null,
      };
    })
  );

  const isolation = useAgentStore(s => s.config.isolation) ?? 'container';

  const {
    connectionStatus, serverUrl, workdir, isProcessing,
    pendingChanges, msgCount, toolCallCount, pendingConfCount, tokenUsage,
  } = slot;

  const cfg = statusConfig[connectionStatus] ?? statusConfig.disconnected;
  const connected = connectionStatus === 'connected';

  return (
    <header
      data-tauri-drag-region={isTauri ? '' : undefined}
      className={`header${isTauri ? ' tauri' : ''}`}
    >
      {/* Logo + title — Tauri drag region */}
      <div className="header-brand" style={{ WebkitAppRegion: isTauri ? 'drag' : undefined } as React.CSSProperties}>
        <div className="header-logo">🤖</div>
        <span className="header-title">Rust Agent</span>
      </div>

      {/* Status pill - clickable to open connection modal */}
      <button className="status-pill" onClick={onOpenConnect}>
        <span className="dot" style={{
          background: cfg.dot,
          boxShadow: connected ? `0 0 6px ${cfg.dot}` : 'none',
        }} />
        <span className="lbl" style={{ color: cfg.color }}>{cfg.label}</span>
        {isProcessing && <span className="spin" style={{ fontSize: 11, color: 'var(--accent)' }}>⟳</span>}
      </button>

      {/* Center: server info */}
      {connected && (
        <div className="header-info">
          <span className="truncate header-mono" style={{ color: 'var(--text3)' }}>{serverUrl}</span>
          {workdir && <span className="truncate header-mono" style={{ color: 'var(--text2)' }}>📂 {workdir}</span>}

          {isolation === 'sandbox' ? (
            <span className={`chip ${pendingChanges > 0 ? 'warn' : 'ok'}`}>
              🔒 沙盒{pendingChanges > 0 ? ` · ${pendingChanges} 待提交` : ''}
            </span>
          ) : isolation === 'container' ? (
            <span className="chip info">🔲 容器</span>
          ) : (
            <span className="chip muted">🕑 无容器</span>
          )}
        </div>
      )}

      {/* Stats badges */}
      {connected && (
        <div className="row" style={{ gap: 6, flexShrink: 0, marginLeft: 'auto' }}>
          <div className="chip neutral"><span>💬</span><span>{msgCount}</span></div>
          <div className="chip neutral"><span>🔨</span><span>{toolCallCount}</span></div>
          <TokenUsageBadge tokenUsage={tokenUsage} />
          {pendingConfCount > 0 && (
            <div className="chip warn"><span>⏳</span><span>{pendingConfCount}</span></div>
          )}
        </div>
      )}

      {/* Disconnect */}
      {connected && (
        <button className="btn-danger" onClick={onDisconnect}>断开</button>
      )}

      {/* Tauri window controls — macOS-style traffic lights */}
      {isTauri && (
        <div
          className="win-ctrl-group"
          style={{ marginLeft: connected ? 0 : 'auto' }}
        >
          <button className="win-ctrl min" onClick={winMinimize} title="最小化"><span className="glyph">─</span></button>
          <button className="win-ctrl max" onClick={winToggleMax} title="最大化">□</button>
          <button className="win-ctrl close" onClick={winClose} title="关闭">✕</button>
        </div>
      )}
    </header>
  );
};
