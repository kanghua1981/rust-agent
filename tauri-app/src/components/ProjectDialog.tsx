import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { v4 as uuidv4 } from 'uuid';
import type { ProjectDefinition } from '../types/agent';

interface Props {
  onConnect: () => void;
  onClose: () => void;
  /** If set, auto-edit the given project on open. */
  editProjectId?: string | null;
}

/** Generate a label from workdir path */
function labelFromWorkdir(workdir: string): string {
  return workdir.replace(/\/+$/, '').split('/').filter(Boolean).pop() || '未命名项目';
}

/** Abbreviate URL for display */
function shortUrl(url: string): string {
  try {
    const u = new URL(url.replace(/^ws(s?):/, 'http$1:'));
    return u.host;
  } catch {
    return url.length > 30 ? url.slice(0, 27) + '...' : url;
  }
}

export const ProjectDialog: React.FC<Props> = ({ onConnect, onClose, editProjectId }) => {
  const store = useAgentStore();

  const projects = store.projects ?? {};
  const projectList = Object.values(projects);

  // Form state
  const [label, setLabel] = useState('');
  const [serverUrl, setLocalUrl] = useState(store.serverUrl || 'ws://localhost:9527');
  const [workdir, setLocalWorkdir] = useState(store.workdir || '');
  const [isolation, setIsolation] = useState<'normal' | 'container' | 'sandbox'>(
    (store.config.isolation as any) || 'normal'
  );
  const [agentMode, setAgentMode] = useState<'auto' | 'simple' | 'plan' | 'pipeline'>(
    store.config.agentMode || 'auto'
  );
  const [autoApprove, setAutoApprove] = useState(store.config.autoApprove ?? false);
  const [newSessionOnConnect, setNewSessionOnConnect] = useState(false);

  // Editing mode
  const [editingId, setEditingId] = useState<string | null>(null);

  // Auto-edit when editProjectId is provided
  useEffect(() => {
    if (editProjectId && projects[editProjectId]) {
      handleEdit(projects[editProjectId]);
    }
    // Only run when the dialog opens with a specific project
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editProjectId]);

  const handleWorkdirChange = (dir: string) => {
    setLocalWorkdir(dir);
    // Auto-fill label when not manually set
    if (!label || label === labelFromWorkdir(workdir)) {
      setLabel(labelFromWorkdir(dir));
    }
  };

  const handleSaveAndConnect = () => {
    if (!serverUrl.trim()) return;

    const id = editingId || uuidv4();
    const project: ProjectDefinition = {
      id,
      label: label.trim() || labelFromWorkdir(workdir),
      serverUrl: serverUrl.trim(),
      workdir: workdir.trim(),
      isolation,
      agentMode,
      autoApprove,
      newSessionOnConnect,
      createdAt: editingId ? projects[editingId]?.createdAt || new Date().toISOString() : new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    // Save project
    store.addProject(project);

    // Set flat proxy fields (ConnectModal compatibility)
    store.setServerUrl(project.serverUrl);
    store.setWorkdir(project.workdir);
    store.setConfig({
      isolation: project.isolation,
      agentMode: project.agentMode,
      autoApprove: project.autoApprove,
      newSessionOnConnect: project.newSessionOnConnect,
    });

    onConnect();
    onClose();
  };

  const handleEdit = (project: ProjectDefinition) => {
    setEditingId(project.id);
    setLabel(project.label);
    setLocalUrl(project.serverUrl);
    setLocalWorkdir(project.workdir);
    setIsolation(project.isolation);
    setAgentMode(project.agentMode);
    setAutoApprove(project.autoApprove);
    setNewSessionOnConnect(project.newSessionOnConnect);
  };

  const handleDelete = (projectId: string) => {
    if (window.confirm('确定要删除此项目吗？')) {
      store.deleteProject(projectId);
    }
  };

  const handleSelectProject = (project: ProjectDefinition) => {
    store.setServerUrl(project.serverUrl);
    store.setWorkdir(project.workdir);
    store.setConfig({
      isolation: project.isolation,
      agentMode: project.agentMode,
      autoApprove: project.autoApprove,
      newSessionOnConnect: project.newSessionOnConnect,
    });
    onConnect();
    onClose();
  };

  const modalStyle: React.CSSProperties = {
    position: 'fixed',
    inset: 0,
    background: 'rgba(0,0,0,0.55)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 1000,
  };

  const cardStyle: React.CSSProperties = {
    background: 'var(--bg2)',
    border: '1px solid var(--border)',
    borderRadius: '12px',
    padding: '24px',
    width: '480px',
    maxWidth: '95vw',
    maxHeight: '85vh',
    overflowY: 'auto',
    boxShadow: '0 8px 32px rgba(0,0,0,0.4)',
  };

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '8px 10px',
    borderRadius: '6px',
    border: '1px solid var(--border)',
    background: 'var(--bg)',
    color: 'var(--text)',
    fontSize: '13px',
    outline: 'none',
    boxSizing: 'border-box',
  };

  const selectStyle: React.CSSProperties = {
    ...inputStyle,
    cursor: 'pointer',
  };

  const labelStyle: React.CSSProperties = {
    fontSize: '11px',
    fontWeight: '600',
    color: 'var(--text3)',
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    marginBottom: '4px',
    display: 'block',
  };

  const btnStyle = (primary?: boolean): React.CSSProperties => ({
    padding: '8px 16px',
    borderRadius: '6px',
    border: primary ? 'none' : '1px solid var(--border)',
    background: primary ? 'var(--accent)' : 'transparent',
    color: primary ? '#fff' : 'var(--text2)',
    fontSize: '13px',
    cursor: 'pointer',
    fontWeight: primary ? '500' : '400',
  });

  return (
    <div style={modalStyle} onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div style={cardStyle}>
        {/* Header */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '18px' }}>
          <h2 style={{ margin: 0, fontSize: '16px', fontWeight: '600' }}>
            {editingId ? '编辑项目' : '添加项目'}
          </h2>
          <button onClick={onClose} style={{
            background: 'transparent', border: 'none', color: 'var(--text3)',
            fontSize: '18px', cursor: 'pointer', padding: '2px 6px',
          }}>×</button>
        </div>

        {/* Form */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {/* Project name */}
          <div>
            <label style={labelStyle}>项目名称</label>
            <input
              style={inputStyle}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={labelFromWorkdir(workdir) || '例如: my-frontend'}
            />
          </div>

          {/* Server URL */}
          <div>
            <label style={labelStyle}>服务器地址</label>
            <input
              style={inputStyle}
              value={serverUrl}
              onChange={(e) => setLocalUrl(e.target.value)}
              placeholder="ws://localhost:9527"
            />
          </div>

          {/* Workdir */}
          <div>
            <label style={labelStyle}>工作目录</label>
            <input
              style={inputStyle}
              value={workdir}
              onChange={(e) => handleWorkdirChange(e.target.value)}
              placeholder="/path/to/project"
            />
          </div>

          {/* Isolation + Agent mode */}
          <div style={{ display: 'flex', gap: '12px' }}>
            <div style={{ flex: 1 }}>
              <label style={labelStyle}>隔离模式</label>
              <select
                style={selectStyle}
                value={isolation}
                onChange={(e) => setIsolation(e.target.value as any)}
              >
                <option value="normal">普通模式</option>
                <option value="container">容器模式</option>
                <option value="sandbox">沙盒模式</option>
              </select>
            </div>
            <div style={{ flex: 1 }}>
              <label style={labelStyle}>运行模式</label>
              <select
                style={selectStyle}
                value={agentMode}
                onChange={(e) => setAgentMode(e.target.value as any)}
              >
                <option value="auto">自动</option>
                <option value="simple">单层</option>
                <option value="plan">计划</option>
                <option value="pipeline">流水线</option>
              </select>
            </div>
          </div>

          {/* Checkboxes */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '12px', color: 'var(--text2)', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={autoApprove}
                onChange={(e) => setAutoApprove(e.target.checked)}
              />
              自动确认工具调用
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '12px', color: 'var(--text2)', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={newSessionOnConnect}
                onChange={(e) => setNewSessionOnConnect(e.target.checked)}
              />
              连接后新建会话
            </label>
          </div>

          {/* Actions */}
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '6px' }}>
            {editingId && (
              <button
                style={btnStyle()}
                onClick={() => {
                  setEditingId(null);
                  setLabel('');
                  setLocalUrl('ws://localhost:9527');
                  setLocalWorkdir('');
                  setIsolation('normal');
                  setAgentMode('auto');
                  setAutoApprove(false);
                  setNewSessionOnConnect(false);
                }}
              >取消编辑</button>
            )}
            <button style={btnStyle()} onClick={onClose}>取消</button>
            <button style={btnStyle(true)} onClick={handleSaveAndConnect}>
              {editingId ? '保存并连接' : '保存并连接'}
            </button>
          </div>
        </div>

        {/* Existing projects list */}
        {projectList.length > 0 && (
          <div style={{ marginTop: '24px', borderTop: '1px solid var(--border)', paddingTop: '16px' }}>
            <label style={{ ...labelStyle, marginBottom: '8px' }}>已有项目 ({projectList.length})</label>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', maxHeight: '180px', overflowY: 'auto' }}>
              {projectList.map((p) => (
                <div
                  key={p.id}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    padding: '6px 8px',
                    borderRadius: '6px',
                    fontSize: '12px',
                    transition: 'background 0.1s',
                  }}
                  onMouseOver={(e) => { e.currentTarget.style.background = 'var(--bg3)'; }}
                  onMouseOut={(e) => { e.currentTarget.style.background = 'transparent'; }}
                >
                  <div
                    style={{ flex: 1, cursor: 'pointer', minWidth: 0 }}
                    onClick={() => handleSelectProject(p)}
                    title={`${p.serverUrl} → ${p.workdir}`}
                  >
                    <div style={{ fontWeight: '500', color: 'var(--text)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {p.label}
                    </div>
                    <div style={{ color: 'var(--text3)', fontSize: '10px', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {shortUrl(p.serverUrl)} {p.workdir ? `→ ${p.workdir.split('/').filter(Boolean).pop()}` : ''}
                    </div>
                  </div>
                  <div style={{ display: 'flex', gap: '4px', flexShrink: 0, marginLeft: '8px' }}>
                    <button
                      onClick={() => handleEdit(p)}
                      title="编辑"
                      style={{
                        background: 'transparent', border: 'none', color: 'var(--text3)',
                        cursor: 'pointer', fontSize: '12px', padding: '2px 4px',
                      }}
                    >✎</button>
                    <button
                      onClick={() => handleDelete(p.id)}
                      title="删除"
                      style={{
                        background: 'transparent', border: 'none', color: 'var(--text3)',
                        cursor: 'pointer', fontSize: '12px', padding: '2px 4px',
                      }}
                    >🗑</button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
