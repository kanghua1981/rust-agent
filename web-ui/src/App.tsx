import React, { useState, useEffect, useCallback } from 'react';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import { InputArea } from './components/InputArea';
import { SettingsPanel } from './components/SettingsPanel';
import { NodesPanel } from './components/NodesPanel';
import { PluginsPanel } from './components/PluginsPanel';
import { ProjectTabs } from './components/ProjectTabs';
import { ProjectDialog } from './components/ProjectDialog';
import { ErrorBoundary } from './components/ErrorBoundary';
import { RightPanel } from './components/RightPanel';
import { useWebSocket } from './hooks/useWebSocket';
import { useAgentStore } from './stores/agentStore';
import { useAgentPool } from './hooks/useAgentPool';

import { ModelsPanel } from './components/ModelsPanel';
import { SettingsShell, SettingsSection } from './components/SettingsShell';
import { CommandPalette, CommandAction } from './components/CommandPalette';

type Tab = 'chat' | 'settings';
type RightTab = 'browse' | 'changes' | 'tasks' | 'terminal';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('general');
  const [rightTab, setRightTab] = useState<RightTab>('browse');
  const [showConnect, setShowConnect] = useState(false);
  const [editProjectId, setEditProjectId] = useState<string | null>(null);
  const [showCommandPalette, setShowCommandPalette] = useState(false);

  const { connect, disconnect, switchToConnection, sendUserMessage, sendCancel, confirmToolCall, answerQuestion, reviewPlan, newSession, sandboxListChanges, sandboxCommit, sandboxCommitFile, sandboxRollback, uploadFile, listPlugins, enablePlugin, disablePlugin, listSessions, deleteSession, loadSessionById, loadSession, setWorkdirRemote, setModelRemote, fetchModels, addModel, deleteModel, listEndpoints, addEndpoint, deleteEndpoint, listNodes, addNode, updateNode, deleteNode, listPeers, addPeer, updatePeer, deletePeer, listLocalSessions, switchLocalSession, newLocalSession, deleteLocalSession, renameLocalSession, listDir, openFileExternal, ptyOpen, ptyInput, ptyResize, ptyClose, registerPtyOutput } = useWebSocket();
  const { reset, config, connectionStatus } = useAgentStore();
  const { dispatchTask } = useAgentPool();

  // Per-tab rendering: only the active ChatArea is mounted
  const activeProjectId = useAgentStore(s => s.activeProjectId);

  const handleOpenConnect = useCallback((editId?: string) => {
    setEditProjectId(editId || null);
    setShowConnect(true);
  }, []);

  const handleConnect = useCallback(() => {
    const st = useAgentStore.getState();
    // flat proxy fields are set by ProjectDialog (or SettingsPanel) before connecting

    // Read the target URL/workdir from the flat proxy
    const targetUrl = st.serverUrl;
    const targetWorkdir = st.workdir;

    // A saved workspace with the same server + workdir owns this connection, so
    // its sidebar row shows the status and its session list. An ad-hoc target
    // (a URL typed straight into the dialog) still gets a throwaway slot.
    const project = Object.values(st.projects ?? {}).find(
      (proj) => proj.serverUrl === targetUrl && (proj.workdir ?? '') === (targetWorkdir ?? ''),
    );

    const slotId = project?.id ?? `conn_${Date.now()}`;
    if (project) {
      st.openProject(project.id); // creates the slot when missing, then activates it
    } else {
      const hostLabel = (() => {
        if (targetWorkdir) {
          return targetWorkdir.split('/').filter(Boolean).pop() || targetWorkdir;
        }
        try {
          const u = new URL(targetUrl.replace(/^ws(s?):/, 'http$1:'));
          return u.host + (u.pathname && u.pathname !== '/' ? u.pathname : '');
        } catch {
          return targetUrl.replace(/^wss?:\/\//, '');
        }
      })();
      st.createConnectionSlot(slotId, hostLabel, targetUrl, targetWorkdir);
      st.setActiveConnection(slotId);
    }

    // `st` is the pre-update snapshot, so a slot missing here was just created.
    if (st.projectSlots[slotId]?.connectionStatus !== 'connected') connect(slotId);
  }, [connect]);

  const handleDisconnect = useCallback(() => {
    disconnect();
    reset();
  }, [disconnect, reset]);

  // File upload handler: read file as base64 and send via WebSocket
  const handleUpload = useCallback((file: File) => {
    // Size check on client side (50 MB limit, same as server)
    if (file.size > 50 * 1024 * 1024) {
      alert(`文件 ${file.name} 太大 (${(file.size / 1024 / 1024).toFixed(1)} MB)，最大 50 MB`);
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const base64 = (reader.result as string).split(',')[1]; // strip data:... prefix
      if (base64) {
        uploadFile(file.name, base64, file.type || undefined);
      }
    };
    reader.onerror = () => {
      console.error('File read error:', reader.error);
    };
    reader.readAsDataURL(file);
  }, [uploadFile]);

  // 键盘快捷键处理

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + K 打开连接模态框
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        handleOpenConnect();
      }
      // Ctrl+Shift+P: 命令面板
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'p' && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setShowCommandPalette(true);
        return;
      }
      // Esc 关闭连接模态框 / 命令面板
      if (e.key === 'Escape') {
        if (showConnect) {
          e.preventDefault();
          setShowConnect(false);
        } else if (showCommandPalette) {
          e.preventDefault();
          setShowCommandPalette(false);
        }
      }
      // 新增快捷键 - 只在聊天页面生效
      if (activeTab === 'chat') {
        if (e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey) {
          switch (e.key.toLowerCase()) {
            case 'n': // Ctrl+Shift+N: 新建会话
              e.preventDefault();
              newSession();
              break;
              
            case 'm': // Ctrl+Shift+M: 切换运行模式
              e.preventDefault();
              const store = useAgentStore.getState();
              const modes = ['auto', 'simple', 'plan'] as const;
              const currentMode = store.config.agentMode || 'auto';
              const currentIndex = modes.indexOf(currentMode);
              const nextIndex = (currentIndex + 1) % modes.length;
              store.setConfig({ agentMode: modes[nextIndex] });
              
              // 显示短暂提示
              const modeNames = { auto: '自动', simple: '单层', plan: '计划' };
              console.log(`运行模式已切换为: ${modeNames[modes[nextIndex]]}`);
              break;
          }
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [showConnect, showCommandPalette, activeTab, config?.agentMode, newSession]);

  // ── 命令面板 extraActions（依赖 App 级函数）──────────────────────
  const commandActions = ((): CommandAction[] => {
    const store = useAgentStore.getState();
    const connected = store.connectionStatus === 'connected';
    return [
      // 连接
      {
        id: 'connect.open',
        label: '连接服务器',
        description: '打开连接配置面板',
        category: '连接',
        keywords: 'connect',
        action: () => handleOpenConnect(),
      },
      {
        id: 'connect.disconnect',
        label: '断开连接',
        description: '断开当前 WebSocket 连接',
        category: '连接',
        keywords: 'disconnect',
        enabled: connected,
        action: () => handleDisconnect(),
      },
      // 面板导航
      {
        id: 'nav.chat',
        label: '打开对话',
        description: '切换到对话面板',
        category: '面板',
        keywords: 'goto chat',
        action: () => setActiveTab('chat'),
      },
      {
        id: 'nav.settings',
        label: '打开设置',
        description: '切换到设置面板',
        category: '面板',
        keywords: 'goto settings',
        action: () => setActiveTab('settings'),
      },
      ...([
        ['nodes', '🌐', '节点管理'],
        ['models', '🧠', '模型管理'],
        ['plugins', '🧩', '插件管理'],
      ] as const).map(([sec, icon, label]) => ({
        id: `settings.${sec}`,
        label: `打开${label}`,
        description: `设置 · ${icon} ${label}`,
        category: '设置',
        keywords: `goto ${sec}`,
        action: () => { setSettingsSection(sec); setActiveTab('settings'); },
      })),
      // 会话操作
      {
        id: 'session.new',
        label: '新建会话',
        description: '开始一个新的对话会话',
        category: '会话',
        keywords: 'new session',
        enabled: connected,
        action: () => newSession(),
      },
      // 模型切换（动态）
      ...store.availableModels.map((m) => ({
        id: `model.${m.alias}`,
        label: `切换模型: ${m.alias}`,
        description: `${m.model} (${m.provider})`,
        category: '模型',
        keywords: `model switch ${m.alias}`,
        enabled: connected,
        action: () => setModelRemote(m.alias),
      })),
      // 运行模式切换
      ...(['auto', 'simple', 'plan'] as const).map((m) => ({
        id: `mode.${m}`,
        label: `切换为 ${{ auto: '自动', simple: '单层', plan: '计划' }[m]} 模式`,
        description: {
          auto: '自动选择执行策略',
          simple: '单层 Agent 循环，速度快',
          plan: '先规划再执行',
        }[m],
        category: '运行模式',
        keywords: `mode ${m}`,
        enabled: connected,
        action: () => useAgentStore.getState().setConfig({ agentMode: m }),
      })),
      // 自动批准
      {
        id: 'op.autoApprove',
        label: '切换自动批准',
        description: '切换工具调用自动批准开关',
        category: '操作',
        keywords: 'yesall confirm auto-approve',
        enabled: connected,
        action: () => {
          const s = useAgentStore.getState();
          s.setConfig({ autoApprove: !(s.config.autoApprove) });
        },
      },
      // 取消任务
      {
        id: 'op.cancel',
        label: '取消当前任务',
        description: '中断正在执行的 Agent 任务',
        category: '操作',
        keywords: 'cancel stop abort',
        enabled: connected && store.isProcessing,
        action: () => sendCancel(),
      },
    ];
  })();

  return (
    <div className="app-shell">
      <ErrorBoundary>
        <Header
          activeProjectId={activeProjectId}
          onOpenConnect={() => handleOpenConnect()}
          onDisconnect={handleDisconnect}
        />
      </ErrorBoundary>

      <ErrorBoundary>
        <ProjectTabs onNewProject={() => handleOpenConnect()} disconnectProject={disconnect} connectProject={connect} />
      </ErrorBoundary>

      <div className="app-row">
        <ErrorBoundary>
          <Sidebar
            activeTab={activeTab}
            onTabChange={setActiveTab}
            onOpenConnect={() => handleOpenConnect()}
            onEditProject={(id) => handleOpenConnect(id)}
            onConnectProject={(id) => connect(id)}
            onSwitchToChat={() => setActiveTab('chat')}
            onSwitchLocalSession={switchLocalSession}
            onNewLocalSession={newLocalSession}
            onDeleteLocalSession={deleteLocalSession}
            onRenameLocalSession={renameLocalSession}
          />
        </ErrorBoundary>

        <div className="app-row">
        <main className="app-main">
          <ErrorBoundary key={activeTab}>
          {activeTab === 'chat' && (
            <>
              {/* Only the active ChatArea is mounted — inactive slots update via _updateSlot in the background. */}
              <div className="app-fill">
                <ChatArea
                  slotId={activeProjectId ?? 'default'}
                  onConfirm={confirmToolCall}
                  onAnswer={(id, answer) => { answerQuestion(answer); useAgentStore.getState().removePendingConfirmation(id); }}
                  onReviewPlan={(id, approved, feedback) => { reviewPlan(approved, feedback); useAgentStore.getState().removePendingConfirmation(id); }}
                  onRestoreSession={() => loadSession()}
                  onDismissRestore={() => useAgentStore.getState().setSessionRestoreAvailable(null)}
                />
              </div>
              <InputArea
                onSend={sendUserMessage}
                onCancel={sendCancel}
                onDispatch={dispatchTask}
                onUpload={handleUpload}
                onSetModelRemote={setModelRemote}
                onNewSession={newSession}
              />
            </>
          )}
          {activeTab === 'settings' && (
            <SettingsShell section={settingsSection} onSectionChange={setSettingsSection}>
              {settingsSection === 'general' && (
                <SettingsPanel isConnected={connectionStatus === 'connected'} onSetWorkdirRemote={setWorkdirRemote} />
              )}
              {settingsSection === 'nodes' && (
                <NodesPanel isConnected={connectionStatus === 'connected'} onListNodes={listNodes} onAddNode={addNode} onUpdateNode={updateNode} onDeleteNode={deleteNode} onListPeers={listPeers} onAddPeer={addPeer} onUpdatePeer={updatePeer} onDeletePeer={deletePeer} />
              )}
              {settingsSection === 'models' && (
                <ModelsPanel onSetModelRemote={setModelRemote} onFetchModels={fetchModels} onAddModel={addModel} onDeleteModel={deleteModel} onListEndpoints={listEndpoints} onAddEndpoint={addEndpoint} onDeleteEndpoint={deleteEndpoint} />
              )}
              {settingsSection === 'plugins' && (
                <PluginsPanel onEnablePlugin={enablePlugin} onDisablePlugin={disablePlugin} />
              )}
            </SettingsShell>
          )}
          </ErrorBoundary>
        </main>
        <ErrorBoundary>
          <RightPanel
            activeTab={rightTab}
            onTabChange={setRightTab}
            onListDir={listDir}
            onOpenFile={openFileExternal}
            onSandboxListChanges={sandboxListChanges}
            onCommit={sandboxCommit}
            onCommitFile={sandboxCommitFile}
            onRollback={sandboxRollback}
            onPtyOpen={ptyOpen}
            onPtyInput={ptyInput}
            onPtyResize={ptyResize}
            onPtyClose={ptyClose}
            registerPtyOutput={registerPtyOutput}
          />
        </ErrorBoundary>
        </div>
      </div>

      {showConnect && (
        <ErrorBoundary>
          <ProjectDialog
            onConnect={handleConnect}
            onClose={() => { setShowConnect(false); setEditProjectId(null); }}
            editProjectId={editProjectId}
          />
        </ErrorBoundary>
      )}

      <CommandPalette
        open={showCommandPalette}
        onClose={() => setShowCommandPalette(false)}
        extraActions={commandActions}
      />
    </div>
  );
}
export default App;
