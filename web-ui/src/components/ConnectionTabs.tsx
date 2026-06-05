import React from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useShallow } from 'zustand/react/shallow';

interface ConnectionTabsProps {
  onNewConnection: () => void;
  /** Switch to a different tab without disconnecting (keeps all WS alive) */
  switchToConnection: (id: string) => void;
  /** Disconnect a specific slot's WebSocket */
  disconnectSlot: (slotId: string) => void;
  /** Connect a specific slot's WebSocket */
  connectSlot: (slotId: string) => void;
}

const statusDot = (status: string) => {
  switch (status) {
    case 'connected': return { color: '#10b981', glow: true };
    case 'connecting': return { color: '#f59e0b', glow: true };
    case 'error': return { color: '#ef4444', glow: false };
    default: return { color: '#6b7280', glow: false };
  }
};

export const ConnectionTabs: React.FC<ConnectionTabsProps> = ({
  onNewConnection,
  switchToConnection,
  disconnectSlot,
  connectSlot,
}) => {
  // ── Subscriptions ──────────────────────────────────────────────────────
  // ❌  OLD: subscribing to full connections map caused re-renders on EVERY
  //          inactive-slot update (streaming token flush every 80ms, event
  //          batch every 200ms).
  // ✅  NEW: subscribe to a single summary string that only changes when
  //          tab-display-relevant fields change.
  const activeConnectionId = useAgentStore(s => s.activeConnectionId);
  const tabSummary = useAgentStore(
    useShallow(s => {
      const entries = Object.values(s.connections ?? {})
        .filter(c => c.id !== 'default')
        .sort((a, b) => a.id.localeCompare(b.id));
      // Flat string that changes ONLY on tab-relevant mutations
      return entries.map(c =>
        `${c.id}|${c.label ?? ''}|${c.serverUrl}|${c.connectionStatus}|${c.isProcessing ? '1' : '0'}`
      ).join('\n');
    })
  );
  // Read full slot data from state snapshot (does NOT create a subscription)
  const removeConnectionSlot = useAgentStore(s => s.removeConnectionSlot);

  // Derive entries from summary string lazily — only recomputes on real tab change
  const entries = React.useMemo(() => {
    const conns = useAgentStore.getState().connections;
    return Object.values(conns).filter(s => s.id !== 'default');
  }, [tabSummary]);  // tabSummary captures all tab-relevant mutations
  const activeId = activeConnectionId;

  // Hide tab bar only when there are no visible connections
  if (entries.length === 0) {
    return null;
  }

  const handleTabClick = (id: string) => {
    if (id === activeId) return;
    // Just swap display — all WS connections stay alive
    switchToConnection(id);
  };

  const handleClose = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (id === activeId) {
      // Closing the active tab: disconnect its WS, remove, auto-switch
      disconnectSlot(id);
      removeConnectionSlot(id);
      // Connect the newly active slot (removeConnectionSlot auto-switches)
      const nextId = useAgentStore.getState().activeConnectionId;
      if (nextId) connectSlot(nextId);
    } else {
      // Closing an inactive tab: disconnect its WS, remove it
      disconnectSlot(id);
      removeConnectionSlot(id);
    }
  };

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      height: '34px',
      background: 'var(--bg2)',
      borderBottom: '1px solid var(--border)',
      flexShrink: 0,
      overflowX: 'auto',
      overflowY: 'hidden',
      padding: '0 4px',
      gap: '2px',
    }}>
      {entries.map(slot => {
        const isActive = slot.id === activeId;
        const dot = statusDot(slot.connectionStatus);
        return (
          <div
            key={slot.id}
            onClick={() => handleTabClick(slot.id)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '5px',
              padding: '4px 10px',
              height: '28px',
              borderRadius: '6px',
              background: isActive ? 'var(--accent-glow)' : 'transparent',
              border: isActive ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
              cursor: isActive ? 'default' : 'pointer',
              fontSize: '12px',
              color: isActive ? 'var(--accent)' : 'var(--text2)',
              fontWeight: isActive ? '500' : '400',
              whiteSpace: 'nowrap',
              flexShrink: 0,
              transition: 'all 0.15s',
              userSelect: 'none',
            }}
            onMouseOver={(e) => { if (!isActive) e.currentTarget.style.background = 'var(--bg3)'; }}
            onMouseOut={(e) => { if (!isActive) e.currentTarget.style.background = 'transparent'; }}
            title={`${slot.serverUrl}${slot.workdir ? ` → ${slot.workdir}` : ''}`}
          >
            {/* Status dot */}
            <span style={{
              width: '6px', height: '6px', borderRadius: '50%',
              background: dot.color,
              boxShadow: dot.glow ? `0 0 4px ${dot.color}` : 'none',
              flexShrink: 0,
            }} />

            {/* Label */}
            <span>{slot.label || (() => { try { return new URL(slot.serverUrl.replace(/^ws(s?):/, 'http$1:')).host; } catch { return slot.serverUrl.replace(/^wss?:\/\//, '').split(':')[0]; } })()}</span>

            {/* Processing spinner */}
            {slot.isProcessing && (
              <span className="spin" style={{ fontSize: '10px', color: 'var(--accent)', flexShrink: 0 }}>⟳</span>
            )}

            {/* Close button */}
            <button
              onClick={(e) => handleClose(e, slot.id)}
              style={{
                width: '16px', height: '16px',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                background: 'transparent',
                border: 'none',
                borderRadius: '3px',
                cursor: 'pointer',
                color: 'var(--text3)',
                fontSize: '10px',
                flexShrink: 0,
                marginLeft: '2px',
                transition: 'all 0.1s',
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.background = 'rgba(239,68,68,0.15)';
                e.currentTarget.style.color = '#ef4444';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = 'transparent';
                e.currentTarget.style.color = 'var(--text3)';
              }}
              title="关闭此连接"
            >
              ✕
            </button>
          </div>
        );
      })}

      {/* New connection button */}
      <button
        onClick={onNewConnection}
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          width: '28px',
          height: '28px',
          borderRadius: '6px',
          background: 'transparent',
          border: '1px dashed var(--border)',
          cursor: 'pointer',
          color: 'var(--text3)',
          fontSize: '14px',
          flexShrink: 0,
          marginLeft: '4px',
          transition: 'all 0.15s',
        }}
        onMouseOver={(e) => {
          e.currentTarget.style.background = 'var(--bg3)';
          e.currentTarget.style.color = 'var(--accent)';
          e.currentTarget.style.borderColor = 'var(--accent)';
        }}
        onMouseOut={(e) => {
          e.currentTarget.style.background = 'transparent';
          e.currentTarget.style.color = 'var(--text3)';
          e.currentTarget.style.borderColor = 'var(--border)';
        }}
        title="新建连接"
      >
        +
      </button>
    </div>
  );
};
