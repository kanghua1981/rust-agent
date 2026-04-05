import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { VirtualNodeInfo, ConfigPreset, ConnectionHistory } from '../types/agent';

interface Props {
  onConnect: () => void;
  onClose: () => void;
}

type Tab = 'manual' | 'presets' | 'history';

// 缩写服务器地址函数
function abbreviateServerUrl(url: string): string {
  try {
    const u = new URL(url);
    return `${u.hostname}${u.port ? ':' + u.port : ''}`;
  } catch {
    return url.length > 25 ? url.substring(0, 22) + '...' : url;
  }
}

// 格式化时间
function formatTime(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp;
  
  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return `${Math.floor(diff / 60000)}分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}小时前`;
  if (diff < 604800000) return `${Math.floor(diff / 86400000)}天前`;
  
  const date = new Date(timestamp);
  return `${date.getMonth() + 1}/${date.getDate()}`;
}

export const ConnectModal: React.FC<Props> = ({ onConnect, onClose }) => {
  const { 
    serverUrl, 
    setServerUrl, 
    workdir, 
    setWorkdir, 
    config, 
    setConfig, 
    nodeList, 
    clusterToken, 
    setClusterToken,
    presets,
    connectionHistory,
    applyPreset,
    removeConnectionHistory
  } = useAgentStore();
  
  const [activeTab, setActiveTab] = useState<Tab>('manual');
  const [url, setUrl] = useState(serverUrl);
  const [dir, setDir] = useState(workdir ?? '');
  const [token, setToken] = useState(clusterToken);
  const [selectedNode, setSelectedNode] = useState<string>('');
  const [selectedPreset, setSelectedPreset] = useState<string>('');
  const [selectedHistory, setSelectedHistory] = useState<string>('');

  // 当标签页切换时，重置选择
  useEffect(() => {
    setSelectedPreset('');
    setSelectedHistory('');
  }, [activeTab]);

  // 当选择预设时，更新表单
  useEffect(() => {
    if (selectedPreset && activeTab === 'presets') {
      const preset = presets.find(p => p.id === selectedPreset);
      if (preset) {
        setUrl(preset.serverUrl);
        setDir(preset.workdir || '');
        setConfig({ 
          autoApprove: preset.autoApprove,
          agentMode: preset.agentMode,
          isolation: preset.isolation || 'container'
        });
      }
    }
  }, [selectedPreset, activeTab, presets, setConfig]);

  // 当选择历史记录时，更新表单
  useEffect(() => {
    if (selectedHistory && activeTab === 'history') {
      const history = connectionHistory.find(h => h.id === selectedHistory);
      if (history) {
        setUrl(history.serverUrl);
        setDir(history.workdir || '');
      }
    }
  }, [selectedHistory, activeTab, connectionHistory]);

  const handleNodeSelect = (nodeName: string) => {
    setSelectedNode(nodeName);
    if (nodeName === '') return;
    const node: VirtualNodeInfo | undefined = nodeList.find(n => n.name === nodeName);
    if (node) {
      setDir(node.workdir);
      // prefer new isolation field; fall back to legacy sandbox boolean
      const iso = node.isolation ?? (node.sandbox ? 'sandbox' : 'container');
      setConfig({ isolation: iso });
    }
  };

  const handleConnect = () => {
    setServerUrl(url.trim() || 'ws://localhost:9527');
    if (dir.trim()) setWorkdir(dir.trim());
    setClusterToken(token.trim());
    onConnect();
    onClose();
  };

  const handleQuickConnect = (presetId: string) => {
    applyPreset(presetId);
    onConnect();
    onClose();
  };

  const handleHistoryConnect = (historyId: string) => {
    const history = connectionHistory.find(h => h.id === historyId);
    if (history) {
      setServerUrl(history.serverUrl);
      if (history.workdir) setWorkdir(history.workdir);
      onConnect();
      onClose();
    }
  };

  const handleRemoveHistory = (e: React.MouseEvent, historyId: string) => {
    e.stopPropagation();
    removeConnectionHistory(historyId);
    if (selectedHistory === historyId) {
      setSelectedHistory('');
    }
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
        width: '500px',
        maxWidth: '90vw',
        maxHeight: '80vh',
        boxShadow: 'var(--shadow)',
        display: 'flex',
        flexDirection: 'column',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '20px', flexShrink: 0 }}>
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

        {/* 标签页导航 */}
        <div style={{ 
          display: 'flex', 
          borderBottom: '1px solid var(--border)', 
          marginBottom: '20px',
          flexShrink: 0
        }}>
          <button
            onClick={() => setActiveTab('manual')}
            style={{
              padding: '8px 16px',
              background: activeTab === 'manual' ? 'var(--accent)' : 'transparent',
              color: activeTab === 'manual' ? '#fff' : 'var(--text2)',
              border: 'none',
              borderBottom: activeTab === 'manual' ? '2px solid var(--accent)' : '2px solid transparent',
              fontSize: '13px',
              fontWeight: '500',
              cursor: 'pointer',
              flex: 1,
              textAlign: 'center',
            }}
          >
            手动连接
          </button>
          <button
            onClick={() => setActiveTab('presets')}
            style={{
              padding: '8px 16px',
              background: activeTab === 'presets' ? 'var(--accent)' : 'transparent',
              color: activeTab === 'presets' ? '#fff' : 'var(--text2)',
              border: 'none',
              borderBottom: activeTab === 'presets' ? '2px solid var(--accent)' : '2px solid transparent',
              fontSize: '13px',
              fontWeight: '500',
              cursor: 'pointer',
              flex: 1,
              textAlign: 'center',
            }}
          >
            预设配置 ({presets.length})
          </button>
          <button
            onClick={() => setActiveTab('history')}
            style={{
              padding: '8px 16px',
              background: activeTab === 'history' ? 'var(--accent)' : 'transparent',
              color: activeTab === 'history' ? '#fff' : 'var(--text2)',
              border: 'none',
              borderBottom: activeTab === 'history' ? '2px solid var(--accent)' : '2px solid transparent',
              fontSize: '13px',
              fontWeight: '500',
              cursor: 'pointer',
              flex: 1,
              textAlign: 'center',
            }}
          >
            连接历史 ({connectionHistory.length})
          </button>
        </div>

        <div style={{ flex: 1, overflowY: 'auto', marginBottom: '20px' }}>
          {/* 手动连接标签页 */}
          {activeTab === 'manual' && (
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
                      width: '100%', padding: '9px 30px 9px 12px',
                      background: `var(--bg3) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%239499b0' d='M6 8L1 3h10z'/%3E%3C/svg%3E") no-repeat right 10px center`,
                      border: '1px solid var(--border)',
                      borderRadius: '8px', color: 'var(--text)',
                      outline: 'none', fontSize: '13px', cursor: 'pointer',
                      appearance: 'none', WebkitAppearance: 'none',
                    }}
                  >
                    <option value=''>── 物理默认（自定义工作目录）──</option>
                    {nodeList.map(n => (
                      <option key={n.name} value={n.name}>
                        {n.name}
                        {n.tags.length > 0 ? `  [${n.tags.join(', ')}]` : ''}
                        {(n.isolation === 'sandbox' || (!n.isolation && n.sandbox)) ? '  🔒' : n.isolation === 'normal' ? '  🔓' : '  🔲'}
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

              <div>
                <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '6px' }}>
                  隔离模式
                </label>
                <select
                  value={config.isolation ?? 'container'}
                  onChange={(e) => setConfig({ isolation: e.target.value as 'normal' | 'container' | 'sandbox' })}
                  style={{
                    width: '100%', padding: '9px 30px 9px 12px',
                    background: `var(--bg3) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%239499b0' d='M6 8L1 3h10z'/%3E%3C/svg%3E") no-repeat right 10px center`,
                    border: '1px solid var(--border)',
                    borderRadius: '8px', color: 'var(--text)',
                    outline: 'none', fontSize: '13px', cursor: 'pointer',
                    appearance: 'none', WebkitAppearance: 'none',
                  }}
                >
                  <option value="normal">🕑3 直接运行（无容器，完全兼容）</option>
                  <option value="container">🔲 容器模式（namespace 隔离，默认）</option>
                  <option value="sandbox">🔒 沙筱模式（overlayfs 保护，支持回滚）</option>
                </select>
                <p style={{ fontSize: '11px', color: 'var(--text3)', marginTop: '4px' }}>
                  {(config.isolation ?? 'container') === 'normal' && '直接在宿主运行，工具可访问全部路径'}
                  {(config.isolation ?? 'container') === 'container' && '进程视图隔离，写操作直接落到项目文件'}
                  {(config.isolation ?? 'container') === 'sandbox' && '写操作落到 tmpfs upper 层，支持 /rollback 和 /commit'}
                </p>
              </div>
            </div>
          )}

          {/* 预设配置标签页 */}
          {activeTab === 'presets' && (
            <div>
              {presets.length === 0 ? (
                <div style={{ 
                  textAlign: 'center', 
                  padding: '40px 20px',
                  color: 'var(--text3)',
                  fontSize: '13px'
                }}>
                  <p>暂无预设配置</p>
                  <p style={{ fontSize: '12px', marginTop: '8px' }}>请在设置页面创建预设配置</p>
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                  {presets.map(preset => (
                    <div
                      key={preset.id}
                      onClick={() => setSelectedPreset(preset.id)}
                      style={{
                        padding: '12px',
                        background: selectedPreset === preset.id ? 'var(--accent-glow)' : 'var(--bg3)',
                        border: selectedPreset === preset.id ? '1px solid rgba(99,102,241,0.3)' : '1px solid var(--border)',
                        borderRadius: '8px',
                        cursor: 'pointer',
                        transition: 'all 0.15s',
                      }}
                      onMouseOver={(e) => { if (selectedPreset !== preset.id) e.currentTarget.style.background = 'var(--bg4)'; }}
                      onMouseOut={(e) => { if (selectedPreset !== preset.id) e.currentTarget.style.background = 'var(--bg3)'; }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '4px' }}>
                        <span style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text)' }}>{preset.name}</span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleQuickConnect(preset.id);
                          }}
                          style={{
                            padding: '4px 12px',
                            background: 'var(--accent)',
                            color: '#fff',
                            borderRadius: '6px',
                            fontSize: '11px',
                            fontWeight: '500',
                            border: 'none',
                            cursor: 'pointer',
                          }}
                        >
                          快速连接
                        </button>
                      </div>
                      <div style={{ fontSize: '11px', color: 'var(--text2)', fontFamily: 'monospace', marginBottom: '2px' }}>
                        {abbreviateServerUrl(preset.serverUrl)}
                      </div>
                      {preset.workdir && (
                        <div style={{ fontSize: '11px', color: 'var(--text3)' }}>
                          📂 {preset.workdir}
                        </div>
                      )}
                      <div style={{ display: 'flex', gap: '8px', marginTop: '6px', fontSize: '10px', color: 'var(--text3)' }}>
                        <span>自动确认: {preset.autoApprove ? '是' : '否'}</span>
                        <span>模式: {preset.agentMode === 'auto' ? '自动' : preset.agentMode === 'simple' ? '单层' : preset.agentMode === 'plan' ? '计划' : '流水线'}</span>
                        <span>隔离: {preset.isolation === 'normal' ? '无容器' : preset.isolation === 'sandbox' ? '沙盒' : '容器'}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* 连接历史标签页 */}
          {activeTab === 'history' && (
            <div>
              {connectionHistory.length === 0 ? (
                <div style={{ 
                  textAlign: 'center', 
                  padding: '40px 20px',
                  color: 'var(--text3)',
                  fontSize: '13px'
                }}>
                  <p>暂无连接历史</p>
                  <p style={{ fontSize: '12px', marginTop: '8px' }}>连接成功后会自动记录历史</p>
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                  {connectionHistory.map(history => (
                    <div
                      key={history.id}
                      onClick={() => setSelectedHistory(history.id)}
                      style={{
                        padding: '12px',
                        background: selectedHistory === history.id ? 'var(--accent-glow)' : 'var(--bg3)',
                        border: selectedHistory === history.id ? '1px solid rgba(99,102,241,0.3)' : '1px solid var(--border)',
                        borderRadius: '8px',
                        cursor: 'pointer',
                        transition: 'all 0.15s',
                        position: 'relative',
                      }}
                      onMouseOver={(e) => { if (selectedHistory !== history.id) e.currentTarget.style.background = 'var(--bg4)'; }}
                      onMouseOut={(e) => { if (selectedHistory !== history.id) e.currentTarget.style.background = 'var(--bg3)'; }}
                    >
                      <button
                        onClick={(e) => handleRemoveHistory(e, history.id)}
                        style={{
                          position: 'absolute',
                          top: '8px',
                          right: '8px',
                          width: '20px',
                          height: '20px',
                          background: 'var(--bg4)',
                          color: 'var(--text3)',
                          border: '1px solid var(--border)',
                          borderRadius: '4px',
                          fontSize: '12px',
                          cursor: 'pointer',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                        }}
                        title="删除记录"
                      >
                        ×
                      </button>
                      
                      <div style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text)', marginBottom: '4px' }}>
                        {abbreviateServerUrl(history.serverUrl)}
                      </div>
                      
                      {history.workdir && (
                        <div style={{ fontSize: '11px', color: 'var(--text2)', marginBottom: '4px' }}>
                          📂 {history.workdir}
                        </div>
                      )}
                      
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '8px' }}>
                        <div style={{ fontSize: '10px', color: 'var(--text3)' }}>
                          连接次数: {history.connectionCount}次
                        </div>
                        <div style={{ fontSize: '10px', color: 'var(--text3)' }}>
                          最后连接: {formatTime(history.lastConnectedAt)}
                        </div>
                      </div>
                      
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          handleHistoryConnect(history.id);
                        }}
                        style={{
                          width: '100%',
                          padding: '6px',
                          background: 'var(--accent)',
                          color: '#fff',
                          borderRadius: '6px',
                          fontSize: '11px',
                          fontWeight: '500',
                          border: 'none',
                          cursor: 'pointer',
                          marginTop: '8px',
                        }}
                      >
                        连接
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* 底部按钮 */}
        <div style={{ display: 'flex', gap: '10px', marginTop: 'auto', flexShrink: 0 }}>
          <button
            onClick={onClose}
            style={{
              flex: 1, padding: '9px',
              background: 'var(--bg3)', color: 'var(--text2)',
              borderRadius: '8px', fontWeight: '500', fontSize: '13px',
              border: '1px solid var(--border)',
            }}
          >
            取消
          </button>
          {activeTab === 'manual' && (
            <button
              onClick={handleConnect}
              style={{
                flex: 2, padding: '9px',
                background: 'var(--accent)', color: '#fff',
                borderRadius: '8px', fontWeight: '600', fontSize: '13px',
              }}
            >
              连接
            </button>
          )}
          {activeTab === 'presets' && selectedPreset && (
            <button
              onClick={() => handleQuickConnect(selectedPreset)}
              style={{
                flex: 2, padding: '9px',
                background: 'var(--accent)', color: '#fff',
                borderRadius: '8px', fontWeight: '600', fontSize: '13px',
              }}
            >
              使用此预设连接
            </button>
          )}
          {activeTab === 'history' && selectedHistory && (
            <button
              onClick={() => handleHistoryConnect(selectedHistory)}
              style={{
                flex: 2, padding: '9px',
                background: 'var(--accent)', color: '#fff',
                borderRadius: '8px', fontWeight: '600', fontSize: '13px',
              }}
            >
              使用此历史连接
            </button>
          )}
          {(activeTab === 'presets' && !selectedPreset) || (activeTab === 'history' && !selectedHistory) ? (
            <button
              onClick={() => setActiveTab('manual')}
              style={{
                flex: 2, padding: '9px',
                background: 'var(--bg3)', color: 'var(--text2)',
                borderRadius: '8px', fontWeight: '500', fontSize: '13px',
                border: '1px solid var(--border)',
              }}
            >
              切换到手动连接
            </button>
          ) : null}
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