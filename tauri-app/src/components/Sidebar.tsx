import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ConfigPreset } from '../types/agent';

type Tab = 'chat' | 'tools' | 'settings' | 'sessions' | 'sandbox' | 'nodes' | 'plugins' | 'models' | 'workflows';

interface SidebarProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onOpenConnect: () => void;
  onQuickConnect?: () => void;  // 可选：快速连接函数
  onNewSession: () => void;
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

// 缩写服务器地址函数
function abbreviateServerUrl(url: string): string {
  try {
    const u = new URL(url);
    return `${u.hostname}${u.port ? ':' + u.port : ''}`;
  } catch {
    return url.length > 20 ? url.substring(0, 17) + '...' : url;
  }
}

// 缩写工作目录路径（取最后两级目录）
function abbreviateWorkdir(workdir: string): string {
  // 去掉末尾的 /
  const cleaned = workdir.replace(/\/+$/, '');
  const parts = cleaned.split('/').filter(Boolean);
  if (parts.length <= 2) return cleaned;
  // 显示最后两级: .../parent/current
  return '…/' + parts.slice(-2).join('/');
}

// 预设配置项组件
interface PresetItemProps {
  preset: ConfigPreset;
  isActive: boolean;
  isConnecting: boolean;
  onConnect: (presetId: string) => void;
}

const PresetItem: React.FC<PresetItemProps> = ({ preset, isActive, isConnecting, onConnect }) => (
  <div
    style={{
      display: 'flex',
      alignItems: 'center',
      gap: '8px',
      padding: '6px 8px',
      background: isActive ? 'var(--accent-glow)' : 'var(--bg3)',
      border: isActive ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
      borderRadius: '6px',
      cursor: 'pointer',
      transition: 'all 0.15s',
    }}
    onClick={() => !isConnecting && onConnect(preset.id)}
    onMouseOver={(e) => { if (!isActive) e.currentTarget.style.background = 'var(--bg4)'; }}
    onMouseOut={(e) => { if (!isActive) e.currentTarget.style.background = 'var(--bg3)'; }}
  >
    <div style={{ flex: 1, minWidth: 0 }}>
      <div style={{ fontSize: '12px', fontWeight: '500', color: 'var(--text)', marginBottom: '2px' }}>
        {preset.name}
      </div>
      <div style={{ fontSize: '10px', color: 'var(--text3)', fontFamily: 'monospace' }}>
        {abbreviateServerUrl(preset.serverUrl)}
      </div>
      {preset.workdir && (
        <div style={{ fontSize: '10px', color: 'var(--text3)', fontFamily: 'monospace', marginTop: '1px' }}>
          📂 {abbreviateWorkdir(preset.workdir)}
        </div>
      )}
    </div>
    <button
      style={{
        width: '20px',
        height: '20px',
        background: isActive ? 'var(--green)' : 'var(--accent)',
        color: '#fff',
        borderRadius: '4px',
        border: 'none',
        cursor: isConnecting ? 'not-allowed' : 'pointer',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: '10px',
        opacity: isConnecting ? 0.7 : 1,
      }}
      disabled={isConnecting}
      title={isActive ? '已连接' : '连接'}
    >
      {isConnecting ? '...' : (isActive ? '✓' : '🔗')}
    </button>
  </div>
);

// 预设配置区域组件
interface PresetsSectionProps {
  presets: ConfigPreset[];
  currentServerUrl: string;
  currentWorkdir?: string;
  connectionStatus: string;
  onConnect: (presetId: string) => void;
  onOpenSettings: () => void;
}

const PresetsSection: React.FC<PresetsSectionProps> = ({ 
  presets, 
  currentServerUrl,
  currentWorkdir,
  connectionStatus, 
  onConnect, 
  onOpenSettings 
}) => {
  const [expanded, setExpanded] = useState(false);
  
  if (!presets || presets.length === 0) {
    return null;
  }
  
  return (
    <div style={{ marginTop: '16px' }}>
      <div 
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: '8px',
          cursor: 'pointer',
          padding: '4px',
        }}
        onClick={() => setExpanded(!expanded)}
      >
        <p style={{ fontSize: '10px', fontWeight: '600', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
          预设配置 ({presets.length})
        </p>
        <span style={{ fontSize: '10px', color: 'var(--text3)', transition: 'transform 0.2s', transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)' }}>
          ▶
        </span>
      </div>
      
      {expanded && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', marginBottom: '8px' }}>
          {presets.map(preset => (
            <PresetItem
              key={preset.id}
              preset={preset}
              isActive={connectionStatus === 'connected' && preset.serverUrl === currentServerUrl && ((!preset.workdir && !currentWorkdir) || preset.workdir === currentWorkdir)}
              isConnecting={connectionStatus === 'connecting'}
              onConnect={onConnect}
            />
          ))}
          <div style={{ textAlign: 'center', marginTop: '4px' }}>
            <button
              onClick={onOpenSettings}
              style={{
                fontSize: '10px',
                color: 'var(--text3)',
                background: 'transparent',
                border: 'none',
                cursor: 'pointer',
                textDecoration: 'underline',
              }}
            >
              管理预设
            </button>
          </div>
        </div>
      )}
    </div>
  );
};

export const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange, onOpenConnect, onQuickConnect, onNewSession }) => {
  // Collapse state — persisted in localStorage
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('sidebar-collapsed') === 'true'; } catch { return false; }
  });
  const toggleCollapsed = () => {
    setCollapsed(prev => {
      const next = !prev;
      try { localStorage.setItem('sidebar-collapsed', String(next)); } catch {}
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
  const presets = useAgentStore(s => s.presets ?? []);
  const applyPreset = useAgentStore(s => s.applyPreset);
  const plugins = useAgentStore(s => s.plugins ?? []);
  // Derived values — primitive selectors only fire on actual value change
  const runningTools = useAgentStore(s => (s.toolCalls ?? []).filter(t => t.status === 'executing').length);
  const pendingCount = useAgentStore(s => (s.pendingConfirmations ?? []).length);
  
  // 处理预设快速连接
  const handleQuickConnect = (presetId: string) => {
    // 应用预设配置
    applyPreset(presetId);
    // 如果有快速连接函数，直接调用它
    if (onQuickConnect) {
      onQuickConnect();
    } else {
      // 否则显示连接模态框
      onOpenConnect();
    }
  };
  
  // 打开设置页面并切换到预设标签页
  const handleOpenSettings = () => {
    onTabChange('settings');
  };

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
          <p style={{ fontSize: '10px', fontWeight: '600', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: '8px', paddingLeft: '4px' }}>
            导航
          </p>
        )}
        <div style={{ display: 'flex', flexDirection: 'column', gap: collapsed ? '2px' : '3px' }}>
          <NavItem icon="💬" label="对话" active={activeTab === 'chat'} badge={pendingCount || undefined} collapsed={collapsed} onClick={() => onTabChange('chat')} />
          <NavItem icon="🔨" label="工具调用" active={activeTab === 'tools'} badge={runningTools || undefined} collapsed={collapsed} onClick={() => onTabChange('tools')} />
          <NavItem icon="🌐" label="节点" active={activeTab === 'nodes'} badge={nodeList.length || undefined} collapsed={collapsed} onClick={() => onTabChange('nodes')} />
          <NavItem icon="📚" label="会话管理" active={activeTab === 'sessions'} collapsed={collapsed} onClick={() => onTabChange('sessions')} />
          <NavItem icon="🔒" label="沙盒" active={activeTab === 'sandbox'} badge={pendingChanges || undefined} collapsed={collapsed} onClick={() => onTabChange('sandbox')} />
          <NavItem icon="🧩" label="插件" active={activeTab === 'plugins'} badge={plugins.length || undefined} collapsed={collapsed} onClick={() => onTabChange('plugins')} />
          <NavItem icon="⚙️" label="工作流" active={activeTab === 'workflows'} collapsed={collapsed} onClick={() => onTabChange('workflows')} />
          <NavItem icon="🧠" label="模型" active={activeTab === 'models'} collapsed={collapsed} onClick={() => onTabChange('models')} />
          <NavItem icon="⚙️" label="设置" active={activeTab === 'settings'} collapsed={collapsed} onClick={() => onTabChange('settings')} />
        </div>

        {/* 预设配置区域 — only when expanded */}
        {!collapsed && (
          <PresetsSection
            presets={presets}
            currentServerUrl={serverUrl}
            currentWorkdir={workdir}
            connectionStatus={connectionStatus}
            onConnect={handleQuickConnect}
            onOpenSettings={handleOpenSettings}
          />
        )}
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
