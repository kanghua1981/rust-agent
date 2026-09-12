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
  const [agentMode, setAgentMode] = useState<'auto' | 'simple' | 'plan'>(
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

  return (
    <div className="overlay-center" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="dlg-card">
        {/* Header */}
        <div className="dlg-head">
          <h2>{editingId ? '编辑项目' : '添加项目'}</h2>
          <button className="dlg-close" onClick={onClose}>×</button>
        </div>

        {/* Form */}
        <div className="dlg-form">
          {/* Project name */}
          <div>
            <label className="form-label up">项目名称</label>
            <input
              className="field"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={labelFromWorkdir(workdir) || '例如: my-frontend'}
            />
          </div>

          {/* Server URL */}
          <div>
            <label className="form-label up">服务器地址</label>
            <input
              className="field"
              value={serverUrl}
              onChange={(e) => setLocalUrl(e.target.value)}
              placeholder="ws://localhost:9527"
            />
          </div>

          {/* Workdir */}
          <div>
            <label className="form-label up">工作目录</label>
            <input
              className="field"
              value={workdir}
              onChange={(e) => handleWorkdirChange(e.target.value)}
              placeholder="/path/to/project"
            />
          </div>

          {/* Isolation + Agent mode */}
          <div className="row" style={{ gap: 12 }}>
            <div className="fill">
              <label className="form-label up">隔离模式</label>
              <select
                className="field"
                value={isolation}
                onChange={(e) => setIsolation(e.target.value as any)}
              >
                <option value="normal">普通模式</option>
                <option value="container">容器模式</option>
                <option value="sandbox">沙盒模式</option>
              </select>
            </div>
            <div className="fill">
              <label className="form-label up">运行模式</label>
              <select
                className="field"
                value={agentMode}
                onChange={(e) => setAgentMode(e.target.value as any)}
              >
                <option value="auto">自动</option>
                <option value="simple">单层</option>
                <option value="plan">计划</option>
              </select>
            </div>
          </div>

          {/* Checkboxes */}
          <div className="check-stack">
            <label className="check-line">
              <input
                type="checkbox"
                checked={autoApprove}
                onChange={(e) => setAutoApprove(e.target.checked)}
              />
              自动确认工具调用
            </label>
            <label className="check-line">
              <input
                type="checkbox"
                checked={newSessionOnConnect}
                onChange={(e) => setNewSessionOnConnect(e.target.checked)}
              />
              连接后新建会话
            </label>
          </div>

          {/* Actions */}
          <div className="dlg-actions end">
            {editingId && (
              <button
                className="btn-dlg"
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
            <button className="btn-dlg" onClick={onClose}>取消</button>
            <button className="btn-dlg primary" onClick={handleSaveAndConnect}>保存并连接</button>
          </div>
        </div>

        {/* Existing projects list */}
        {projectList.length > 0 && (
          <div className="dlg-section">
            <label className="form-label up" style={{ marginBottom: 8 }}>已有项目 ({projectList.length})</label>
            <div className="project-list">
              {projectList.map((p) => (
                <div key={p.id} className="project-item">
                  <div
                    className="project-item-body"
                    onClick={() => handleSelectProject(p)}
                    title={`${p.serverUrl} → ${p.workdir}`}
                  >
                    <div className="project-item-title">{p.label}</div>
                    <div className="project-item-sub">
                      {shortUrl(p.serverUrl)} {p.workdir ? `→ ${p.workdir.split('/').filter(Boolean).pop()}` : ''}
                    </div>
                  </div>
                  <div className="project-item-actions">
                    <button className="task-icon-btn" style={{ fontSize: 12 }} onClick={() => handleEdit(p)} title="编辑">✎</button>
                    <button className="task-icon-btn" style={{ fontSize: 12 }} onClick={() => handleDelete(p.id)} title="删除">🗑</button>
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
