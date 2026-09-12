import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { SessionList } from './SessionList';

type Tab = 'chat' | 'settings';

interface SidebarProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onOpenConnect: () => void;
  onDisconnect: () => void;
  onSwitchToChat: () => void;
  onListLocalSessions: () => void;
  onSwitchLocalSession: (name: string) => void;
  onNewLocalSession: (name: string) => void;
  onDeleteLocalSession: (name: string) => void;
  onRenameLocalSession: (oldName: string, newName: string) => void;
}

interface NavDef { tab: Tab; icon: string; label: string }

const NAV: NavDef[] = [
  { tab: 'chat',     icon: '💬', label: '对话' },
  { tab: 'settings', icon: '⚙️', label: '设置' },
];

const NavItem: React.FC<{
  item: NavDef;
  active: boolean;
  badge?: number;
  collapsed: boolean;
  onClick: () => void;
}> = ({ item, active, badge, collapsed, onClick }) => (
  <button
    className={`nav-item${active ? ' active' : ''}${collapsed ? ' is-collapsed' : ''}`}
    title={collapsed ? item.label : undefined}
    onClick={onClick}
  >
    <span className="ico">
      {item.icon}
      {collapsed && badge ? <span className="badge sm abs">{badge}</span> : null}
    </span>
    {!collapsed && <span className="label">{item.label}</span>}
    {!collapsed && badge ? <span className="badge">{badge}</span> : null}
  </button>
);

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab, onTabChange, onOpenConnect, onDisconnect, onSwitchToChat,
  onListLocalSessions, onSwitchLocalSession, onNewLocalSession,
  onDeleteLocalSession, onRenameLocalSession,
}) => {
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-collapsed') === 'true'; } catch { return false; }
  });

  const toggleCollapsed = () => setCollapsed(prev => {
    const next = !prev;
    try { localStorage.setItem('sidebar-collapsed', String(next)); } catch {}
    return next;
  });

  // Selective subscriptions — avoid re-rendering on every streaming token.
  const pendingCount = useAgentStore(s => (s.pendingConfirmations ?? []).length);
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const serverUrl = useAgentStore(s => s.serverUrl);
  const workdir = useAgentStore(s => s.workdir);
  const connected = connectionStatus === 'connected';

  return (
    <aside className={`sidebar${collapsed ? ' collapsed' : ''}`}>
      <div className="sidebar-head">
        <button
          className="icon-btn"
          onClick={toggleCollapsed}
          title={collapsed ? '展开侧边栏' : '收起侧边栏'}
        >
          {collapsed ? '▶' : '◀'}
        </button>
      </div>

      <div className="sidebar-body">
        <div className="nav-list">
          {NAV.map(item => (
            <NavItem
              key={item.tab}
              item={item}
              active={activeTab === item.tab}
              badge={item.tab === 'chat' ? (pendingCount || undefined) : undefined}
              collapsed={collapsed}
              onClick={() => onTabChange(item.tab)}
            />
          ))}
        </div>

        {!collapsed && (
          <SessionList
            isConnected={connected}
            onSwitchToChat={onSwitchToChat}
            onListLocalSessions={onListLocalSessions}
            onSwitchLocalSession={onSwitchLocalSession}
            onNewLocalSession={onNewLocalSession}
            onDeleteLocalSession={onDeleteLocalSession}
            onRenameLocalSession={onRenameLocalSession}
          />
        )}

        {!collapsed && (
          <div className="conn-block">
            <div className="side-head">
              <span className="side-title">连接</span>
              <button className="side-add" onClick={onOpenConnect} title="连接 / 切换项目">＋</button>
            </div>
            <div className="conn-row">
              <span className={`dot ${connectionStatus}`} />
              <span className="conn-text">{connected ? serverUrl : '未连接'}</span>
            </div>
            {connected && workdir && (
              <div className="conn-row">📂<span className="conn-text">{workdir}</span></div>
            )}
            {connected && (
              <button className="btn-ghost conn-open" onClick={onDisconnect}>断开连接</button>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
