import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ProjectTree } from './ProjectTree';

type Tab = 'chat' | 'settings';

interface SidebarProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onOpenConnect: () => void;
  onQuickConnect?: () => void;  // 保留以兼容调用方
  onSwitchLocalSession?: (name: string) => void;
  onNewLocalSession?: (name: string) => void;
  onConnectProject?: (id: string) => void;
  onEditProject?: (id: string) => void;
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
  activeTab,
  onTabChange,
  onOpenConnect,
  onSwitchLocalSession,
  onNewLocalSession,
  onConnectProject,
  onEditProject,
}) => {
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-collapsed') === 'true'; } catch { return false; }
  });

  const toggleCollapsed = () => setCollapsed(prev => {
    const next = !prev;
    try { localStorage.setItem('sidebar-collapsed', String(next)); } catch {}
    return next;
  });

  // Selective subscription — avoid re-rendering on every streaming token.
  const pendingCount = useAgentStore(s => (s.pendingConfirmations ?? []).length);

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

        <ProjectTree
          collapsed={collapsed}
          onOpenConnect={onOpenConnect}
          onSwitchLocalSession={onSwitchLocalSession}
          onNewLocalSession={onNewLocalSession}
          onConnectProject={onConnectProject}
          onEditProject={onEditProject}
        />
      </div>
    </aside>
  );
};
