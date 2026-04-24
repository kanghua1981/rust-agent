import React, { useState, useEffect } from 'react';
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
import { ConnectModal } from './components/ConnectModal';
import { useWebSocket } from './hooks/useWebSocket';
import { useAgentStore } from './stores/agentStore';
import { useAgentPool } from './hooks/useAgentPool';

type Tab = 'chat' | 'tools' | 'settings' | 'sessions' | 'sandbox' | 'nodes';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [showConnect, setShowConnect] = useState(false);

  const { connect, disconnect, sendUserMessage, sendCancel, confirmToolCall, answerQuestion, reviewPlan, loadSession, newSession, sandboxListChanges, sandboxCommit, sandboxCommitFile, sandboxRollback } = useWebSocket();
  const { reset, config } = useAgentStore();
  const { dispatchTask } = useAgentPool();

  const handleConnect = () => {
    connect();
  };

  const handleDisconnect = () => {
    disconnect();
    reset();
  };

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
                useAgentStore.getState().clearSession();
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
      <Header
        onOpenConnect={() => setShowConnect(true)}
        onDisconnect={handleDisconnect}
        onNewSession={newSession}
      />

      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          onOpenConnect={() => setShowConnect(true)}
          onQuickConnect={handleConnect}  // 添加快速连接支持
          onLoadSession={loadSession}
          onNewSession={newSession}
        />

        <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <main style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', background: 'var(--bg)' }}>
          {activeTab === 'chat' && (
            <>
              <ChatArea
                onConfirm={confirmToolCall}
                onAnswer={(id, answer) => { answerQuestion(answer); useAgentStore.getState().removePendingConfirmation(id); }}
                onReviewPlan={(id, approved, feedback) => { reviewPlan(approved, feedback); useAgentStore.getState().removePendingConfirmation(id); }}
              />
              <InputArea onSend={sendUserMessage} onCancel={sendCancel} onDispatch={dispatchTask} />
            </>
          )}
          {activeTab === 'tools' && <ToolsPanel />}
          {activeTab === 'nodes' && <NodesPanel />}
          {activeTab === 'sessions' && <SessionsPanel onSwitchToChat={() => setActiveTab('chat')} />}
          {activeTab === 'settings' && <SettingsPanel />}
          {activeTab === 'sandbox' && (
            <SandboxPanel
              onSandboxListChanges={sandboxListChanges}
              onCommit={sandboxCommit}
              onCommitFile={sandboxCommitFile}
              onRollback={sandboxRollback}
            />
          )}
        </main>
        <TaskPanelList />
        </div>
      </div>

      {showConnect && (
        <ConnectModal
          onConnect={handleConnect}
          onClose={() => setShowConnect(false)}
        />
      )}
    </div>
  );
}
export default App;
