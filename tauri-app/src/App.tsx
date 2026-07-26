import React, { useState, useEffect, useCallback } from 'react';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import { InputArea } from './components/InputArea';
import { SettingsPanel } from './components/SettingsPanel';
import { NodesPanel } from './components/NodesPanel';
import { PluginsPanel } from './components/PluginsPanel';
import { WorkflowPanel } from './components/WorkflowPanel';
import { ProjectTabs } from './components/ProjectTabs';
import { ProjectDialog } from './components/ProjectDialog';
import { ErrorBoundary } from './components/ErrorBoundary';
import { RightPanel } from './components/RightPanel';
import { useWebSocket } from './hooks/useWebSocket';
import { useAgentStore } from './stores/agentStore';
import { useAgentPool } from './hooks/useAgentPool';

import { ModelsPanel } from './components/ModelsPanel';
import { CommandPalette, CommandAction } from './components/CommandPalette';
import { PipelinePanel } from './components/PipelinePanel';

type Tab = 'chat' | 'settings' | 'nodes' | 'plugins' | 'models' | 'workflows' | 'pipelines';
type RightTab = 'browse' | 'changes' | 'tasks';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [rightTab, setRightTab] = useState<RightTab>('browse');
  const [showConnect, setShowConnect] = useState(false);
  const [editProjectId, setEditProjectId] = useState<string | null>(null);
  const [showCommandPalette, setShowCommandPalette] = useState(false);

  const { connect, disconnect, switchToConnection, sendUserMessage, sendCancel, confirmToolCall, answerQuestion, reviewPlan, newSession, sandboxListChanges, sandboxCommit, sandboxCommitFile, sandboxRollback, uploadFile, listPlugins, enablePlugin, disablePlugin, listSessions, deleteSession, loadSessionById, loadSession, setWorkdirRemote, setModelRemote, fetchModels, addModel, deleteModel, listEndpoints, addEndpoint, deleteEndpoint, listNodes, addNode, updateNode, deleteNode, listPeers, addPeer, updatePeer, deletePeer, listWorkflows, saveWorkflow: saveWorkflowWs, deleteWorkflow: deleteWorkflowWs, runWorkflow, listLocalSessions, switchLocalSession, newLocalSession, deleteLocalSession, renameLocalSession, listPipelines, getPipeline, savePipeline, deletePipeline, listDir, openFileExternal } = useWebSocket();
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

    // Create a new slot for this connection
    const slotId = `conn_${Date.now()}`;
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

    // Switch to the new (empty) slot — this clears messages/toolCalls from the flat proxy
    st.setActiveConnection(slotId);

    // Connect with the new slot
    connect(slotId);
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
              
            case 'c': // Ctrl+Shift+C: 清空会话
              e.preventDefault();
              if (window.confirm('确定要清空当前会话吗？')) {
                newSession();
              }
              break;
              
            case 'm': // Ctrl+Shift+M: 切换运行模式
              e.preventDefault();
              const store = useAgentStore.getState();
              const modes = ['auto', 'simple', 'plan', 'pipeline'] as const;
              const currentMode = store.config.agentMode || 'auto';
              const currentIndex = modes.indexOf(currentMode);
              const nextIndex = (currentIndex + 1) % modes.length;
              store.setConfig({ agentMode: modes[nextIndex] });
              
              // 显示短暂提示
              const modeNames = { auto: '自动', simple: '单层', plan: '计划', pipeline: '流水线' };
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
      ...([
        ['chat', '对话', '💬'],
        ['nodes', '节点', '🌐'],
        ['plugins', '插件', '🧩'],
        ['models', '模型管理', '🧠'],
        ['settings', '设置', '⚙️'],
      ] as const).map(([tab, tabLabel]) => ({
        id: `nav.${tab}`,
        label: `打开${tabLabel}`,
        description: `切换到${tabLabel}面板`,
        category: '面板',
        keywords: `goto ${tab}`,
        action: () => setActiveTab(tab as Tab),
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
      {
        id: 'session.clear',
        label: '清空会话',
        description: '清除当前所有对话消息',
        category: '会话',
        keywords: 'clear reset',
        enabled: connected,
        action: () => {
          if (window.confirm('确定要清空当前会话吗？')) newSession();
        },
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
      ...(['auto', 'simple', 'plan', 'pipeline'] as const).map((m) => ({
        id: `mode.${m}`,
        label: `切换为 ${{ auto: '自动', simple: '单层', plan: '计划', pipeline: '流水线' }[m]} 模式`,
        description:
          m === 'auto' ? 'Router 自动选择执行策略'
          : m === 'simple' ? '单层 Agent 循环，速度快'
          : m === 'plan' ? '先规划再执行'
          : 'Planner → Executor → Checker 三阶段流水线',
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
    <div style={{
      height: '100vh',
      display: 'flex',
      flexDirection: 'column',
      background: 'var(--bg)',
      color: 'var(--text)',
      overflow: 'hidden',
    }}>
      <ErrorBoundary>
        <Header
          activeProjectId={activeProjectId}
          onOpenConnect={() => handleOpenConnect()}
          onDisconnect={handleDisconnect}
          onNewSession={newSession}
          onSetModelRemote={setModelRemote}
        />
      </ErrorBoundary>

      <ErrorBoundary>
        <ProjectTabs onNewProject={() => handleOpenConnect()} disconnectProject={disconnect} connectProject={connect} />
      </ErrorBoundary>

      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <ErrorBoundary>
          <Sidebar
            activeTab={activeTab}
            onTabChange={setActiveTab}
            onOpenConnect={() => handleOpenConnect()}
            onQuickConnect={handleConnect}
            onSwitchLocalSession={switchLocalSession}
            onNewLocalSession={newLocalSession}
            onConnectProject={(id: string) => connect(id)}
            onEditProject={(id) => handleOpenConnect(id)}
          />
        </ErrorBoundary>

        <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <main style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', background: 'var(--bg)' }}>
          <ErrorBoundary key={activeTab}>
          {activeTab === 'chat' && (
            <>
              {/* Only the active ChatArea is mounted — inactive slots update via _updateSlot in the background. */}
              <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                <ChatArea
                  slotId={activeProjectId ?? 'default'}
                  onConfirm={confirmToolCall}
                  onAnswer={(id, answer) => { answerQuestion(answer); useAgentStore.getState().removePendingConfirmation(id); }}
                  onReviewPlan={(id, approved, feedback) => { reviewPlan(approved, feedback); useAgentStore.getState().removePendingConfirmation(id); }}
                  onRestoreSession={() => loadSession()}
                  onDismissRestore={() => useAgentStore.getState().setSessionRestoreAvailable(null)}
                />
              </div>
              <InputArea onSend={sendUserMessage} onCancel={sendCancel} onDispatch={dispatchTask} onUpload={handleUpload} />
            </>
          )}
          {activeTab === 'nodes' && <NodesPanel isConnected={connectionStatus === 'connected'} onListNodes={listNodes} onAddNode={addNode} onUpdateNode={updateNode} onDeleteNode={deleteNode} onListPeers={listPeers} onAddPeer={addPeer} onUpdatePeer={updatePeer} onDeletePeer={deletePeer} />}
          {activeTab === 'settings' && <SettingsPanel isConnected={connectionStatus === 'connected'} onSetWorkdirRemote={setWorkdirRemote} />}
          {activeTab === 'plugins' && <PluginsPanel onEnablePlugin={enablePlugin} onDisablePlugin={disablePlugin} />}
          {activeTab === 'workflows' && <WorkflowPanel
            isConnected={connectionStatus === 'connected'}
            listWorkflowsWs={listWorkflows}
            saveWorkflowWs={saveWorkflowWs}
            deleteWorkflowWs={deleteWorkflowWs}
            runWorkflowWs={runWorkflow}
          />}
          {activeTab === 'pipelines' && <PipelinePanel
            isConnected={connectionStatus === 'connected'}
            listPipelinesWs={listPipelines}
            getPipelineWs={getPipeline}
            savePipelineWs={savePipeline}
            deletePipelineWs={deletePipeline}
          />}
          {activeTab === 'models' && <ModelsPanel onSetModelRemote={setModelRemote} onFetchModels={fetchModels} onAddModel={addModel} onDeleteModel={deleteModel} onListEndpoints={listEndpoints} onAddEndpoint={addEndpoint} onDeleteEndpoint={deleteEndpoint} />}
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
