import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { VirtualNodeInfo } from '../types/agent';

interface Props {
  onConnect: () => void;
  onClose: () => void;
}

export const ConnectModal: React.FC<Props> = ({ onConnect, onClose }) => {
  const { serverUrl, setServerUrl, workdir, setWorkdir, config, setConfig, nodeList, clusterToken, setClusterToken } = useAgentStore();
  const [url, setUrl] = useState(serverUrl);
  const [dir, setDir] = useState(workdir ?? '');
  const [token, setToken] = useState(clusterToken);
  const [selectedNode, setSelectedNode] = useState<string>('');

  const handleNodeSelect = (nodeName: string) => {
    setSelectedNode(nodeName);
    if (nodeName === '') return;
    const node: VirtualNodeInfo | undefined = nodeList.find(n => n.name === nodeName);
    if (node) {
      setDir(node.workdir);
      setConfig({ sandbox: node.sandbox });
    }
  };

  const handleConnect = () => {
    setServerUrl(url.trim() || 'ws://localhost:9527');
    if (dir.trim()) setWorkdir(dir.trim());
    setClusterToken(token.trim());
    onConnect();
    onClose();
  };

  return (
    <div style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      zIndex: 1000, backdropFilter: 'blur(4px)',
    }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div style={{
        background: 'var(--bg2)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius-lg)',
        padding: '24px',
        width: '440px',
        maxWidth: '90vw',
        boxShadow: 'var(--shadow)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '20px' }}>
          <div style={{
            width: '32px', height: '32px',
            background: 'linear-gradient(135deg, var(--accent), #8b5cf6)',
            borderRadius: '8px',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            fontSize: '16px',
          }}>🤖</div>
          <h2 style={{ fontSize: '16px', fontWeight: '600', color: 'var(--text)' }}>连接到 Rust Agent</h2>
          <button
            onClick={onClose}
            style={{ marginLeft: 'auto', color: 'var(--text3)', fontSize: '18px', lineHeight: 1, padding: '2px 6px', borderRadius: '4px' }}
          >×</button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
          <div>
            <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '6px' }}>
              WebSocket 地址
            </label>
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleConnect()}
              placeholder="ws://localhost:9527"
              style={{
                width: '100%', padding: '9px 12px',
                background: 'var(--bg3)', border: '1px solid var(--border)',
                borderRadius: '8px', color: 'var(--text)',
                outline: 'none', fontFamily: 'monospace', fontSize: '13px',
              }}
            />
          </div>

          <div>
            <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '6px' }}>
              集群 Token（可选，服务器开启认证时需要）
            </label>
            <input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleConnect()}
              placeholder="无 token 则留空"
              style={{
                width: '100%', padding: '9px 12px',
                background: 'var(--bg3)', border: '1px solid var(--border)',
                borderRadius: '8px', color: 'var(--text)',
                outline: 'none', fontFamily: 'monospace', fontSize: '13px',
              }}
            />
          </div>

          {/* Node selector — only shown when nodeList is populated from last connection */}
          {nodeList.length > 0 && (
            <div>
              <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '6px' }}>
                节点（来自上次连接）
              </label>
              <select
                value={selectedNode}
                onChange={(e) => handleNodeSelect(e.target.value)}
                style={{
                  width: '100%', padding: '9px 12px',
                  background: 'var(--bg3)', border: '1px solid var(--border)',
                  borderRadius: '8px', color: 'var(--text)',
                  outline: 'none', fontSize: '13px', cursor: 'pointer',
                }}
              >
                <option value=''>── 物理默认（自定义工作目录）──</option>
                {nodeList.map(n => (
                  <option key={n.name} value={n.name}>
                    {n.name}
                    {n.tags.length > 0 ? `  [${n.tags.join(', ')}]` : ''}
                    {n.sandbox ? '  🔒' : ''}
                  </option>
                ))}
              </select>
              {selectedNode && (() => {
                const node = nodeList.find(n => n.name === selectedNode);
                return node?.description ? (
                  <p style={{ fontSize: '11px', color: 'var(--text3)', marginTop: '4px' }}>{node.description}</p>
                ) : null;
              })()}
            </div>
          )}

          <div>
            <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '6px' }}>
              工作目录（可选）
            </label>
            <input
              value={dir}
              onChange={(e) => { setDir(e.target.value); setSelectedNode(''); }}
              placeholder="/path/to/project"
              style={{
                width: '100%', padding: '9px 12px',
                background: 'var(--bg3)', border: '1px solid var(--border)',
                borderRadius: '8px', color: 'var(--text)',
                outline: 'none', fontFamily: 'monospace', fontSize: '13px',
              }}
            />
          </div>

          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={!!config.autoApprove}
              onChange={(e) => setConfig({ autoApprove: e.target.checked })}
              style={{ accentColor: 'var(--accent)', cursor: 'pointer', width: '14px', height: '14px' }}
            />
            <span style={{ fontSize: '13px', color: 'var(--text2)' }}>自动确认工具调用（跳过每次确认弹窗）</span>
          </label>

          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={!!config.sandbox}
              onChange={(e) => setConfig({ sandbox: e.target.checked })}
              style={{ accentColor: 'var(--accent)', cursor: 'pointer', width: '14px', height: '14px' }}
            />
            <div>
              <span style={{ fontSize: '13px', color: 'var(--text2)' }}>启用沙盒模式</span>
              <p style={{ fontSize: '11px', color: 'var(--text3)', marginTop: '2px' }}>
                在隔离环境中执行文件操作，支持回滚和提交（需服务器支持 overlay）
              </p>
            </div>
          </label>
        </div>

        <div style={{ display: 'flex', gap: '10px', marginTop: '20px' }}>
          <button
            onClick={onClose}
            style={{
              flex: 1, padding: '9px',
              background: 'var(--bg3)', color: 'var(--text2)',
              borderRadius: '8px', fontWeight: '500', fontSize: '13px',
              border: '1px solid var(--border)',
            }}
          >取消</button>
          <button
            onClick={handleConnect}
            style={{
              flex: 2, padding: '9px',
              background: 'var(--accent)', color: '#fff',
              borderRadius: '8px', fontWeight: '600', fontSize: '13px',
            }}
          >连接</button>
        </div>

        <div style={{ marginTop: '16px', padding: '10px 12px', background: 'var(--bg3)', borderRadius: '8px', border: '1px solid var(--border)' }}>
          <p style={{ fontSize: '11px', color: 'var(--text3)', lineHeight: '1.6' }}>
            启动服务器：<code style={{ color: 'var(--accent)', background: 'none', border: 'none', padding: 0 }}>cargo run --release -- --mode server</code>
          </p>
        </div>
      </div>
    </div>
  );
};
