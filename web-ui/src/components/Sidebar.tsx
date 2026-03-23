import React from 'react';
import { useAgentStore } from '../stores/agentStore';

type Tab = 'chat' | 'tools' | 'settings' | 'sessions' | 'sandbox';

interface SidebarProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onOpenConnect: () => void;
  onLoadSession: () => void;
  onNewSession: () => void;
}

const NavItem: React.FC<{
  icon: string;
  label: string;
  active: boolean;
  badge?: number;
  onClick: () => void;
}> = ({ icon, label, active, badge, onClick }) => (
  <button
    onClick={onClick}
    style={{
      display: 'flex', alignItems: 'center', gap: '9px',
      padding: '8px 10px',
      width: '100%',
      borderRadius: '8px',
      background: active ? 'var(--accent-glow)' : 'transparent',
      color: active ? 'var(--accent)' : 'var(--text2)',
      fontWeight: active ? '500' : '400',
      fontSize: '13px',
      border: active ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
      transition: 'all 0.15s',
      textAlign: 'left',
      position: 'relative',
    }}
    onMouseOver={(e) => { if (!active) e.currentTarget.style.background = 'var(--bg3)'; }}
    onMouseOut={(e) => { if (!active) e.currentTarget.style.background = 'transparent'; }}
  >
    <span style={{ fontSize: '15px', width: '18px', textAlign: 'center', flexShrink: 0 }}>{icon}</span>
    <span style={{ flex: 1 }}>{label}</span>
    {badge !== undefined && badge > 0 && (
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

export const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange, onOpenConnect, onLoadSession, onNewSession }) => {
  const { connectionStatus, toolCalls, pendingConfirmations, serverUrl, config, setConfig, messages, sessionInfo, pendingChanges } = useAgentStore();

  const runningTools = toolCalls.filter(t => t.status === 'executing').length;

  return (
    <aside style={{
      width: '220px',
      background: 'var(--bg2)',
      borderRight: '1px solid var(--border)',
      display: 'flex',
      flexDirection: 'column',
      flexShrink: 0,
      overflow: 'hidden',
    }}>
      {/* Navigation */}
      <div style={{ padding: '16px 12px 12px', flex: 1 }}>
        <p style={{ fontSize: '10px', fontWeight: '600', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: '8px', paddingLeft: '4px' }}>
          导航
        </p>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '3px' }}>
          <NavItem icon="💬" label="对话" active={activeTab === 'chat'} badge={pendingConfirmations.length || undefined} onClick={() => onTabChange('chat')} />
          <NavItem icon="🔨" label="工具调用" active={activeTab === 'tools'} badge={runningTools || undefined} onClick={() => onTabChange('tools')} />
          <NavItem icon="📚" label="会话管理" active={activeTab === 'sessions'} onClick={() => onTabChange('sessions')} />
          <NavItem icon="🔒" label="沙盒" active={activeTab === 'sandbox'} badge={pendingChanges || undefined} onClick={() => onTabChange('sandbox')} />
          <NavItem icon="⚙️" label="设置" active={activeTab === 'settings'} onClick={() => onTabChange('settings')} />
        </div>

        {/* Stats */}
        {connectionStatus === 'connected' && (
          <div style={{ marginTop: '20px' }}>
            <p style={{ fontSize: '10px', fontWeight: '600', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: '8px', paddingLeft: '4px' }}>
              统计
            </p>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
              {[
                { label: '消息', value: messages.length },
                { label: '工具调用', value: toolCalls.length },
                { label: '待确认', value: pendingConfirmations.length },
              ].map(s => (
                <div key={s.label} style={{
                  display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                  padding: '5px 10px',
                  background: 'var(--bg3)',
                  borderRadius: '6px',
                  fontSize: '12px',
                }}>
                  <span style={{ color: 'var(--text2)' }}>{s.label}</span>
                  <span style={{ color: 'var(--text)', fontWeight: '600', fontFamily: 'monospace' }}>{s.value}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Bottom: server config */}
      <div style={{ padding: '12px', borderTop: '1px solid var(--border)' }}>
        {connectionStatus === 'disconnected' || connectionStatus === 'error' ? (
          <div style={{
            background: 'var(--yellow-dim)',
            border: '1px solid rgba(245,158,11,0.3)',
            borderRadius: '8px',
            padding: '10px',
          }}>
            <p style={{ fontSize: '12px', fontWeight: '500', color: 'var(--yellow)', marginBottom: '4px' }}>未连接</p>
            <p style={{ fontSize: '11px', color: 'var(--text2)', marginBottom: '8px' }}>默认地址：{serverUrl}</p>
            <button
              onClick={onOpenConnect}
              style={{
                width: '100%', padding: '5px', background: 'var(--yellow)', color: '#000',
                borderRadius: '6px', fontWeight: '600', fontSize: '12px',
              }}
            >
              连接
            </button>
          </div>
        ) : (
          <div style={{
            background: 'var(--green-dim)',
            border: '1px solid rgba(16,185,129,0.3)',
            borderRadius: '8px',
            padding: '10px',
          }}>
            <p style={{ fontSize: '11px', color: 'var(--green)', fontWeight: '500' }}>● 已连接</p>
            <p className="truncate" style={{ fontSize: '11px', color: 'var(--text2)', marginTop: '2px', fontFamily: 'monospace' }}>{serverUrl}</p>
            <label style={{ display: 'flex', alignItems: 'center', gap: '6px', marginTop: '8px', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={!!config.autoApprove}
                onChange={(e) => setConfig({ autoApprove: e.target.checked })}
                style={{ accentColor: 'var(--accent)', cursor: 'pointer' }}
              />
              <span style={{ fontSize: '11px', color: 'var(--text2)' }}>自动确认工具</span>
            </label>

            {/* Execution mode selector */}
            <div style={{ marginTop: '8px' }}>
              <p style={{ fontSize: '10px', color: 'var(--text3)', marginBottom: '4px' }}>执行模式</p>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '3px' }}>
                {(['auto', 'simple', 'plan', 'pipeline'] as const).map(m => {
                  const labels: Record<string, string> = { auto: '自动', simple: '单层', plan: '计划', pipeline: '流水线' };
                  const active = (config.agentMode ?? 'auto') === m;
                  return (
                    <button
                      key={m}
                      onClick={() => setConfig({ agentMode: m })}
                      style={{
                        padding: '3px 0',
                        fontSize: '10px',
                        borderRadius: '5px',
                        background: active ? 'var(--accent)' : 'var(--bg3)',
                        color: active ? '#fff' : 'var(--text2)',
                        border: 'none',
                        cursor: 'pointer',
                        fontWeight: active ? '600' : '400',
                      }}
                    >{labels[m]}</button>
                  );
                })}
              </div>
            </div>

            {/* Session info */}
            {sessionInfo?.exists && (
              <div style={{
                marginTop: '8px',
                background: 'rgba(99,102,241,0.07)',
                border: '1px solid rgba(99,102,241,0.2)',
                borderRadius: '6px',
                padding: '7px 9px',
              }}>
                <p style={{ fontSize: '10px', color: 'var(--accent)', fontWeight: '600', marginBottom: '3px' }}>
                  💾 已保存会话
                </p>
                {sessionInfo.summary && (
                  <p className="truncate" style={{ fontSize: '10px', color: 'var(--text2)', marginBottom: '2px' }}>
                    {sessionInfo.summary}
                  </p>
                )}
                <p style={{ fontSize: '10px', color: 'var(--text3)', marginBottom: '5px' }}>
                  {sessionInfo.message_count} 条消息
                  {sessionInfo.updated_at && ` · ${sessionInfo.updated_at.slice(0, 16).replace('T', ' ')}`}
                </p>
                <div style={{ display: 'flex', gap: '3px' }}>
                  <button
                    onClick={onLoadSession}
                    style={{
                      flex: 1, padding: '3px 0',
                      background: 'var(--accent)', color: '#fff',
                      borderRadius: '5px', fontSize: '10px',
                      fontWeight: '600', border: 'none', cursor: 'pointer',
                    }}
                  >
                    恢复该会话
                  </button>
                  <button
                    onClick={onNewSession}
                    style={{
                      flex: 1, padding: '3px 0',
                      background: 'var(--bg3)', color: 'var(--text2)',
                      borderRadius: '5px', fontSize: '10px',
                      fontWeight: '500', border: 'none', cursor: 'pointer',
                    }}
                    title="开始新会话"
                  >
                    新建
                  </button>
                </div>
              </div>
            )}

            {/* New session button when no saved session */}
            {!sessionInfo?.exists && (
              <div style={{
                marginTop: '8px',
                textAlign: 'center',
              }}>
                <button
                  onClick={onNewSession}
                  style={{
                    width: '100%', padding: '5px 0',
                    background: 'var(--bg3)', color: 'var(--text2)',
                    borderRadius: '6px', fontSize: '11px',
                    fontWeight: '500', border: '1px solid var(--border)',
                    cursor: 'pointer',
                  }}
                  title="清空当前对话，开始新会话"
                >
                  🆕 新会话
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
