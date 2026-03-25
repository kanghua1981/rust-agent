import React, { useState } from 'react';
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
  const { reset } = useAgentStore();
  const { dispatchTask } = useAgentPool();

  const handleConnect = () => {
    connect();
  };

  const handleDisconnect = () => {
    disconnect();
    reset();
  };

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
      />

      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          onOpenConnect={() => setShowConnect(true)}
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
