import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ProjectTree } from './ProjectTree';

type Tab = 'chat' | 'settings' | 'nodes' | 'plugins' | 'models' | 'workflows' | 'pipelines';

interface SidebarProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onOpenConnect: () => void;
  onQuickConnect?: () => void;  // 可选：快速连接函数
  onSwitchLocalSession?: (name: string) => void;
  onNewLocalSession?: (name: string) => void;
  onConnectProject?: (id: string) => void;
  onEditProject?: (id: string) => void;
}

const NavItem: React.FC<{
  icon: string;
  label: string;
  active: boolean;
  badge?: number;
  collapsed?: boolean;
  onClick: () => void;
}> = ({ icon, label, active, badge, collapsed, onClick }) => (
  <button
    onClick={onClick}
    title={collapsed ? label : undefined}
    style={{
      display: 'flex', alignItems: 'center', justifyContent: collapsed ? 'center' : 'flex-start', gap: collapsed ? '0' : '9px',
      padding: collapsed ? '10px 0' : '8px 10px',
      width: '100%',
      borderRadius: '8px',
      background: active ? 'var(--accent-glow)' : 'transparent',
      color: active ? 'var(--accent)' : 'var(--text2)',
      fontWeight: active ? '500' : '400',
      fontSize: '13px',
      border: active ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
      transition: 'all 0.15s',
      textAlign: collapsed ? 'center' : 'left',
      position: 'relative',
    }}
    onMouseOver={(e) => { if (!active) e.currentTarget.style.background = 'var(--bg3)'; }}
    onMouseOut={(e) => { if (!active) e.currentTarget.style.background = 'transparent'; }}
  >
    <span style={{ fontSize: '15px', width: collapsed ? 'auto' : '18px', textAlign: 'center', flexShrink: 0, position: 'relative' }}>
      {icon}
      {collapsed && badge !== undefined && badge > 0 && (
        <span style={{
          position: 'absolute', top: '-4px', right: '-8px',
          background: 'var(--red)', color: '#fff',
          borderRadius: '10px', padding: '1px 5px',
          fontSize: '9px', fontWeight: '600',
          lineHeight: '14px', minWidth: '16px', textAlign: 'center',
        }}>{badge}</span>
      )}
    </span>
    {!collapsed && <span style={{ flex: 1 }}>{label}</span>}
    {!collapsed && badge !== undefined && badge > 0 && (
      <span style={{
        background: 'var(--red)',
        color: '#fff',
        borderRadius: '10px',
        padding: '1px 6px',
        fontSize: '11px',
        fontWeight: '600',
        minWidth: '18px',
        textAlign: 'center',
      }}>{badge}</span>
    )}
  </button>
);


export const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange, onOpenConnect, onQuickConnect, onSwitchLocalSession, onNewLocalSession, onConnectProject, onEditProject }) => {
  // Collapse state — persisted in localStorage
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-collapsed') === 'true'; } catch { return false; }
  });
  // Section-level collapse: hide the nav items to give project tree more room
  const [navCollapsed, setNavCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-nav-collapsed') === 'true'; } catch { return false; }
  });
  const toggleCollapsed = () => {
    setCollapsed(prev => {
      const next = !prev;
      try { localStorage.setItem('sidebar-collapsed', String(next)); } catch {}
      return next;
    });
  };
  const toggleNavCollapsed = () => {
    setNavCollapsed(prev => {
      const next = !prev;
      try { localStorage.setItem('sidebar-nav-collapsed', String(next)); } catch {}
      return next;
    });
  };

  // Selective subscriptions — subscribe only to what the component renders.
  // Avoids re-rendering on every streaming token / toolCall / message change.
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const serverUrl = useAgentStore(s => s.serverUrl);
  const workdir = useAgentStore(s => s.workdir);
  const pendingChanges = useAgentStore(s => s.pendingChanges);
  const nodeList = useAgentStore(s => s.nodeList ?? []);
  const plugins = useAgentStore(s => s.plugins ?? []);
  // Derived values — primitive selectors only fire on actual value change
  const pendingCount = useAgentStore(s => (s.pendingConfirmations ?? []).length);

  return (
    <aside style={{
      width: collapsed ? '48px' : '220px',
      background: 'var(--bg2)',
      borderRight: '1px solid var(--border)',
      display: 'flex',
      flexDirection: 'column',
      flexShrink: 0,
      overflowY: 'auto',
      overflowX: 'hidden',
      maxHeight: '100vh',
      transition: 'width 0.2s ease',
    }}>
      {/* Toggle button */}
      <div style={{
        display: 'flex', justifyContent: collapsed ? 'center' : 'flex-end',
        padding: collapsed ? '10px 0 4px' : '8px 10px 4px',
      }}>
        <button
          onClick={toggleCollapsed}
          title={collapsed ? '展开侧边栏' : '收起侧边栏'}
          style={{
            width: '24px', height: '24px',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            borderRadius: '6px',
            color: 'var(--text3)',
            fontSize: '12px',
            transition: 'all 0.15s',
          }}
          onMouseOver={(e) => { e.currentTarget.style.background = 'var(--bg3)'; e.currentTarget.style.color = 'var(--text)'; }}
          onMouseOut={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = 'var(--text3)'; }}
        >
          {collapsed ? '▶' : '◀'}
        </button>
      </div>

      {/* Navigation */}
      <div style={{ padding: collapsed ? '0 6px' : '16px 12px 12px' }}>
        {!collapsed && (
          <div
            style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              marginBottom: '6px', paddingLeft: '4px', cursor: 'pointer',
            }}
            onClick={toggleNavCollapsed}
            title={navCollapsed ? '展开导航' : '收起导航'}
          >
            <span style={{ fontSize: '10px', fontWeight: '600', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
              导航
            </span>
            <span style={{
              fontSize: '10px', color: 'var(--text3)',
              transition: 'transform 0.2s',
              transform: navCollapsed ? 'rotate(0deg)' : 'rotate(90deg)',
            }}>▶</span>
          </div>
        )}

        {(!collapsed && navCollapsed) ? (
          // Collapsed nav: single-row icon strip showing only the active tab
          <div style={{
            display: 'flex', gap: '4px', flexWrap: 'wrap',
            marginBottom: '8px',
          }}>
            {(() => {
              const items = [
                { tab: 'chat' as Tab, icon: '💬', label: '对话' },
                { tab: 'nodes' as Tab, icon: '🌐', label: '节点' },
                { tab: 'plugins' as Tab, icon: '🧩', label: '插件' },
                { tab: 'workflows' as Tab, icon: '🔄', label: '工作流' },
                { tab: 'pipelines' as Tab, icon: '🚀', label: '流水线' },
                { tab: 'models' as Tab, icon: '🧠', label: '模型' },
                { tab: 'settings' as Tab, icon: '⚙️', label: '设置' },
              ];
              return items.map(item => (
                <button
                  key={item.tab}
                  onClick={(e) => { e.stopPropagation(); onTabChange(item.tab); }}
                  title={item.label}
                  style={{
                    width: '28px', height: '28px',
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    borderRadius: '6px',
                    fontSize: '14px',
                    background: activeTab === item.tab ? 'var(--accent-glow)' : 'transparent',
                    border: activeTab === item.tab ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
                    cursor: 'pointer',
                    transition: 'all 0.15s',
                    opacity: activeTab === item.tab ? 1 : 0.5,
                  }}
                  onMouseOver={(e) => { e.currentTarget.style.opacity = '1'; }}
                  onMouseOut={(e) => { e.currentTarget.style.opacity = activeTab === item.tab ? '1' : '0.5'; }}
                >{item.icon}</button>
              ));
            })()}
          </div>
        ) : (
          // Expanded nav: full items
          <div style={{ display: 'flex', flexDirection: 'column', gap: collapsed ? '2px' : '3px' }}>
            <NavItem icon="💬" label="对话" active={activeTab === 'chat'} badge={pendingCount || undefined} collapsed={collapsed} onClick={() => onTabChange('chat')} />
            <NavItem icon="🌐" label="节点" active={activeTab === 'nodes'} badge={nodeList.length || undefined} collapsed={collapsed} onClick={() => onTabChange('nodes')} />
            <NavItem icon="🧩" label="插件" active={activeTab === 'plugins'} badge={plugins.length || undefined} collapsed={collapsed} onClick={() => onTabChange('plugins')} />
            <NavItem icon="🔄" label="工作流" active={activeTab === 'workflows'} collapsed={collapsed} onClick={() => onTabChange('workflows')} />
            <NavItem icon="🚀" label="流水线" active={activeTab === 'pipelines'} collapsed={collapsed} onClick={() => onTabChange('pipelines')} />
            <NavItem icon="🧠" label="模型" active={activeTab === 'models'} collapsed={collapsed} onClick={() => onTabChange('models')} />
            <NavItem icon="⚙️" label="设置" active={activeTab === 'settings'} collapsed={collapsed} onClick={() => onTabChange('settings')} />
          </div>
        )}

        {/* ── Project Tree (Project-First) ── */}
        <ProjectTree collapsed={collapsed} onOpenConnect={onOpenConnect} onSwitchLocalSession={onSwitchLocalSession} onNewLocalSession={onNewLocalSession} onConnectProject={onConnectProject} onEditProject={onEditProject} />

      </div>

      {/* Bottom: connection status — dot only when collapsed */}
      <div style={{ 
        padding: collapsed ? '8px 0' : '8px 10px', 
        borderTop: '1px solid var(--border)',
        marginTop: 'auto',
        flexShrink: 0,
        display: 'flex', justifyContent: 'center',
      }}>
        {collapsed ? (
          <span
            onClick={onOpenConnect}
            title={connectionStatus === 'connected' ? '已连接' : connectionStatus === 'connecting' ? '连接中…' : '未连接'}
            style={{
              width: '8px', height: '8px', borderRadius: '50%',
              background: connectionStatus === 'connected' ? '#10b981' : 
                        connectionStatus === 'connecting' ? '#f59e0b' : 
                        connectionStatus === 'error' ? '#ef4444' : '#6b7280',
              cursor: 'pointer',
            }}
          />
        ) : (
          <div 
            onClick={onOpenConnect}
            style={{
              background: connectionStatus === 'connected' ? 'var(--green-dim)' : 'var(--yellow-dim)',
              border: connectionStatus === 'connected' ? '1px solid rgba(16,185,129,0.3)' : '1px solid rgba(245,158,11,0.3)',
              borderRadius: '8px',
              padding: '8px',
              cursor: 'pointer',
              transition: 'all 0.15s',
              flex: 1,
            }}
            onMouseOver={(e) => e.currentTarget.style.opacity = '0.9'}
            onMouseOut={(e) => e.currentTarget.style.opacity = '1'}
          >
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '2px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span style={{
                  width: '6px', height: '6px', borderRadius: '50%',
                  background: connectionStatus === 'connected' ? '#10b981' : 
                            connectionStatus === 'connecting' ? '#f59e0b' : 
                            connectionStatus === 'error' ? '#ef4444' : '#6b7280',
                  flexShrink: 0,
                }} />
                <span style={{ 
                  fontSize: '11px', 
                  fontWeight: '500', 
                  color: connectionStatus === 'connected' ? 'var(--green)' : 
                        connectionStatus === 'connecting' ? 'var(--yellow)' : 
                        connectionStatus === 'error' ? 'var(--red)' : 'var(--text3)'
                }}>
                  {connectionStatus === 'connected' ? '已连接' : 
                   connectionStatus === 'connecting' ? '连接中…' : 
                   connectionStatus === 'error' ? '连接错误' : '未连接'}
                </span>
              </div>
              <span style={{ fontSize: '10px', color: 'var(--text3)' }}>点击管理</span>
            </div>
            
            <p className="truncate" style={{ 
              fontSize: '10px', 
              color: 'var(--text2)', 
              fontFamily: 'monospace',
              marginBottom: '2px'
            }}>
              {serverUrl}
            </p>
            
            {connectionStatus === 'connected' && workdir && (
              <p className="truncate" style={{ 
                fontSize: '10px', 
                color: 'var(--text3)', 
                fontFamily: 'monospace'
              }}>
                📂 {workdir}
              </p>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
