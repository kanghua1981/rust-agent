import React from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useShallow } from 'zustand/react/shallow';

interface ProjectTabsProps {
  /** Open the project dialog to add a new project */
  onNewProject: () => void;
  /** Disconnect a project's WebSocket */
  disconnectProject: (projectId: string) => void;
  /** Connect a project's WebSocket */
  connectProject: (projectId: string) => void;
}

const statusDot = (status: string) => {
  switch (status) {
    case 'connected': return { color: '#10b981', glow: true };
    case 'connecting': return { color: '#f59e0b', glow: true };
    case 'error': return { color: '#ef4444', glow: false };
    default: return { color: '#6b7280', glow: false };
  }
};

export const ProjectTabs: React.FC<ProjectTabsProps> = ({
  onNewProject,
  disconnectProject,
  connectProject,
}) => {
  // ── Subscriptions ─────────────────────────────────────────────────
  const activeProjectId = useAgentStore(s => s.activeProjectId);
  const tabSummary = useAgentStore(
    useShallow(s => {
      const entries = Object.values(s.projectSlots ?? {})
        .filter(c => c.id !== 'default')
        .sort((a, b) => a.id.localeCompare(b.id));
      return entries.map(c =>
        `${c.id}|${c.label ?? ''}|${c.serverUrl}|${c.connectionStatus}|${c.isProcessing ? '1' : '0'}`
      ).join('\n');
    })
  );
  const closeProject = useAgentStore(s => s.closeProject);
  const setActiveProject = useAgentStore(s => s.setActiveProject);

  // Derive entries from summary string lazily
  const entries = React.useMemo(() => {
    const slots = useAgentStore.getState().projectSlots ?? {};
    return Object.values(slots).filter(s => s.id !== 'default');
  }, [tabSummary]);

  // Hide tab bar when no active projects
  if (entries.length === 0) {
    return null;
  }

  const handleTabClick = (id: string) => {
    if (id === activeProjectId) return;
    const slot = useAgentStore.getState().projectSlots[id];
    setActiveProject(id);
    // Auto-connect if the slot has been created but never connected
    if (slot && slot.connectionStatus === 'disconnected' && slot.serverUrl) {
      connectProject(id);
    }
  };

  const handleClose = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (id === activeProjectId) {
      // Closing active tab: disconnect WS, remove slot, connect next
      disconnectProject(id);
      closeProject(id);
      const nextId = useAgentStore.getState().activeProjectId;
      if (nextId) connectProject(nextId);
    } else {
      // Closing inactive tab
      disconnectProject(id);
      closeProject(id);
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
        const isActive = slot.id === activeProjectId;
        const dot = statusDot(slot.connectionStatus);
        // Label: prefer project label, fallback to hostname
        const label = slot.label || (() => {
          try {
            return new URL(slot.serverUrl.replace(/^ws(s?):/, 'http$1:')).host;
          } catch {
            return slot.serverUrl.replace(/^wss?:\/\//, '').split(':')[0];
          }
        })();

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
            <span>{label}</span>

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
              title="关闭此项目"
            >
              ✕
            </button>
          </div>
        );
      })}

      {/* New project button */}
      <button
        onClick={onNewProject}
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
        title="新建项目"
      >
        +
      </button>
    </div>
  );
};
