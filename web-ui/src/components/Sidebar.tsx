import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ProjectTree } from './ProjectTree';

type Tab = 'chat' | 'settings' | 'nodes' | 'plugins' | 'models';

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
  { tab: 'nodes',    icon: '🌐', label: '节点' },
  { tab: 'plugins',  icon: '🧩', label: '插件' },
  { tab: 'models',   icon: '🧠', label: '模型' },
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
  const [navCollapsed, setNavCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-nav-collapsed') === 'true'; } catch { return false; }
  });

  const toggleCollapsed = () => setCollapsed(prev => {
    const next = !prev;
    try { localStorage.setItem('sidebar-collapsed', String(next)); } catch {}
    return next;
  });
  const toggleNavCollapsed = () => setNavCollapsed(prev => {
    const next = !prev;
    try { localStorage.setItem('sidebar-nav-collapsed', String(next)); } catch {}
    return next;
  });

  // Selective subscriptions — avoid re-rendering on every streaming token.
  const nodeList = useAgentStore(s => s.nodeList ?? []);
  const plugins = useAgentStore(s => s.plugins ?? []);
  const pendingCount = useAgentStore(s => (s.pendingConfirmations ?? []).length);

  const badgeFor = (tab: Tab): number | undefined =>
    tab === 'chat' ? (pendingCount || undefined)
    : tab === 'nodes' ? (nodeList.length || undefined)
    : tab === 'plugins' ? (plugins.length || undefined)
    : undefined;

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
        {!collapsed && (
          <div
            className="section-toggle"
            onClick={toggleNavCollapsed}
            title={navCollapsed ? '展开导航' : '收起导航'}
          >
            <span className="section-label">导航</span>
            <span
              className="section-chevron"
              style={{ transform: navCollapsed ? 'none' : 'rotate(90deg)' }}
            >▶</span>
          </div>
        )}

        {!collapsed && navCollapsed ? (
          <div className="nav-rail">
            {NAV.map(item => (
              <button
                key={item.tab}
                className={activeTab === item.tab ? 'active' : undefined}
                title={item.label}
                onClick={(e) => { e.stopPropagation(); onTabChange(item.tab); }}
              >{item.icon}</button>
            ))}
          </div>
        ) : (
          <div className="nav-list">
            {NAV.map(item => (
              <NavItem
                key={item.tab}
                item={item}
                active={activeTab === item.tab}
                badge={badgeFor(item.tab)}
                collapsed={collapsed}
                onClick={() => onTabChange(item.tab)}
              />
            ))}
          </div>
        )}

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
