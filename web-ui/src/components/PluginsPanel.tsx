import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { PluginInfo } from '../types/agent';

interface PluginsPanelProps {
  onEnablePlugin: (id: string) => void;
  onDisablePlugin: (id: string) => void;
}

export const PluginsPanel: React.FC<PluginsPanelProps> = ({ onEnablePlugin, onDisablePlugin }) => {
  const plugins = useAgentStore(s => s.plugins ?? []);
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const isConnected = connectionStatus === 'connected';

  const toggleExpand = (id: string) => {
    setExpandedId(prev => prev === id ? null : id);
  };

  const handleToggleEnabled = (e: React.MouseEvent, plugin: PluginInfo) => {
    e.stopPropagation();
    if (plugin.enabled) {
      onDisablePlugin(plugin.id);
    } else {
      onEnablePlugin(plugin.id);
    }
  };

  // Empty state when not connected
  if (!isConnected) {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center', gap: '12px',
        color: 'var(--text3)', padding: '40px',
      }}>
        <span style={{ fontSize: '40px' }}>🧩</span>
        <p style={{ fontSize: '14px', fontWeight: '500' }}>未连接</p>
        <p style={{ fontSize: '12px', textAlign: 'center', lineHeight: 1.6 }}>
          连接到服务器后，插件列表将自动显示。
        </p>
      </div>
    );
  }

  // Empty state when no plugins
  if (plugins.length === 0) {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center', gap: '12px',
        color: 'var(--text3)', padding: '40px',
      }}>
        <span style={{ fontSize: '40px' }}>🧩</span>
        <p style={{ fontSize: '14px', fontWeight: '500' }}>暂无插件</p>
        <p style={{ fontSize: '12px', textAlign: 'center', lineHeight: 1.6 }}>
          当前服务器没有安装插件，<br />或插件系统不支持。
        </p>
      </div>
    );
  }

  const enabledCount = plugins.filter(p => p.enabled).length;

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
      <div style={{ marginBottom: '16px' }}>
        <h2 style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', marginBottom: '4px' }}>
          🧩 插件列表
        </h2>
        <p style={{ fontSize: '12px', color: 'var(--text3)' }}>
          共 {plugins.length} 个插件，{enabledCount} 个已启用。点击卡片查看详情。
        </p>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
        {plugins.map((plugin) => {
          const isExpanded = expandedId === plugin.id;
          return (
            <div
              key={plugin.id}
              onClick={() => toggleExpand(plugin.id)}
              style={{
                padding: '14px 16px',
                background: isExpanded ? 'var(--bg3)' : 'var(--bg2)',
                border: isExpanded
                  ? '1px solid rgba(99,102,241,0.3)'
                  : '1px solid var(--border)',
                borderRadius: '10px',
                cursor: 'pointer',
                transition: 'all 0.15s',
              }}
              onMouseOver={(e) => { if (!isExpanded) e.currentTarget.style.background = 'var(--bg3)'; }}
              onMouseOut={(e) => { if (!isExpanded) e.currentTarget.style.background = 'var(--bg2)'; }}
            >
              {/* Header row */}
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
                <span style={{ fontSize: '16px' }}>🧩</span>
                <span style={{ fontSize: '13px', fontWeight: '600', color: 'var(--text)' }}>
                  {plugin.name}
                </span>
                <span style={{
                  fontSize: '10px', color: 'var(--text3)', fontFamily: 'monospace',
                }}>
                  v{plugin.version}
                </span>

                {/* Enabled/Disabled badge */}
                <span
                  onClick={(e) => handleToggleEnabled(e, plugin)}
                  style={{
                    marginLeft: 'auto',
                    fontSize: '10px', fontWeight: '600',
                    color: plugin.enabled ? 'var(--green)' : 'var(--text3)',
                    background: plugin.enabled ? 'var(--green-dim)' : 'var(--bg3)',
                    border: plugin.enabled
                      ? '1px solid rgba(16,185,129,0.3)'
                      : '1px solid var(--border)',
                    borderRadius: '4px',
                    padding: '2px 8px',
                    cursor: 'pointer',
                    transition: 'all 0.15s',
                  }}
                  title={plugin.enabled ? '点击禁用' : '点击启用'}
                >
                  {plugin.enabled ? '✅ 已启用' : '⏸ 已禁用'}
                </span>

                {/* Tools count badge */}
                <span style={{
                  fontSize: '10px',
                  color: 'var(--accent)',
                  background: 'var(--accent-glow)',
                  border: '1px solid rgba(99,102,241,0.3)',
                  borderRadius: '4px',
                  padding: '1px 6px',
                }}>
                  🔧 {plugin.tools.length}
                </span>
              </div>

              {/* Description */}
              <p style={{
                fontSize: '11px', color: 'var(--text2)', marginBottom: isExpanded ? '10px' : 0,
                lineHeight: 1.5,
              }}>
                {plugin.description}
              </p>

              {/* Expanded detail */}
              {isExpanded && (
                <div style={{
                  marginTop: '10px', paddingTop: '10px',
                  borderTop: '1px solid var(--border)',
                }}>
                  {/* Author & Homepage */}
                  {(plugin.author || plugin.homepage) && (
                    <div style={{ marginBottom: '10px', display: 'flex', flexDirection: 'column', gap: '4px' }}>
                      {plugin.author && (
                        <div style={{ fontSize: '11px', color: 'var(--text3)' }}>
                          <span style={{ fontWeight: '500', color: 'var(--text2)' }}>作者：</span>
                          {plugin.author}
                        </div>
                      )}
                      {plugin.homepage && (
                        <div style={{ fontSize: '11px', color: 'var(--text3)' }}>
                          <span style={{ fontWeight: '500', color: 'var(--text2)' }}>主页：</span>
                          <a
                            href={plugin.homepage}
                            target="_blank"
                            rel="noopener noreferrer"
                            style={{ color: 'var(--accent)', textDecoration: 'none' }}
                            onClick={(e) => e.stopPropagation()}
                          >
                            {plugin.homepage}
                          </a>
                        </div>
                      )}
                    </div>
                  )}

                  {/* Tools list */}
                  <div>
                    <span style={{ fontSize: '11px', fontWeight: '600', color: 'var(--text2)' }}>
                      提供的工具 ({plugin.tools.length})：
                    </span>
                    <div style={{
                      display: 'flex', flexWrap: 'wrap', gap: '4px',
                      marginTop: '6px',
                    }}>
                      {plugin.tools.map(tool => (
                        <span key={tool} style={{
                          fontSize: '10px',
                          padding: '2px 8px',
                          borderRadius: '10px',
                          background: 'var(--accent-glow)',
                          color: 'var(--accent)',
                          border: '1px solid rgba(99,102,241,0.2)',
                          fontFamily: 'monospace',
                        }}>
                          {tool}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};
