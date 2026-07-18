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

/** Connection status dot */
const StatusDot: React.FC<{ status: string }> = ({ status }) => {
  const color =
    status === 'connected' ? '#10b981' :
    status === 'connecting' ? '#f59e0b' :
    status === 'error' ? '#ef4444' : '#6b7280';
  return (
    <span style={{
      width: '7px', height: '7px', borderRadius: '50%',
      background: color, flexShrink: 0,
      display: 'inline-block', marginRight: '6px',
    }} />
  );
};

/** Simple context menu */
interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
  projectId: string;
}

export const ProjectTree: React.FC<ProjectTreeProps> = ({ collapsed, onOpenConnect, onSwitchLocalSession, onNewLocalSession, onConnectProject, onEditProject }) => {
  const [expanded, setExpanded] = useState(true);
  // Per-project session-list expand state: projectId → boolean
  const [sessionsExpanded, setSessionsExpanded] = useState<Record<string, boolean>>({});
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    visible: false, x: 0, y: 0, projectId: '',
  });

  const menuRef = useRef<HTMLDivElement>(null);

  // Read from store
  const projects = useAgentStore(s => s.projects ?? {});
  const projectSlots = useAgentStore(s => s.projectSlots ?? {});
  const activeProjectId = useAgentStore(s => s.activeProjectId);
  const openProject = useAgentStore(s => s.openProject);
  const closeProject = useAgentStore(s => s.closeProject);
  const setActiveProject = useAgentStore(s => s.setActiveProject);
  const deleteProject = useAgentStore(s => s.deleteProject);

  const projectList = Object.values(projects);

  // Close context menu on outside click
  useEffect(() => {
    const handler = () => setContextMenu(prev => ({ ...prev, visible: false }));
    if (contextMenu.visible) {
      document.addEventListener('click', handler);
      return () => document.removeEventListener('click', handler);
    }
  }, [contextMenu.visible]);

  // Keyboard shortcut
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
    // When sidebar is collapsed, just show connection dot(s)
    const connectedCount = Object.values(projectSlots).filter(
      s => s.connectionStatus === 'connected'
    ).length;
    return (
      <div style={{ display: 'flex', justifyContent: 'center', padding: '8px 0' }}>
        <span title={`${connectedCount} project(s) connected`} style={{
          width: '8px', height: '8px', borderRadius: '50%',
          background: connectedCount > 0 ? '#10b981' : '#6b7280',
          cursor: 'pointer',
        }} onClick={onOpenConnect} />
      </div>
    );
  }

  return (
    <div style={{ marginTop: '12px' }}>
      {/* Header */}
      <div
        style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          marginBottom: '6px', cursor: 'pointer', padding: '4px',
        }}
        onClick={() => setExpanded(!expanded)}
      >
        <p style={{
          fontSize: '10px', fontWeight: '600', color: 'var(--text3)',
          letterSpacing: '0.08em', textTransform: 'uppercase', margin: 0,
        }}>
          📁 项目 ({projectList.length})
        </p>
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          {/* Add project button */}
          <button
            onClick={(e) => { e.stopPropagation(); onOpenConnect(); }}
            title="添加项目 (⌘K)"
            style={{
              fontSize: '12px', color: 'var(--text3)', background: 'transparent',
              border: 'none', cursor: 'pointer', padding: '0 2px', lineHeight: '1',
            }}
          >+</button>
          <span style={{
            fontSize: '10px', color: 'var(--text3)',
            transition: 'transform 0.2s', transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)',
          }}>▶</span>
        </div>
      </div>

      {/* Project list */}
      {expanded && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '3px' }}>
          {projectList.length === 0 ? (
            <p style={{
              fontSize: '11px', color: 'var(--text3)', textAlign: 'center',
              padding: '12px 0', margin: 0,
            }}>
              暂无项目<br />
              <button
                onClick={onOpenConnect}
                style={{
                  marginTop: '6px', fontSize: '11px', color: 'var(--accent)',
                  background: 'transparent', border: 'none', cursor: 'pointer',
                  textDecoration: 'underline',
                }}
              >+ 添加第一个项目</button>
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
                  {/* Project row */}
                  <div
                    onClick={() => {
                      if (slot) {
                        setActiveProject(project.id);
                      } else {
                        openProject(project.id);
                        // Auto-connect after creating slot for the first time
                        onConnectProject?.(project.id);
                      }
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setContextMenu({
                        visible: true,
                        x: e.clientX,
                        y: e.clientY,
                        projectId: project.id,
                      });
                    }}
                    style={{
                      display: 'flex', alignItems: 'center',
                      padding: '5px 8px', borderRadius: '6px',
                      cursor: 'pointer',
                      background: isActive ? 'var(--accent-glow)' : 'transparent',
                      border: isActive ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
                      transition: 'all 0.1s',
                      fontSize: '12px',
                    }}
                    onMouseOver={(e) => { if (!isActive) e.currentTarget.style.background = 'var(--bg3)'; }}
                    onMouseOut={(e) => { if (!isActive) e.currentTarget.style.background = 'transparent'; }}
                  >
                    <StatusDot status={status} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        fontWeight: isActive ? '500' : '400',
                        color: 'var(--text)',
                        whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                      }}>
                        {project.label || project.id}
                      </div>
                      {project.workdir && (
                        <div style={{
                          fontSize: '9px', color: 'var(--text3)',
                          whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                        }}>
                          {shortWorkdir(project.workdir)}
                        </div>
                      )}
                    </div>
                    {/* Session toggle: only show for connected projects */}
                    {status === 'connected' && sessions.length > 0 && (
                      <span
                        onClick={(e) => {
                          e.stopPropagation();
                          setSessionsExpanded(prev => ({ ...prev, [project.id]: !sessExpanded }));
                        }}
                        title={sessExpanded ? '收起会话' : '展开会话'}
                        style={{
                          fontSize: '10px', color: 'var(--text3)', padding: '0 4px',
                          transition: 'transform 0.2s',
                          transform: sessExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
                        }}
                      >▶</span>
                    )}
                  </div>

                  {/* Session list under project */}
                  {status === 'connected' && sessExpanded && sessions.length > 0 && (
                    <div style={{
                      marginLeft: '18px', padding: '2px 0 4px',
                      borderLeft: '1px solid var(--border)',
                    }}>
                      {sessions.map(sess => {
                        const name = sess.session_name || sess.id || '(未命名)';
                        const isActiveSess = activeSessionName === name;
                        return (
                          <div
                            key={sess.id}
                            onClick={(e) => {
                              e.stopPropagation();
                              // Ensure project tab is active, then switch session
                              if (activeProjectId !== project.id) {
                                setActiveProject(project.id);
                              }
                              onSwitchLocalSession?.(name);
                            }}
                            title={`${name} — ${sess.message_count ?? 0} 条消息`}
                            style={{
                              display: 'flex', alignItems: 'center', gap: '4px',
                              padding: '3px 8px', borderRadius: '4px',
                              cursor: 'pointer',
                              fontSize: '11px',
                              color: isActiveSess ? 'var(--accent)' : 'var(--text2)',
                              background: isActiveSess ? 'rgba(99,102,241,0.08)' : 'transparent',
                              fontWeight: isActiveSess ? '500' : '400',
                            }}
                            onMouseOver={(e) => { if (!isActiveSess) e.currentTarget.style.background = 'var(--bg3)'; }}
                            onMouseOut={(e) => { if (!isActiveSess) e.currentTarget.style.background = 'transparent'; }}
                          >
                            <span style={{ fontSize: '11px' }}>📄</span>
                            <span style={{
                              flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                            }}>{name}</span>
                            {sess.message_count !== undefined && (
                              <span style={{ fontSize: '9px', color: 'var(--text3)' }}>{sess.message_count}</span>
                            )}
                          </div>
                        );
                      })}
                      {/* New session button */}
                      {onNewLocalSession && (
                        <div
                          onClick={(e) => {
                            e.stopPropagation();
                            const name = window.prompt('输入新会话名称:');
                            if (name?.trim()) {
                              if (activeProjectId !== project.id) {
                                setActiveProject(project.id);
                              }
                              onNewLocalSession(name.trim());
                            }
                          }}
                          title="新建会话"
                          style={{
                            display: 'flex', alignItems: 'center', gap: '4px',
                            padding: '3px 8px', borderRadius: '4px',
                            cursor: 'pointer', fontSize: '11px',
                            color: 'var(--text3)',
                          }}
                          onMouseOver={(e) => { e.currentTarget.style.background = 'var(--bg3)'; e.currentTarget.style.color = 'var(--accent)'; }}
                          onMouseOut={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = 'var(--text3)'; }}
                        >
                          <span>＋</span>
                          <span>新会话</span>
                        </div>
                      )}
                    </div>
                  )}
                </React.Fragment>
              );
            })
          )}

          {/* Add project button at bottom */}
          {projectList.length > 0 && (
            <button
              onClick={onOpenConnect}
              style={{
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                gap: '4px', padding: '5px 8px', borderRadius: '6px',
                fontSize: '11px', color: 'var(--text3)', background: 'transparent',
                border: '1px dashed var(--border)', cursor: 'pointer',
                marginTop: '2px',
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.background = 'var(--bg3)';
                e.currentTarget.style.color = 'var(--text)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = 'transparent';
                e.currentTarget.style.color = 'var(--text3)';
              }}
            >
              + 添加项目
            </button>
          )}
        </div>
      )}

      {/* Context menu */}
      {contextMenu.visible && (
        <div
          ref={menuRef}
          style={{
            position: 'fixed',
            left: contextMenu.x,
            top: contextMenu.y,
            background: 'var(--bg2)',
            border: '1px solid var(--border)',
            borderRadius: '8px',
            padding: '4px',
            zIndex: 1000,
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
            minWidth: '140px',
          }}
        >
          {projectSlots[contextMenu.projectId] && (
            <>
              <ContextMenuItem
                label="编辑项目"
                onClick={() => {
                  onEditProject?.(contextMenu.projectId);
                  setContextMenu(prev => ({ ...prev, visible: false }));
                }}
              />
              <ContextMenuItem
                label="断开连接"
                onClick={() => {
                  closeProject(contextMenu.projectId);
                  setContextMenu(prev => ({ ...prev, visible: false }));
                }}
              />
            </>
          )}
          {!projectSlots[contextMenu.projectId] && (
            <ContextMenuItem
              label="编辑项目"
              onClick={() => {
                onEditProject?.(contextMenu.projectId);
                setContextMenu(prev => ({ ...prev, visible: false }));
              }}
            />
          )}
          <ContextMenuItem
            label="删除项目"
            danger
            onClick={() => {
              // Close first if connected
              if (projectSlots[contextMenu.projectId]) {
                closeProject(contextMenu.projectId);
              }
              deleteProject(contextMenu.projectId);
              setContextMenu(prev => ({ ...prev, visible: false }));
            }}
          />
        </div>
      )}
    </div>
  );
};

/** Single context menu item */
const ContextMenuItem: React.FC<{
  label: string;
  danger?: boolean;
  onClick: () => void;
}> = ({ label, danger, onClick }) => (
  <div
    onClick={onClick}
    style={{
      padding: '6px 10px',
      fontSize: '12px',
      color: danger ? 'var(--red)' : 'var(--text)',
      cursor: 'pointer',
      borderRadius: '4px',
      transition: 'background 0.1s',
    }}
    onMouseOver={(e) => { e.currentTarget.style.background = 'var(--bg3)'; }}
    onMouseOut={(e) => { e.currentTarget.style.background = 'transparent'; }}
  >
    {label}
  </div>
);
