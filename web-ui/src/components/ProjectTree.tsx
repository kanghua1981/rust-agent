import React, { useState, useRef, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';

interface ProjectTreeProps {
  collapsed: boolean;
  onOpenConnect: () => void;
  onSwitchLocalSession?: (name: string) => void;
  onNewLocalSession?: (name: string) => void;
  onConnectProject?: (id: string) => void;
  /** Edit a project — opens the connection dialog in edit mode */
  onEditProject?: (id: string) => void;
}

/** Abbreviate workdir to last two segments */
function shortWorkdir(workdir: string): string {
  const cleaned = workdir.replace(/\/+$/, '');
  const parts = cleaned.split('/').filter(Boolean);
  if (parts.length <= 2) return cleaned;
  return '…/' + parts.slice(-2).join('/');
}

const DOT_STATES = ['connected', 'connecting', 'error'];

const StatusDot: React.FC<{ status: string }> = ({ status }) => (
  <span className={`status-dot ${DOT_STATES.includes(status) ? status : 'disconnected'}`} />
);

interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
  projectId: string;
}

const ContextMenuItem: React.FC<{ label: string; danger?: boolean; onClick: () => void }> = ({ label, danger, onClick }) => (
  <div className={`ctx-menu-item${danger ? ' danger' : ''}`} onClick={onClick}>{label}</div>
);

export const ProjectTree: React.FC<ProjectTreeProps> = ({ collapsed, onOpenConnect, onSwitchLocalSession, onNewLocalSession, onConnectProject, onEditProject }) => {
  const [expanded, setExpanded] = useState(true);
  const [sessionsExpanded, setSessionsExpanded] = useState<Record<string, boolean>>({});
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({ visible: false, x: 0, y: 0, projectId: '' });
  const menuRef = useRef<HTMLDivElement>(null);

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

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        onOpenConnect();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onOpenConnect]);

  if (collapsed) {
    const connectedCount = Object.values(projectSlots).filter(s => s.connectionStatus === 'connected').length;
    return (
      <div className="tree-mini">
        <span
          className={`status-dot ${connectedCount > 0 ? 'connected' : 'disconnected'}`}
          title={`${connectedCount} project(s) connected`}
          onClick={onOpenConnect}
        />
      </div>
    );
  }

  return (
    <div className="tree">
      <div className="tree-header" onClick={() => setExpanded(!expanded)}>
        <p className="tree-title">📁 项目 ({projectList.length})</p>
        <div className="tree-actions">
          <button className="tree-add" onClick={(e) => { e.stopPropagation(); onOpenConnect(); }} title="添加项目 (⌘K)">+</button>
          <span className="tree-chevron" style={{ transform: expanded ? 'rotate(90deg)' : 'none' }}>▶</span>
        </div>
      </div>

      {expanded && (
        <div className="tree-list">
          {projectList.length === 0 ? (
            <p className="tree-empty">
              暂无项目<br />
              <button onClick={onOpenConnect}>+ 添加第一个项目</button>
            </p>
          ) : (
            projectList.map(project => {
              const slot = projectSlots[project.id];
              const status = slot?.connectionStatus ?? 'disconnected';
              const isActive = activeProjectId === project.id;
              const sessions: any[] = slot?.localSessions ?? [];
              const activeSessionName: string | null = slot?.activeSessionName ?? null;
              const sessExpanded = sessionsExpanded[project.id] ?? false;

              return (
                <React.Fragment key={project.id}>
                  <div
                    className={`tree-item${isActive ? ' active' : ''}`}
                    onClick={() => {
                      if (slot) {
                        setActiveProject(project.id);
                      } else {
                        openProject(project.id);
                        onConnectProject?.(project.id);
                      }
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setContextMenu({ visible: true, x: e.clientX, y: e.clientY, projectId: project.id });
                    }}
                  >
                    <StatusDot status={status} />
                    <div className="tree-item-body">
                      <div className="tree-item-name">{project.label || project.id}</div>
                      {project.workdir && <div className="tree-item-sub">{shortWorkdir(project.workdir)}</div>}
                    </div>
                    {status === 'connected' && sessions.length > 0 && (
                      <span
                        className="tree-session-toggle"
                        onClick={(e) => { e.stopPropagation(); setSessionsExpanded(prev => ({ ...prev, [project.id]: !sessExpanded })); }}
                        title={sessExpanded ? '收起会话' : '展开会话'}
                        style={{ transform: sessExpanded ? 'rotate(90deg)' : 'none' }}
                      >▶</span>
                    )}
                  </div>

                  {status === 'connected' && sessExpanded && sessions.length > 0 && (
                    <div className="tree-sessions">
                      {sessions.map(sess => {
                        const name = sess.session_name || sess.id || '(未命名)';
                        const isActiveSess = activeSessionName === name;
                        return (
                          <div
                            key={sess.id}
                            className={`tree-session${isActiveSess ? ' active' : ''}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              if (activeProjectId !== project.id) setActiveProject(project.id);
                              onSwitchLocalSession?.(name);
                            }}
                            title={`${name} — ${sess.message_count ?? 0} 条消息`}
                          >
                            <span>📄</span>
                            <span className="tree-session-name">{name}</span>
                            {sess.message_count !== undefined && <span className="tree-session-count">{sess.message_count}</span>}
                          </div>
                        );
                      })}
                      {onNewLocalSession && (
                        <div
                          className="tree-new-session"
                          onClick={(e) => {
                            e.stopPropagation();
                            const name = window.prompt('输入新会话名称:');
                            if (name?.trim()) {
                              if (activeProjectId !== project.id) setActiveProject(project.id);
                              onNewLocalSession(name.trim());
                            }
                          }}
                          title="新建会话"
                        >
                          <span>＋</span><span>新会话</span>
                        </div>
                      )}
                    </div>
                  )}
                </React.Fragment>
              );
            })
          )}

          {projectList.length > 0 && (
            <button className="tree-add-project" onClick={onOpenConnect}>+ 添加项目</button>
          )}
        </div>
      )}

      {contextMenu.visible && (
        <div className="ctx-menu" ref={menuRef} style={{ left: contextMenu.x, top: contextMenu.y }}>
          <ContextMenuItem
            label="编辑项目"
            onClick={() => { onEditProject?.(contextMenu.projectId); setContextMenu(prev => ({ ...prev, visible: false })); }}
          />
          {projectSlots[contextMenu.projectId] && (
            <ContextMenuItem
              label="断开连接"
              onClick={() => { closeProject(contextMenu.projectId); setContextMenu(prev => ({ ...prev, visible: false })); }}
            />
          )}
          <ContextMenuItem
            label="删除项目"
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
