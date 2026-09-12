import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { VirtualNodeInfo } from '../types/agent';

interface NodesPanelProps {
  isConnected: boolean;
  onListNodes: () => void;
  onAddNode: (node: any) => void;
  onUpdateNode: (node: any) => void;
  onDeleteNode: (id: string) => void;
  onListPeers: () => void;
  onAddPeer: (peer: any) => void;
  onUpdatePeer: (peer: any) => void;
  onDeletePeer: (id: string) => void;
}

const EMPTY_NODE_FORM = {
  id: '',
  name: '',
  workdir: '',
  description: '',
  isolation: 'container' as 'normal' | 'container' | 'sandbox',
  sandbox: false,
  exec_mode: 'auto' as string,
  tags: '',
  createdAt: '',
};

const EMPTY_PEER_FORM = {
  id: '',
  name: '',
  url: '',
  token: '',
  tags: '',
  enabled: true,
  createdAt: '',
};

export const NodesPanel: React.FC<NodesPanelProps> = ({
  isConnected, onListNodes, onAddNode, onUpdateNode, onDeleteNode,
  onListPeers, onAddPeer, onUpdatePeer, onDeletePeer,
}) => {
  const { nodeList, peerList, workdir, setWorkdir, setConfig, connectedWorkdir } = useAgentStore();

  const [activeTab, setActiveTab] = useState<'nodes' | 'peers'>('nodes');

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState({ ...EMPTY_NODE_FORM });
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  // Peer state
  const [showPeerForm, setShowPeerForm] = useState(false);
  const [editingPeerId, setEditingPeerId] = useState<string | null>(null);
  const [peerForm, setPeerForm] = useState({ ...EMPTY_PEER_FORM, id: `peer_${Date.now()}` });
  const [deletePeerConfirm, setDeletePeerConfirm] = useState<string | null>(null);

  // Fetch node list when panel opens or connection changes
  useEffect(() => {
    if (isConnected) {
      onListNodes();
    }
  }, [isConnected, onListNodes]);

  // When connected: highlight the node matching the actual server-reported workdir.
  // When disconnected: highlight the node matching the pre-selected workdir.
  const isActive = (node: VirtualNodeInfo) => {
    const ref = isConnected ? connectedWorkdir : workdir;
    return ref === node.workdir;
  };

  const handleSelectNode = (node: VirtualNodeInfo) => {
    if (isConnected) return; // read-only while connected
    setWorkdir(node.workdir);
    const iso = node.isolation ?? (node.sandbox ? 'sandbox' : 'container');
    setConfig({ isolation: iso });
  };

  const openAddForm = () => {
    setEditingId(null);
    setForm({
      ...EMPTY_NODE_FORM,
      id: `node_${Date.now()}`,
    });
    setShowForm(true);
  };

  const openEditForm = (node: VirtualNodeInfo) => {
    if (isConnected) return;
    setEditingId(node.id);
    setForm({
      id: node.id,
      name: node.name,
      workdir: node.workdir,
      description: node.description || '',
      isolation: node.isolation || (node.sandbox ? 'sandbox' : 'container'),
      sandbox: node.sandbox,
      exec_mode: node.exec_mode || 'auto',
      tags: (node.tags || []).join(', '),
      createdAt: node.createdAt || '',
    });
    setShowForm(true);
  };

  const handleSave = () => {
    if (!form.name.trim() || !form.workdir.trim()) return;

    const now = new Date().toISOString();
    const nodeData: any = {
      id: form.id,
      name: form.name.trim(),
      workdir: form.workdir.trim(),
      description: form.description.trim(),
      isolation: form.isolation === 'container' ? null : form.isolation,  // "container" is default, send null
      sandbox: form.isolation === 'sandbox',
      execMode: form.exec_mode === 'auto' ? null : form.exec_mode,
      tags: form.tags.split(',').map(t => t.trim()).filter(t => t),
      createdAt: form.createdAt || now,  // preserve original createdAt on edit
      updatedAt: now,
    };

    if (editingId) {
      onUpdateNode(nodeData);
    } else {
      onAddNode(nodeData);
    }

    setShowForm(false);
    setEditingId(null);
  };

  const handleDelete = (id: string) => {
    onDeleteNode(id);
    setDeleteConfirm(null);
  };

  // ── Peer helpers ──
  const openAddPeer = () => {
    setEditingPeerId(null);
    setPeerForm({ ...EMPTY_PEER_FORM, id: `peer_${Date.now()}` });
    setShowPeerForm(true);
  };

  const openEditPeer = (peer: any) => {
    setEditingPeerId(peer.id);
    setPeerForm({
      id: peer.id,
      name: peer.name,
      url: peer.url,
      token: peer.token || '',
      tags: (peer.tags || []).join(', '),
      enabled: peer.enabled !== false,
      createdAt: peer.createdAt || '',
    });
    setShowPeerForm(true);
  };

  const handleSavePeer = () => {
    if (!peerForm.name.trim() || !peerForm.url.trim()) return;
    const now = new Date().toISOString();
    const peerData: any = {
      id: peerForm.id,
      name: peerForm.name.trim(),
      url: peerForm.url.trim(),
      token: peerForm.token.trim() || null,
      tags: peerForm.tags.split(',').map((t: string) => t.trim()).filter((t: string) => t),
      enabled: peerForm.enabled,
      createdAt: peerForm.createdAt || now,
      updatedAt: now,
    };
    if (editingPeerId) {
      onUpdatePeer(peerData);
    } else {
      onAddPeer(peerData);
    }
    setShowPeerForm(false);
    setEditingPeerId(null);
  };

  // ── Render helpers ──

  const iconForIsolation = (iso?: string, sandbox?: boolean) => {
    if (iso === 'sandbox' || (!iso && sandbox)) return '🔒';
    if (iso === 'normal') return '🔓';
    return '📂';
  };

  const isoLabel = (iso?: string, sandbox?: boolean) => {
    if (iso === 'sandbox' || (!iso && sandbox)) return '沙盒';
    if (iso === 'normal') return '无容器';
    return '容器';
  };

  const isoTone = (iso?: string, sandbox?: boolean) => {
    if (iso === 'sandbox' || (!iso && sandbox)) return 'warn';
    if (iso === 'normal') return 'muted';
    return 'info';
  };

  // NOTE: No more early return for empty nodeList — tabs must always be visible

  return (
    <div className="panel">
      {/* Tabs */}
      <div className="subtabs">
        {(['nodes', 'peers'] as const).map(tab => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`subtab${activeTab === tab ? ' active' : ''}`}
          >
            {tab === 'nodes' ? '🌐 节点' : '📡 Peers'}
          </button>
        ))}
      </div>

      {activeTab === 'peers' ? (
        /* ── Peers tab ── */
        <div className="panel">
          <div className="panel-head">
            <div>
              <h2 className="panel-head-title">📡 远程节点发现</h2>
              <p className="panel-head-sub">
                {isConnected
                  ? '配置远程 Agent 服务器，自动发现其上的虚拟节点。'
                  : '请先连接到服务器以管理 Peers。'}
              </p>
            </div>
            <div className="panel-actions">
              {isConnected && (
                <button className="btn-secondary" onClick={onListPeers}>🔄 刷新</button>
              )}
              {isConnected && (
                <button className="btn-primary" onClick={openAddPeer}>+ 添加 Peer</button>
              )}
            </div>
          </div>

          {showPeerForm && (
            <div className="form-card">
              <h3 className="form-title">{editingPeerId ? '✏️ 编辑 Peer' : '➕ 新建 Peer'}</h3>
              <div className="form-grid">
                <div>
                  <label className="form-label">名称 *</label>
                  <input className="field" placeholder="如: gpu-server" value={peerForm.name}
                    onChange={e => setPeerForm({ ...peerForm, name: e.target.value })} />
                </div>
                <div>
                  <label className="form-label">WebSocket URL *</label>
                  <input className="field" placeholder="ws://10.0.0.5:9527" value={peerForm.url}
                    onChange={e => setPeerForm({ ...peerForm, url: e.target.value })} />
                </div>
                <div>
                  <label className="form-label">Token (可选)</label>
                  <input className="field" placeholder="peer 认证 token" value={peerForm.token}
                    onChange={e => setPeerForm({ ...peerForm, token: e.target.value })} />
                </div>
                <div>
                  <label className="form-label">标签 (逗号分隔)</label>
                  <input className="field" placeholder="gpu, large-ram" value={peerForm.tags}
                    onChange={e => setPeerForm({ ...peerForm, tags: e.target.value })} />
                </div>
                <div className="row" style={{ gap: 8 }}>
                  <input type="checkbox" checked={peerForm.enabled}
                    onChange={e => setPeerForm({ ...peerForm, enabled: e.target.checked })} />
                  <label className="form-label" style={{ marginBottom: 0 }}>启用 (禁用后停止探测)</label>
                </div>
              </div>
              <div className="form-actions start">
                <button className="btn-primary" onClick={handleSavePeer}>保存</button>
                <button className="btn-secondary" onClick={() => { setShowPeerForm(false); setEditingPeerId(null); }}>
                  取消
                </button>
              </div>
            </div>
          )}

          <div className="panel-body">
            {peerList.length === 0 ? (
              <div className="empty-state">
                <span className="empty-icon">📡</span>
                <p className="empty-title">暂无 Peer 配置</p>
                <p className="empty-sub">添加远程 Agent 服务器，自动发现对方节点。</p>
              </div>
            ) : (
              peerList.map((peer: any) => (
                <div key={peer.id} className="list-item">
                  <span style={{ fontSize: 18 }}>📡</span>
                  <div className="fill">
                    <div className="row" style={{ gap: 8, marginBottom: 2 }}>
                      <span className="item-title">{peer.name}</span>
                      {!peer.enabled && <span className="tag">已禁用</span>}
                    </div>
                    <div className="item-mono" style={{ marginBottom: 4 }}>{peer.url}</div>
                    {(peer.tags || []).length > 0 && (
                      <div className="tag-row">
                        {(peer.tags || []).map((t: string) => (
                          <span key={t} className="tag accent">{t}</span>
                        ))}
                      </div>
                    )}
                  </div>
                  {isConnected && (
                    <div className="row" style={{ gap: 4 }}>
                      <button className="icon-btn" onClick={() => openEditPeer(peer)} title="编辑 Peer">✏️</button>
                      <button className="icon-btn" onClick={() => setDeletePeerConfirm(peer.id)} title="删除 Peer">🗑️</button>
                    </div>
                  )}
                </div>
              ))
            )}
          </div>

          {deletePeerConfirm && (
            <div className="confirm-bar" style={{ margin: '12px 24px' }}>
              <span className="msg">确定要删除此 Peer 吗？</span>
              <div className="confirm-bar-actions">
                <button className="btn-danger" onClick={() => { onDeletePeer(deletePeerConfirm); setDeletePeerConfirm(null); }}>
                  删除
                </button>
                <button className="btn-secondary" onClick={() => setDeletePeerConfirm(null)}>取消</button>
              </div>
            </div>
          )}
        </div>
      ) : (
      /* ── Nodes tab ── */
      <div className="panel">
        <div className="panel-head">
          <div>
            <h2 className="panel-head-title">🌐 节点列表</h2>
            <p className="panel-head-sub">
              {isConnected
                ? '管理服务器端节点：可添加 / 编辑 / 删除节点。'
                : '请先连接到服务器以管理节点。'}
            </p>
          </div>
          <div className="panel-actions">
            {isConnected && (
              <button className="btn-secondary" onClick={onListNodes}>🔄 刷新</button>
            )}
            {isConnected && (
              <button className="btn-primary" onClick={openAddForm}>+ 添加节点</button>
            )}
          </div>
        </div>

        {showForm && (
          <div className="form-card">
            <h3 className="form-title">{editingId ? '✏️ 编辑节点' : '➕ 新建节点'}</h3>

            <div className="form-grid">
              <div>
                <label className="form-label">名称 *</label>
                <input className="field" placeholder="如: my-project" value={form.name}
                  onChange={e => setForm({ ...form, name: e.target.value })} />
              </div>
              <div>
                <label className="form-label">工作目录 *</label>
                <input className="field" placeholder="如: /home/user/projects/my-app" value={form.workdir}
                  onChange={e => setForm({ ...form, workdir: e.target.value })} />
              </div>
              <div className="span-2">
                <label className="form-label">描述</label>
                <input className="field" placeholder="可选的节点描述" value={form.description}
                  onChange={e => setForm({ ...form, description: e.target.value })} />
              </div>
              <div>
                <label className="form-label">隔离模式</label>
                <select className="field" value={form.isolation}
                  onChange={e => setForm({ ...form, isolation: e.target.value as any })}>
                  <option value="container">容器 (container)</option>
                  <option value="sandbox">沙盒 (sandbox)</option>
                  <option value="normal">无隔离 (normal)</option>
                </select>
              </div>
              <div>
                <label className="form-label">执行模式</label>
                <select className="field" value={form.exec_mode}
                  onChange={e => setForm({ ...form, exec_mode: e.target.value })}>
                  <option value="auto">自动</option>
                  <option value="simple">简单</option>
                  <option value="plan">计划</option>
                </select>
              </div>
              <div className="span-2">
                <label className="form-label">标签 (逗号分隔)</label>
                <input className="field" placeholder="如: frontend, react, critical" value={form.tags}
                  onChange={e => setForm({ ...form, tags: e.target.value })} />
              </div>
            </div>

            <div className="form-actions">
              <button className="btn-secondary" onClick={() => { setShowForm(false); setEditingId(null); }}>
                取消
              </button>
              <button className="btn-primary" onClick={handleSave}
                disabled={!form.name.trim() || !form.workdir.trim()}>
                {editingId ? '保存修改' : '创建节点'}
              </button>
            </div>
          </div>
        )}

        <div className="panel-body">
          {nodeList.length === 0 && !showForm ? (
            <div className="empty-state">
              <span className="empty-icon">🌐</span>
              <p className="empty-title">暂无节点信息</p>
              <p className="empty-sub">
                连接到服务器后，若服务器配置了虚拟节点，<br />节点列表将自动填充到这里。
              </p>
              {isConnected && (
                <button className="btn-ghost" style={{ marginTop: 8, color: 'var(--accent)' }} onClick={openAddForm}>
                  + 添加第一个节点
                </button>
              )}
            </div>
          ) : (
          <div className="list-stack">
            {nodeList.map((node) => {
              const active = isActive(node);
              return (
                <div key={node.id || node.name}
                  className={`node-card${!isConnected ? ' selectable' : ''}${active ? ' active' : ''}${isConnected && !active ? ' dimmed' : ''}`}
                  onClick={isConnected ? undefined : () => handleSelectNode(node)}
                >
                  <div className="node-card-head">
                    <span style={{ fontSize: 16 }}>{iconForIsolation(node.isolation, node.sandbox)}</span>
                    <span className={`node-card-name${active ? ' active' : ''}`}>{node.name}</span>
                    <div className="node-card-meta">
                      {active && isConnected && <span className="chip ok">已连接</span>}
                      {active && !isConnected && <span className="chip info">已预选</span>}
                      <span className={`chip ${isoTone(node.isolation, node.sandbox)}`}>
                        {isoLabel(node.isolation, node.sandbox)}
                      </span>

                      {isConnected && (
                        <>
                          <button className="icon-btn" onClick={(e) => { e.stopPropagation(); openEditForm(node); }} title="编辑节点">✏️</button>
                          <button className="icon-btn" onClick={(e) => { e.stopPropagation(); setDeleteConfirm(node.id); }} title="删除节点">🗑️</button>
                        </>
                      )}
                    </div>
                  </div>

                  <p className="item-mono" style={{ marginBottom: node.description || node.tags.length > 0 ? 6 : 0 }}>
                    {node.workdir}
                  </p>

                  {node.description && (
                    <p className="item-sub" style={{ marginBottom: node.tags.length > 0 ? 6 : 0 }}>
                      {node.description}
                    </p>
                  )}

                  {node.tags.length > 0 && (
                    <div className="tag-row">
                      {node.tags.map(tag => <span key={tag} className="tag">{tag}</span>)}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          )}
        </div>

        {deleteConfirm && (
          <div className="modal-backdrop">
            <div className="modal">
              <p className="modal-text">确定要删除此节点吗？此操作不可撤销。</p>
              <div className="modal-actions">
                <button className="btn-secondary" onClick={() => setDeleteConfirm(null)}>取消</button>
                <button className="btn-danger" onClick={() => handleDelete(deleteConfirm)}>删除</button>
              </div>
            </div>
          </div>
        )}
      </div>
      )}
    </div>
  );
};
