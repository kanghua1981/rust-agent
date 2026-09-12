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
      <div className="empty-state">
        <span className="empty-icon">🧩</span>
        <p className="empty-title">未连接</p>
        <p className="empty-sub">连接到服务器后，插件列表将自动显示。</p>
      </div>
    );
  }

  // Empty state when no plugins
  if (plugins.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-icon">🧩</span>
        <p className="empty-title">暂无插件</p>
        <p className="empty-sub">当前服务器没有安装插件，<br />或插件系统不支持。</p>
      </div>
    );
  }

  const enabledCount = plugins.filter(p => p.enabled).length;

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
      <div style={{ marginBottom: 16 }}>
        <h2 className="panel-head-title">🧩 插件列表</h2>
        <p style={{ fontSize: 12, color: 'var(--text3)', marginTop: 4 }}>
          共 {plugins.length} 个插件，{enabledCount} 个已启用。点击卡片查看详情。
        </p>
      </div>

      <div className="list-stack">
        {plugins.map((plugin) => {
          const isExpanded = expandedId === plugin.id;
          return (
            <div
              key={plugin.id}
              className={`plugin-card${isExpanded ? ' expanded' : ''}`}
              onClick={() => toggleExpand(plugin.id)}
            >
              {/* Header row */}
              <div className="plugin-head">
                <span style={{ fontSize: 16 }}>🧩</span>
                <span className="plugin-name">{plugin.name}</span>
                <span className="plugin-version">v{plugin.version}</span>

                {/* Enabled/Disabled badge */}
                <span
                  className={`plugin-toggle${plugin.enabled ? ' on' : ''}`}
                  onClick={(e) => handleToggleEnabled(e, plugin)}
                  title={plugin.enabled ? '点击禁用' : '点击启用'}
                >
                  {plugin.enabled ? '✅ 已启用' : '⏸ 已禁用'}
                </span>

                {/* Tools count badge */}
                <span className="plugin-tools-badge">🔧 {plugin.tools.length}</span>
              </div>

              {/* Description */}
              <p className={`plugin-desc${isExpanded ? ' expanded' : ''}`}>{plugin.description}</p>

              {/* Expanded detail */}
              {isExpanded && (
                <div className="plugin-detail">
                  {/* Author & Homepage */}
                  {(plugin.author || plugin.homepage) && (
                    <div className="plugin-meta">
                      {plugin.author && (
                        <div><span className="k">作者：</span>{plugin.author}</div>
                      )}
                      {plugin.homepage && (
                        <div>
                          <span className="k">主页：</span>
                          <a
                            className="plugin-link"
                            href={plugin.homepage}
                            target="_blank"
                            rel="noopener noreferrer"
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
                    <span className="plugin-tools-label">提供的工具 ({plugin.tools.length})：</span>
                    <div className="tag-row" style={{ marginTop: 6 }}>
                      {plugin.tools.map(tool => (
                        <span key={tool} className="tag mono accent" style={{ padding: '2px 8px' }}>{tool}</span>
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
