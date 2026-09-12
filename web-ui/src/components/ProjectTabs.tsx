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

const dotClass = (status: string): string => {
  switch (status) {
    case 'connected': return 'connected';
    case 'connecting': return 'connecting';
    case 'error': return 'error';
    default: return 'disconnected';
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
    <div className="proj-tabs">
      {entries.map(slot => {
        const isActive = slot.id === activeProjectId;
        const dot = dotClass(slot.connectionStatus);
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
            className={`proj-tab${isActive ? ' active' : ''}`}
            onClick={() => handleTabClick(slot.id)}
            title={`${slot.serverUrl}${slot.workdir ? ` → ${slot.workdir}` : ''}`}
          >
            {/* Status dot */}
            <span className={`proj-dot ${dot}`} />

            {/* Label */}
            <span>{label}</span>

            {/* Processing spinner */}
            {slot.isProcessing && (
              <span className="spin" style={{ fontSize: 10, color: 'var(--accent)', flexShrink: 0 }}>⟳</span>
            )}

            {/* Close button */}
            <button className="proj-close" onClick={(e) => handleClose(e, slot.id)} title="关闭此项目">✕</button>
          </div>
        );
      })}

      {/* New project button */}
      <button className="proj-add" onClick={onNewProject} title="新建项目">+</button>
    </div>
  );
};
