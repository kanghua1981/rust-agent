import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { SessionList } from './SessionList';

interface ProjectTreeProps {
  /** Sidebar is collapsed: render a single status dot instead of the tree. */
  collapsed: boolean;
  /** Add / edit a workspace (opens the connection dialog). */
  onOpenConnect: () => void;
  onEditProject: (id: string) => void;
  onConnectProject: (id: string) => void;
  onSwitchToChat: () => void;
  onSwitchLocalSession: (name: string) => void;
  onNewLocalSession: (name: string) => void;
  onDeleteLocalSession: (name: string) => void;
  onRenameLocalSession: (oldName: string, newName: string) => void;
}

/** Abbreviate workdir to last two segments */
function shortWorkdir(workdir: string): string {
  const cleaned = workdir.replace(/\/+$/, '');
  const parts = cleaned.split('/').filter(Boolean);
  if (parts.length <= 2) return cleaned;
  return '…/' + parts.slice(-2).join('/');
}

const DOT_STATES = ['connected', 'connecting', 'error'];

interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
  projectId: string;
}

const ContextMenuItem: React.FC<{ label: string; danger?: boolean; onClick: () => void }> = ({ label, danger, onClick }) => (
  <div className={`ctx-menu-item${danger ? ' danger' : ''}`} onClick={onClick}>{label}</div>
);

/**
 * Workspace tree. A workspace groups sessions: clicking its row only expands or
 * collapses that list, while clicking a session switches into it.
 */
export const ProjectTree: React.FC<ProjectTreeProps> = ({
  collapsed, onOpenConnect, onEditProject, onConnectProject, onSwitchToChat,
  onSwitchLocalSession, onNewLocalSession, onDeleteLocalSession, onRenameLocalSession,
}) => {
  // Explicit expand/collapse choices; a workspace defaults to open when active.
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({ visible: false, x: 0, y: 0, projectId: '' });

  const projects = useAgentStore(s => s.projects ?? {});
  const projectSlots = useAgentStore(s => s.projectSlots ?? {});
  const activeProjectId = useAgentStore(s => s.activeProjectId);
  const openProject = useAgentStore(s => s.openProject);
  const closeProject = useAgentStore(s => s.closeProject);
  const setActiveProject = useAgentStore(s => s.setActiveProject);
  const deleteProject = useAgentStore(s => s.deleteProject);

  const projectList = Object.values(projects);

  useEffect(() => {
    const handler = () => setContextMenu(prev => ({ ...prev, visible: false }));
    if (contextMenu.visible) {
      document.addEventListener('click', handler);
      return () => document.removeEventListener('click', handler);
    }
  }, [contextMenu.visible]);

  if (collapsed) {
    const connectedCount = Object.values(projectSlots).filter(s => s.connectionStatus === 'connected').length;
    return (
      <div className="tree-mini">
        <span
          className={`ws-dot ${connectedCount > 0 ? 'connected' : 'disconnected'}`}
          title={`${connectedCount} 个工作区已连接`}
          onClick={onOpenConnect}
        />
      </div>
    );
  }

  const isOpen = (id: string) => toggled[id] ?? (id === activeProjectId);
  const toggleWorkspace = (id: string) =>
    setToggled(prev => ({ ...prev, [id]: !(prev[id] ?? (id === activeProjectId)) }));
  /** Session actions travel over the active socket, so make the workspace active first. */
  const ensureActive = (id: string) => { if (activeProjectId !== id) setActiveProject(id); };

  return (
    <div className="tree">
      <div className="tree-header">
        <span className="tree-title">工作区 ({projectList.length})</span>
        <div className="tree-actions">
          <button className="tree-add" onClick={onOpenConnect} title="添加工作区 (⌘K)">＋</button>
        </div>
      </div>

      <div className="tree-list">
        {projectList.length === 0 ? (
          <p className="tree-empty">
            暂无工作区<br />
            <button onClick={onOpenConnect}>+ 添加第一个工作区</button>
          </p>
        ) : (
          projectList.map(project => {
            const slot = projectSlots[project.id];
            const status = slot?.connectionStatus ?? 'disconnected';
            const isActive = activeProjectId === project.id;
            const sessions = slot?.localSessions ?? [];
            const activeSessionName = slot?.activeSessionName ?? null;
            const open = isOpen(project.id);

            return (
              <React.Fragment key={project.id}>
                <div
                  className={`ws-item${isActive ? ' active' : ''}`}
                  onClick={() => toggleWorkspace(project.id)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setContextMenu({ visible: true, x: e.clientX, y: e.clientY, projectId: project.id });
                  }}
                  title={`${project.serverUrl}${project.workdir ? ' → ' + project.workdir : ''}`}
                >
                  <span className={`tree-chevron${open ? ' open' : ''}`}>▶</span>
                  <span className={`ws-dot ${DOT_STATES.includes(status) ? status : 'disconnected'}`} />
                  <div className="ws-body">
                    <div className="ws-name">{project.label || project.id}</div>
                    {project.workdir && <div className="ws-sub">{shortWorkdir(project.workdir)}</div>}
                  </div>
                </div>

                {open && (
                  status === 'connected' ? (
                    <SessionList
                      sessions={sessions}
                      activeSessionName={activeSessionName}
                      onSwitchToChat={onSwitchToChat}
                      onSwitchLocalSession={(name) => { ensureActive(project.id); onSwitchLocalSession(name); }}
                      onNewLocalSession={(name) => { ensureActive(project.id); onNewLocalSession(name); }}
                      onDeleteLocalSession={(name) => { ensureActive(project.id); onDeleteLocalSession(name); }}
                      onRenameLocalSession={(o, n) => { ensureActive(project.id); onRenameLocalSession(o, n); }}
                    />
                  ) : status === 'connecting' ? (
                    <div className="sess-empty">连接中…</div>
                  ) : (
                    <div className="sess-empty">
                      未连接
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          if (!slot) openProject(project.id);
                          onConnectProject(project.id);
                        }}
                      >连接</button>
                    </div>
                  )
                )}
              </React.Fragment>
            );
          })
        )}
      </div>

      {contextMenu.visible && (
        <div className="ctx-menu" style={{ left: contextMenu.x, top: contextMenu.y }}>
          <ContextMenuItem
            label="编辑工作区"
            onClick={() => { onEditProject(contextMenu.projectId); setContextMenu(prev => ({ ...prev, visible: false })); }}
          />
          {projectSlots[contextMenu.projectId] && (
            <ContextMenuItem
              label="断开连接"
              onClick={() => { closeProject(contextMenu.projectId); setContextMenu(prev => ({ ...prev, visible: false })); }}
            />
          )}
          <ContextMenuItem
            label="删除工作区"
            danger
            onClick={() => {
              if (projectSlots[contextMenu.projectId]) closeProject(contextMenu.projectId);
              deleteProject(contextMenu.projectId);
              setContextMenu(prev => ({ ...prev, visible: false }));
            }}
          />
        </div>
      )}
    </div>
  );
};
