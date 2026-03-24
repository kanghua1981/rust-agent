import React from 'react';
import { useAgentStore } from '../stores/agentStore';
import { VirtualNodeInfo } from '../types/agent';

export const NodesPanel: React.FC = () => {
  const { nodeList, workdir, setWorkdir, config, setConfig, connectionStatus, connectedWorkdir } = useAgentStore();

  const isConnected = connectionStatus === 'connected';

  // When connected: highlight the node matching the actual server-reported workdir.
  // When disconnected: highlight the node matching the pre-selected workdir.
  const isActive = (node: VirtualNodeInfo) => {
    const ref = isConnected ? connectedWorkdir : workdir;
    return ref === node.workdir;
  };

  const handleSelectNode = (node: VirtualNodeInfo) => {
    if (isConnected) return; // read-only while connected
    setWorkdir(node.workdir);
    setConfig({ sandbox: node.sandbox });
  };

  if (nodeList.length === 0) {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center', gap: '12px',
        color: 'var(--text3)', padding: '40px',
      }}>
        <span style={{ fontSize: '40px' }}>🌐</span>
        <p style={{ fontSize: '14px', fontWeight: '500' }}>暂无节点信息</p>
        <p style={{ fontSize: '12px', textAlign: 'center', lineHeight: 1.6 }}>
          连接到服务器后，若服务器配置了虚拟节点，<br />节点列表将自动填充到这里。
        </p>
      </div>
    );
  }

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
      <div style={{ marginBottom: '16px' }}>
        <h2 style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', marginBottom: '4px' }}>
          🌐 节点列表
        </h2>
        {isConnected ? (
          <p style={{ fontSize: '12px', color: 'var(--text3)' }}>
            当前会话已连接，节点只读。断开后可点击预选下次连接的节点。
          </p>
        ) : (
          <p style={{ fontSize: '12px', color: 'var(--text3)' }}>
            点击节点预选工作目录，下次连接时生效。
          </p>
        )}
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
        {nodeList.map((node) => {
          const active = isActive(node);
          return (
            <div
              key={node.name}
              onClick={() => handleSelectNode(node)}
              style={{
                padding: '14px 16px',
                background: active ? 'var(--accent-glow)' : 'var(--bg2)',
                border: active
                  ? '1px solid rgba(99,102,241,0.5)'
                  : '1px solid var(--border)',
                borderRadius: '10px',
                cursor: isConnected ? 'default' : 'pointer',
                opacity: isConnected && !active ? 0.65 : 1,
                transition: 'all 0.15s',
              }}
              onMouseOver={(e) => { if (!isConnected && !active) e.currentTarget.style.background = 'var(--bg3)'; }}
              onMouseOut={(e) => { if (!isConnected && !active) e.currentTarget.style.background = 'var(--bg2)'; }}
            >
              {/* Header row */}
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
                <span style={{ fontSize: '16px' }}>{node.sandbox ? '🔒' : '📂'}</span>
                <span style={{ fontSize: '13px', fontWeight: '600', color: active ? 'var(--accent)' : 'var(--text)' }}>
                  {node.name}
                </span>
                <div style={{ marginLeft: 'auto', display: 'flex', gap: '4px' }}>
                  {active && isConnected && (
                    <span style={{
                      fontSize: '10px', fontWeight: '600',
                      color: 'var(--green)',
                      background: 'var(--green-dim)',
                      border: '1px solid rgba(16,185,129,0.3)',
                      borderRadius: '4px',
                      padding: '1px 6px',
                    }}>已连接</span>
                  )}
                  {active && !isConnected && (
                    <span style={{
                      fontSize: '10px', fontWeight: '600',
                      color: 'var(--accent)',
                      background: 'var(--accent-glow)',
                      border: '1px solid rgba(99,102,241,0.4)',
                      borderRadius: '4px',
                      padding: '1px 6px',
                    }}>已预选</span>
                  )}
                  {node.sandbox && (
                    <span style={{
                      fontSize: '10px', color: 'var(--yellow)',
                      background: 'var(--yellow-dim)',
                      border: '1px solid rgba(245,158,11,0.3)',
                      borderRadius: '4px',
                      padding: '1px 6px',
                    }}>沙盒</span>
                  )}
                </div>
              </div>

              {/* Workdir */}
              <p style={{
                fontSize: '11px', fontFamily: 'monospace',
                color: 'var(--text3)', marginBottom: node.description || node.tags.length > 0 ? '6px' : 0,
                wordBreak: 'break-all',
              }}>
                {node.workdir}
              </p>

              {/* Description */}
              {node.description && (
                <p style={{ fontSize: '11px', color: 'var(--text2)', marginBottom: node.tags.length > 0 ? '6px' : 0 }}>
                  {node.description}
                </p>
              )}

              {/* Tags */}
              {node.tags.length > 0 && (
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px' }}>
                  {node.tags.map(tag => (
                    <span key={tag} style={{
                      fontSize: '10px',
                      padding: '1px 7px',
                      borderRadius: '10px',
                      background: 'var(--bg3)',
                      color: 'var(--text2)',
                      border: '1px solid var(--border)',
                    }}>{tag}</span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};
