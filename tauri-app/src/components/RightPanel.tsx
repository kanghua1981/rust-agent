import React, { useState, useEffect } from 'react';
import { DirectoryTree } from './DirectoryTree';
import { ChangesList } from './ChangesList';
import { TaskPanel } from './TaskPanel';
import { useTaskStore } from '../stores/taskStore';
import { useAgentStore } from '../stores/agentStore';
import { useResizable } from '../hooks/useResizable';

type RightTab = 'browse' | 'changes' | 'tasks';

interface Props {
  activeTab: RightTab;
  onTabChange: (tab: RightTab) => void;
  onListDir: (path: string) => void;
  onOpenFile: (path: string) => void;
  onSandboxListChanges: () => void;
  onCommit: () => void;
  onCommitFile: (filePath: string) => void;
  onRollback: () => void;
}

export const RightPanel: React.FC<Props> = ({
  activeTab,
  onTabChange,
  onListDir,
  onOpenFile,
  onSandboxListChanges,
  onCommit,
  onCommitFile,
  onRollback,
}) => {
  const tasks = useTaskStore(s => s.tasks);
  const removeTask = useTaskStore(s => s.removeTask);
  const pendingChanges = useAgentStore(s => s.pendingChanges);
  const connectionStatus = useAgentStore(s => s.connectionStatus);

  const { width, onMouseDown } = useResizable({
    initialWidth: 360,
    minWidth: 200,
    maxWidth: 600,
    side: 'right',
  });

  // Collapse state
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('rightpanel-collapsed') === 'true'; } catch { return false; }
  });

  const toggleCollapsed = () => {
    setCollapsed(prev => {
      const next = !prev;
      try { localStorage.setItem('rightpanel-collapsed', String(next)); } catch {}
      return next;
    });
  };

  const running = tasks.filter(t => t.status === 'running' || t.status === 'connecting');
  const done = tasks.filter(t => t.status === 'done' || t.status === 'error');

  // Auto-fetch sandbox changes when connected in sandbox mode
  const config = useAgentStore(s => s.config);
  const isProcessing = useAgentStore(s => s.isProcessing);
  const sandboxBackend = useAgentStore(s => s.sandboxBackend);

  useEffect(() => {
    if (connectionStatus === 'connected' && config.isolation === 'sandbox' && sandboxBackend !== 'disabled') {
      onSandboxListChanges();
    }
  }, [connectionStatus, config.isolation, sandboxBackend]);

  useEffect(() => {
    if (!isProcessing && connectionStatus === 'connected' && config.isolation === 'sandbox' && sandboxBackend !== 'disabled') {
      onSandboxListChanges();
    }
  }, [isProcessing]);

  // ── Collapsed state ────────────────────────────────────────────────
  if (collapsed) {
    return (
      <div
        onClick={toggleCollapsed}
        title="展开右侧面板"
        style={{
          width: '36px',
          flexShrink: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'flex-start',
          borderLeft: '1px solid var(--border)',
          background: 'var(--bg)',
          cursor: 'pointer',
          padding: '12px 0',
          gap: '10px',
          transition: 'background 0.15s',
          userSelect: 'none',
        }}
        onMouseOver={(e) => e.currentTarget.style.background = 'var(--bg2)'}
        onMouseOut={(e) => e.currentTarget.style.background = 'var(--bg)'}
      >
        <span style={{
          writingMode: 'vertical-rl',
          letterSpacing: '0.15em',
          fontSize: '11px',
          fontWeight: '600',
          color: 'var(--text2)',
        }}>
          面板
        </span>

        {running.length > 0 && (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '4px' }}>
            <span style={{
              width: '7px', height: '7px',
              borderRadius: '50%',
              background: '#10b981',
              boxShadow: '0 0 6px rgba(16,185,129,0.6)',
            }} />
            <span style={{ fontSize: '12px', fontWeight: '700', color: '#10b981' }}>
              {running.length}
            </span>
          </div>
        )}

        {pendingChanges > 0 && (
          <span style={{
            fontSize: '11px', fontWeight: '700',
            color: '#f59e0b',
          }}>
            {pendingChanges}
          </span>
        )}

        <span style={{ fontSize: '10px', color: 'var(--text3)', marginTop: 'auto' }}>
          ▶
        </span>
      </div>
    );
  }

  // ── Expanded state ─────────────────────────────────────────────────
  return (
    <div style={{ display: 'flex', flexShrink: 0 }}>
      {/* Resize handle */}
      <div
        onMouseDown={onMouseDown}
        style={{
          width: '4px',
          cursor: 'col-resize',
          background: 'transparent',
          flexShrink: 0,
          transition: 'background 0.15s',
        }}
        onMouseOver={(e) => e.currentTarget.style.background = 'var(--accent)'}
        onMouseOut={(e) => e.currentTarget.style.background = 'transparent'}
      />

      <div style={{
        width: `${width}px`,
        display: 'flex',
        flexDirection: 'column',
        borderLeft: '1px solid var(--border)',
        background: 'var(--bg)',
        overflow: 'hidden',
        transition: 'width 0.2s ease',
      }}>
        {/* Tab bar */}
        <div style={{
          display: 'flex',
          borderBottom: '1px solid var(--border)',
          background: 'var(--bg2)',
          flexShrink: 0,
        }}>
          {([
            { id: 'browse' as RightTab, icon: '📂', label: '浏览' },
            { id: 'changes' as RightTab, icon: '📝', label: '变更', badge: pendingChanges || undefined },
            { id: 'tasks' as RightTab, icon: '📋', label: '任务', badge: running.length || (done.length || undefined) },
          ]).map(tab => (
            <button
              key={tab.id}
              onClick={() => onTabChange(tab.id)}
              style={{
                flex: 1,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '4px',
                padding: '8px 10px',
                border: 'none',
                borderBottom: activeTab === tab.id ? '2px solid var(--accent)' : '2px solid transparent',
                background: activeTab === tab.id ? 'var(--bg)' : 'transparent',
                color: activeTab === tab.id ? 'var(--accent)' : 'var(--text2)',
                fontSize: '12px',
                fontWeight: activeTab === tab.id ? '600' : '400',
                cursor: 'pointer',
                transition: 'all 0.15s',
                position: 'relative',
              }}
            >
              <span style={{ fontSize: '13px' }}>{tab.icon}</span>
              <span>{tab.label}</span>
              {tab.badge && tab.badge > 0 && (
                <span style={{
                  background: 'var(--red)',
                  color: '#fff',
                  borderRadius: '10px',
                  padding: '1px 5px',
                  fontSize: '10px',
                  fontWeight: '600',
                  minWidth: '16px',
                  textAlign: 'center',
                }}>
                  {tab.badge}
                </span>
              )}
            </button>
          ))}

          {/* Collapse button */}
          <button
            onClick={toggleCollapsed}
            title="折叠右侧面板"
            style={{
              padding: '8px 10px',
              border: 'none',
              background: 'transparent',
              color: 'var(--text3)',
              fontSize: '12px',
              cursor: 'pointer',
              flexShrink: 0,
            }}
            onMouseOver={(e) => e.currentTarget.style.color = 'var(--text)'}
            onMouseOut={(e) => e.currentTarget.style.color = 'var(--text3)'}
          >
            ◀
          </button>
        </div>

        {/* Tab content */}
        <div style={{ flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
          {activeTab === 'browse' && (
            <DirectoryTree
              collapsed={false}
              onListDir={onListDir}
              onOpenFile={onOpenFile}
            />
          )}

          {activeTab === 'changes' && (
            connectionStatus !== 'connected' ? (
              <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text3)', padding: '40px' }}>
                <p style={{ textAlign: 'center', fontSize: '14px' }}>未连接到服务器</p>
              </div>
            ) : sandboxBackend === 'disabled' || config.isolation !== 'sandbox' ? (
              <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', color: 'var(--text3)', padding: '40px', gap: '12px' }}>
                <span style={{ fontSize: '40px' }}>🚧</span>
                <p style={{ textAlign: 'center', fontSize: '14px' }}>沙盒未启用</p>
                <p style={{ textAlign: 'center', fontSize: '12px', color: 'var(--text3)' }}>在设置中开启沙盒后，此面板显示所有文件变更</p>
              </div>
            ) : (
              <ChangesList
                onSandboxListChanges={onSandboxListChanges}
                onCommit={onCommit}
                onCommitFile={onCommitFile}
                onRollback={onRollback}
              />
            )
          )}

          {activeTab === 'tasks' && (
            <div style={{
              flex: 1,
              overflowY: 'auto',
              padding: '10px',
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
            }}>
              {tasks.length === 0 ? (
                <div style={{ textAlign: 'center', color: 'var(--text3)', paddingTop: '40px', fontSize: '13px' }}>
                  暂无后台任务
                </div>
              ) : (
                <>
                  {running.map(t => (
                    <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />
                  ))}

                  {running.length > 0 && done.length > 0 && (
                    <div style={{
                      display: 'flex', alignItems: 'center', gap: '8px', padding: '4px 0',
                    }}>
                      <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
                      <span style={{ fontSize: '10px', color: 'var(--text3)' }}>已完成</span>
                      <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
                    </div>
                  )}

                  {done.map(t => (
                    <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />
                  ))}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
