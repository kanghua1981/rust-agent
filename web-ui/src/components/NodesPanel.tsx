import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { VirtualNodeInfo } from '../types/agent';

interface NodesPanelProps {
  isConnected: boolean;
  onListNodes: () => void;
  onAddNode: (node: any) => void;
  onUpdateNode: (node: any) => void;
  onDeleteNode: (id: string) => void;
}

const EMPTY_FORM = {
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

export const NodesPanel: React.FC<NodesPanelProps> = ({
  isConnected, onListNodes, onAddNode, onUpdateNode, onDeleteNode,
}) => {
  const { nodeList, workdir, setWorkdir, setConfig, connectedWorkdir } = useAgentStore();

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState({ ...EMPTY_FORM });
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

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
      ...EMPTY_FORM,
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

  const isoBadgeStyle = (iso?: string, sandbox?: boolean): React.CSSProperties => {
    if (iso === 'sandbox' || (!iso && sandbox)) return {
      color: 'var(--yellow)',
      background: 'var(--yellow-dim)',
      border: '1px solid rgba(245,158,11,0.3)',
    };
    if (iso === 'normal') return {
      color: 'var(--text3)',
      background: 'var(--bg3)',
      border: '1px solid var(--border)',
    };
    return {
      color: 'var(--accent)',
      background: 'var(--accent-glow)',
      border: '1px solid rgba(99,102,241,0.3)',
    };
  };

  // ── CSS-in-JS helpers ──

  const btnStyle: React.CSSProperties = {
    padding: '5px 12px', fontSize: '11px', fontWeight: '600',
    border: 'none', borderRadius: '6px', cursor: 'pointer',
    transition: 'all 0.12s',
  };

  const inputStyle: React.CSSProperties = {
    width: '100%', padding: '8px 10px', fontSize: '12px',
    background: 'var(--bg2)', border: '1px solid var(--border)',
    borderRadius: '6px', color: 'var(--text)', outline: 'none',
    boxSizing: 'border-box',
  };

  const labelStyle: React.CSSProperties = {
    fontSize: '11px', fontWeight: '500', color: 'var(--text2)',
    marginBottom: '4px', display: 'block',
  };

  // ── Empty state ──

  if (nodeList.length === 0 && !showForm) {
    return (
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
        {/* Header */}
        <div style={{ padding: '20px 24px 0', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <h2 style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', margin: 0 }}>
              🌐 节点管理
            </h2>
            <p style={{ fontSize: '12px', color: 'var(--text3)', margin: '4px 0 0' }}>
              {isConnected ? '管理服务器端节点配置' : '管理节点，连接到服务器后可用'}
            </p>
          </div>
          {isConnected && (
            <button
              onClick={openAddForm}
              style={{ ...btnStyle, background: 'var(--accent)', color: '#fff', fontSize: '13px', padding: '6px 16px' }}
            >+ 添加节点</button>
          )}
        </div>

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
          {isConnected && (
            <button
              onClick={openAddForm}
              style={{ ...btnStyle, background: 'var(--bg3)', color: 'var(--accent)', border: '1px solid var(--border)', marginTop: '8px' }}
            >+ 添加第一个节点</button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{ padding: '20px 24px 0', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', margin: 0 }}>
            🌐 节点列表
          </h2>
          <p style={{ fontSize: '12px', color: 'var(--text3)', margin: '4px 0 0' }}>
            {isConnected
              ? '管理服务器端节点：可添加 / 编辑 / 删除节点。'
              : '请先连接到服务器以管理节点。'}
          </p>
        </div>
        <div style={{ display: 'flex', gap: '8px' }}>
          {isConnected && (
            <button
              onClick={onListNodes}
              style={{ ...btnStyle, background: 'var(--bg3)', color: 'var(--text2)', border: '1px solid var(--border)' }}
            >🔄 刷新</button>
          )}
          {isConnected && (
            <button
              onClick={openAddForm}
              style={{ ...btnStyle, background: 'var(--accent)', color: '#fff', fontSize: '13px', padding: '6px 16px' }}
            >+ 添加节点</button>
          )}
        </div>
      </div>

      {/* Form modal */}
      {showForm && (
        <div style={{
          margin: '16px 24px 0', padding: '16px',
          background: 'var(--bg2)', border: '1px solid var(--border)',
          borderRadius: '10px',
        }}>
          <h3 style={{ fontSize: '13px', fontWeight: '600', color: 'var(--text)', margin: '0 0 12px' }}>
            {editingId ? '✏️ 编辑节点' : '➕ 新建节点'}
          </h3>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px' }}>
            <div>
              <label style={labelStyle}>名称 *</label>
              <input style={inputStyle} placeholder="如: my-project" value={form.name}
                onChange={e => setForm({ ...form, name: e.target.value })} />
            </div>
            <div>
              <label style={labelStyle}>工作目录 *</label>
              <input style={inputStyle} placeholder="如: /home/user/projects/my-app" value={form.workdir}
                onChange={e => setForm({ ...form, workdir: e.target.value })} />
            </div>
            <div style={{ gridColumn: '1 / -1' }}>
              <label style={labelStyle}>描述</label>
              <input style={inputStyle} placeholder="可选的节点描述" value={form.description}
                onChange={e => setForm({ ...form, description: e.target.value })} />
            </div>
            <div>
              <label style={labelStyle}>隔离模式</label>
              <select style={inputStyle} value={form.isolation}
                onChange={e => setForm({ ...form, isolation: e.target.value as any })}>
                <option value="container">容器 (container)</option>
                <option value="sandbox">沙盒 (sandbox)</option>
                <option value="normal">无隔离 (normal)</option>
              </select>
            </div>
            <div>
              <label style={labelStyle}>执行模式</label>
              <select style={inputStyle} value={form.exec_mode}
                onChange={e => setForm({ ...form, exec_mode: e.target.value })}>
                <option value="auto">自动</option>
                <option value="simple">简单</option>
                <option value="plan">计划</option>
                <option value="pipeline">流水线</option>
              </select>
            </div>
            <div style={{ gridColumn: '1 / -1' }}>
              <label style={labelStyle}>标签 (逗号分隔)</label>
              <input style={inputStyle} placeholder="如: frontend, react, critical" value={form.tags}
                onChange={e => setForm({ ...form, tags: e.target.value })} />
            </div>
          </div>

          <div style={{ display: 'flex', gap: '8px', marginTop: '14px', justifyContent: 'flex-end' }}>
            <button onClick={() => { setShowForm(false); setEditingId(null); }}
              style={{ ...btnStyle, background: 'var(--bg3)', color: 'var(--text2)', border: '1px solid var(--border)' }}>
              取消
            </button>
            <button onClick={handleSave}
              style={{ ...btnStyle, background: 'var(--accent)', color: '#fff' }}
              disabled={!form.name.trim() || !form.workdir.trim()}>
              {editingId ? '保存修改' : '创建节点'}
            </button>
          </div>
        </div>
      )}

      {/* Node list */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '16px 24px 20px' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
          {nodeList.map((node) => {
            const active = isActive(node);
            return (
              <div key={node.id || node.name}
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
                  position: 'relative',
                }}
                onMouseOver={(e) => { if (!isConnected && !active) e.currentTarget.style.background = 'var(--bg3)'; }}
                onMouseOut={(e) => { if (!isConnected && !active) e.currentTarget.style.background = 'var(--bg2)'; }}
              >
                {/* Header row */}
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '6px' }}>
                  <span style={{ fontSize: '16px' }}>{iconForIsolation(node.isolation, node.sandbox)}</span>
                  <span style={{ fontSize: '13px', fontWeight: '600', color: active ? 'var(--accent)' : 'var(--text)' }}
                    onClick={(e) => { e.stopPropagation(); handleSelectNode(node); }}>
                    {node.name}
                  </span>
                  <div style={{ marginLeft: 'auto', display: 'flex', gap: '4px', alignItems: 'center' }}>
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
                    <span style={{
                      fontSize: '10px', ...isoBadgeStyle(node.isolation, node.sandbox),
                      borderRadius: '4px', padding: '1px 6px',
                    }}>{isoLabel(node.isolation, node.sandbox)}</span>

                    {/* Edit / Delete buttons — only when connected */}
                    {isConnected && (
                      <>
                        <button
                          onClick={(e) => { e.stopPropagation(); openEditForm(node); }}
                          style={{ ...btnStyle, background: 'transparent', color: 'var(--text3)', padding: '2px 6px', fontSize: '12px', border: 'none' }}
                          title="编辑节点"
                        >✏️</button>
                        <button
                          onClick={(e) => { e.stopPropagation(); setDeleteConfirm(node.id); }}
                          style={{ ...btnStyle, background: 'transparent', color: 'var(--text3)', padding: '2px 6px', fontSize: '12px', border: 'none' }}
                          title="删除节点"
                        >🗑️</button>
                      </>
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

      {/* Delete confirmation modal */}
      {deleteConfirm && (
        <div style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0,0,0,0.5)', display: 'flex',
          alignItems: 'center', justifyContent: 'center', zIndex: 100,
        }}>
          <div style={{
            background: 'var(--bg1)', border: '1px solid var(--border)',
            borderRadius: '12px', padding: '24px', maxWidth: '360px', textAlign: 'center',
          }}>
            <p style={{ fontSize: '14px', color: 'var(--text)', marginBottom: '16px' }}>
              确定要删除此节点吗？此操作不可撤销。
            </p>
            <div style={{ display: 'flex', gap: '8px', justifyContent: 'center' }}>
              <button onClick={() => setDeleteConfirm(null)}
                style={{ ...btnStyle, background: 'var(--bg3)', color: 'var(--text2)', border: '1px solid var(--border)' }}>
                取消
              </button>
              <button onClick={() => handleDelete(deleteConfirm)}
                style={{ ...btnStyle, background: '#dc2626', color: '#fff' }}>
                删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
