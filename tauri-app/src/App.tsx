import React, { useState, useEffect, useCallback } from 'react';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import { InputArea } from './components/InputArea';
import { ToolsPanel } from './components/ToolsPanel';
import { SettingsPanel } from './components/SettingsPanel';
import { SessionsPanel } from './components/SessionsPanel';
import { SandboxPanel } from './components/SandboxPanel';
import { NodesPanel } from './components/NodesPanel';
import { TaskPanelList } from './components/TaskPanelList';
import { PluginsPanel } from './components/PluginsPanel';
import { ConnectionTabs } from './components/ConnectionTabs';
import { ConnectModal } from './components/ConnectModal';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useWebSocket } from './hooks/useWebSocket';
import { useAgentStore } from './stores/agentStore';
import { useAgentPool } from './hooks/useAgentPool';

type Tab = 'chat' | 'tools' | 'settings' | 'sessions' | 'sandbox' | 'nodes' | 'plugins';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [showConnect, setShowConnect] = useState(false);

  const { connect, disconnect, switchToConnection, sendUserMessage, sendCancel, confirmToolCall, answerQuestion, reviewPlan, newSession, sandboxListChanges, sandboxCommit, sandboxCommitFile, sandboxRollback, uploadFile, listPlugins, enablePlugin, disablePlugin, listSessions, deleteSession, loadSessionById, loadSession, setWorkdirRemote } = useWebSocket();
  const { reset, config, connectionStatus } = useAgentStore();
  const { dispatchTask } = useAgentPool();

  const handleConnect = useCallback(() => {
    const st = useAgentStore.getState();
    // Save current active slot state (flat proxy → connections map)
    st._saveActiveSlot();

    // Read the target URL/workdir from the flat proxy (set by ConnectModal/applyPreset)
    const targetUrl = st.serverUrl;
    const targetWorkdir = st.workdir;

    // Create a new connection slot — every connection gets its own isolated slot
    const slotId = `conn_${Date.now()}`;
    // Label: last workdir component if set, otherwise host:port
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
        setShowConnect(true);
      }
      // Esc 关闭连接模态框
      if (e.key === 'Escape' && showConnect) {
        e.preventDefault();
        setShowConnect(false);
      }
      // Cmd/Ctrl + Enter 快速发送消息（在聊天页面时）
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && activeTab === 'chat') {
        e.preventDefault();
        // 这里需要获取当前输入框的内容并发送
        // 由于InputArea组件内部处理发送逻辑，我们暂时不在这里实现
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
  }, [showConnect, activeTab, config?.agentMode, newSession]);
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
          onOpenConnect={() => setShowConnect(true)}
          onDisconnect={handleDisconnect}
          onNewSession={newSession}
        />
      </ErrorBoundary>

      <ErrorBoundary>
        <ConnectionTabs onNewConnection={() => setShowConnect(true)} switchToConnection={switchToConnection} disconnectSlot={disconnect} connectSlot={connect} />
      </ErrorBoundary>

      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <ErrorBoundary>
          <Sidebar
            activeTab={activeTab}
            onTabChange={setActiveTab}
            onOpenConnect={() => setShowConnect(true)}
            onQuickConnect={handleConnect}  // 添加快速连接支持
            onNewSession={newSession}
          />
        </ErrorBoundary>

        <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <main style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', background: 'var(--bg)' }}>
          <ErrorBoundary key={activeTab}>
          {activeTab === 'chat' && (
            <>
              <ChatArea
                onConfirm={confirmToolCall}
                onAnswer={(id, answer) => { answerQuestion(answer); useAgentStore.getState().removePendingConfirmation(id); }}
                onReviewPlan={(id, approved, feedback) => { reviewPlan(approved, feedback); useAgentStore.getState().removePendingConfirmation(id); }}
                onRestoreSession={() => loadSession()}
                onDismissRestore={() => useAgentStore.getState().setSessionRestoreAvailable(null)}
              />
              <InputArea onSend={sendUserMessage} onCancel={sendCancel} onDispatch={dispatchTask} onUpload={handleUpload} />
            </>
          )}
          {activeTab === 'tools' && <ToolsPanel />}
          {activeTab === 'nodes' && <NodesPanel />}
          {activeTab === 'sessions' && <SessionsPanel onSwitchToChat={() => setActiveTab('chat')} isConnected={connectionStatus === 'connected'} onListSessions={listSessions} onDeleteSession={deleteSession} onLoadSessionById={loadSessionById} />}
          {activeTab === 'settings' && <SettingsPanel isConnected={connectionStatus === 'connected'} onSetWorkdirRemote={setWorkdirRemote} />}
          {activeTab === 'plugins' && <PluginsPanel onEnablePlugin={enablePlugin} onDisablePlugin={disablePlugin} />}
          {activeTab === 'sandbox' && (
            <SandboxPanel
              onSandboxListChanges={sandboxListChanges}
              onCommit={sandboxCommit}
              onCommitFile={sandboxCommitFile}
              onRollback={sandboxRollback}
            />
          )}
          </ErrorBoundary>
        </main>
        <ErrorBoundary>
          <TaskPanelList />
        </ErrorBoundary>
        </div>
      </div>

      {showConnect && (
        <ErrorBoundary>
          <ConnectModal
            onConnect={handleConnect}
            onClose={() => setShowConnect(false)}
          />
        </ErrorBoundary>
      )}
    </div>
  );
}
export default App;
