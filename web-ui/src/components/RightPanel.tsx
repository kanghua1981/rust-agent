import React, { useState, useEffect } from 'react';
import { DirectoryTree } from './DirectoryTree';
import { ChangesList } from './ChangesList';
import { TaskPanel } from './TaskPanel';
import { TerminalView } from './Terminal';
import { FileViewer } from './FileViewer';
import { localOpenAvailability } from '../utils/fileTransfer';
import { useTaskStore } from '../stores/taskStore';
import { useAgentStore } from '../stores/agentStore';
import { useResizable } from '../hooks/useResizable';

type RightTab = 'browse' | 'changes' | 'tasks' | 'terminal' | 'file';

interface Props {
  activeTab: RightTab;
  onTabChange: (tab: RightTab) => void;
  onListDir: (path: string) => void;
  onOpenFile: (path: string) => void;
  onSandboxListChanges: () => void;
  onCommit: () => void;
  onCommitFile: (filePath: string) => void;
  onRollback: () => void;
  // ── File viewer ──
  onOpenLocally: (path: string) => void;
  onDownload: (path: string) => void;
  onCloseFile: () => void;
  // ── Terminal ──
  onPtyOpen: (workdir: string | undefined, rows: number, cols: number) => void;
  onPtyInput: (input: string) => void;
  onPtyResize: (rows: number, cols: number) => void;
  onPtyClose: () => void;
  registerPtyOutput: (cb: (data: string) => void) => void;
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
  onOpenLocally,
  onDownload,
  onCloseFile,
  onPtyOpen,
  onPtyInput,
  onPtyResize,
  onPtyClose,
  registerPtyOutput,
}) => {
  const tasks = useTaskStore(s => s.tasks);
  const removeTask = useTaskStore(s => s.removeTask);
  const pendingChanges = useAgentStore(s => s.pendingChanges);
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const config = useAgentStore(s => s.config);
  const isProcessing = useAgentStore(s => s.isProcessing);
  const sandboxBackend = useAgentStore(s => s.sandboxBackend);
  const serverUrl = useAgentStore(s => s.serverUrl);
  const openFile = useAgentStore(s => s.openFile);

  const { width, onMouseDown } = useResizable({
    initialWidth: 480,
    minWidth: 280,
    maxWidth: 900,
    side: 'right',
  });

  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('rightpanel-collapsed') === 'true'; } catch { return false; }
  });
  const toggleCollapsed = () => setCollapsed(prev => {
    const next = !prev;
    try { localStorage.setItem('rightpanel-collapsed', String(next)); } catch {}
    return next;
  });

  const running = tasks.filter(t => t.status === 'running' || t.status === 'connecting');
  const done = tasks.filter(t => t.status === 'done' || t.status === 'error');

  const sandboxActive =
    connectionStatus === 'connected' &&
    config.isolation === 'sandbox' &&
    sandboxBackend !== 'disabled';

  // Auto-fetch sandbox changes when connected in sandbox mode (and after each turn).
  useEffect(() => {
    if (sandboxActive) onSandboxListChanges();
  }, [connectionStatus, config.isolation, sandboxBackend]);

  useEffect(() => {
    if (!isProcessing && sandboxActive) onSandboxListChanges();
  }, [isProcessing]);

  // Opening a file is only useful if you can see it.
  const openFilePath = openFile?.path;
  useEffect(() => {
    if (!openFilePath) return;
    setCollapsed(prev => {
      if (!prev) return prev;
      try { localStorage.setItem('rightpanel-collapsed', 'false'); } catch {}
      return false;
    });
  }, [openFilePath]);

  // Contextual tabs: 变更 appears only in sandbox mode (or with pending changes),
  // 任务 only while tasks exist, 文件 only while a file is open, so an empty
  // panel never occupies tab space.
  const tabs: { id: RightTab; icon: string; label: string; badge?: number }[] = [
    { id: 'browse', icon: '📂', label: '浏览' },
    ...(openFile ? [{ id: 'file' as RightTab, icon: '📄', label: '文件' }] : []),
    { id: 'terminal', icon: '🖥', label: '终端' },
    ...(sandboxActive || pendingChanges > 0
      ? [{ id: 'changes' as RightTab, icon: '📝', label: '变更', badge: pendingChanges || undefined }]
      : []),
    ...(tasks.length > 0
      ? [{ id: 'tasks' as RightTab, icon: '📋', label: '任务', badge: running.length || done.length || undefined }]
      : []),
  ];
  const active: RightTab = tabs.some(t => t.id === activeTab) ? activeTab : 'browse';

  if (collapsed) {
    return (
      <div className="rail-collapsed" onClick={toggleCollapsed} title="展开右侧面板">
        <span className="rail-label">面板</span>
        {running.length > 0 && (
          <div className="col center gap-1">
            <span className="dot-live" />
            <span className="text-sm weight-600 text-ok">{running.length}</span>
          </div>
        )}
        {pendingChanges > 0 && (
          <span className="text-xs weight-600 text-warn">{pendingChanges}</span>
        )}
        <span className="text-xs text-3 mt-auto">▶</span>
      </div>
    );
  }

  return (
    <div className="right-shell">
      <div className="resize-handle" onMouseDown={onMouseDown} />

      <div className="right-panel" style={{ width: `${width}px` }}>
        <div className="tabs">
          {tabs.map(tab => (
            <button
              key={tab.id}
              className={`tab${active === tab.id ? ' active' : ''}`}
              onClick={() => onTabChange(tab.id)}
            >
              <span>{tab.icon}</span>
              <span>{tab.label}</span>
              {tab.badge ? <span className="badge">{tab.badge}</span> : null}
            </button>
          ))}
          <button
            className="icon-btn panel-collapse"
            onClick={toggleCollapsed}
            title="折叠右侧面板"
          >◀</button>
        </div>

        <div className="col fill clip">
          {active === 'browse' && (
            <DirectoryTree collapsed={false} onListDir={onListDir} onOpenFile={onOpenFile} />
          )}

          {active === 'file' && openFile && (
            <FileViewer
              file={openFile}
              localOpen={localOpenAvailability(serverUrl, config.isolation)}
              onOpenLocally={onOpenLocally}
              onDownload={onDownload}
              onClose={onCloseFile}
            />
          )}

          {active === 'terminal' && (
            <TerminalView
              onPtyOpen={onPtyOpen}
              onPtyInput={onPtyInput}
              onPtyResize={onPtyResize}
              onPtyClose={onPtyClose}
              registerPtyOutput={registerPtyOutput}
              isConnected={connectionStatus === 'connected'}
            />
          )}

          {active === 'changes' && (
            connectionStatus !== 'connected' ? (
              <div className="panel-empty"><p className="text-sm">未连接到服务器</p></div>
            ) : !sandboxActive ? (
              <div className="panel-empty">
                <span style={{ fontSize: 40 }}>🚧</span>
                <p className="text-sm">沙盒未启用</p>
                <p className="text-sm" style={{ color: 'var(--text3)' }}>在设置中开启沙盒后，此面板显示所有文件变更</p>
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

          {active === 'tasks' && (
            <div className="panel-scroll">
              {tasks.length === 0 ? (
                <div className="empty-line">暂无后台任务</div>
              ) : (
                <>
                  {running.map(t => <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />)}
                  {running.length > 0 && done.length > 0 && (
                    <div className="divider-label"><span>已完成</span></div>
                  )}
                  {done.map(t => <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />)}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
