import React, { useEffect, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import type { SessionMeta } from '../types/agent';
import { isTauri, exportSessionAsMarkdown, exportSessionAsJson } from '../utils/export';

interface Props {
  onSwitchToChat: () => void;
  isConnected: boolean;
  // Global sessions (existing)
  onListSessions: () => void;
  onDeleteSession: (id: string) => void;
  onLoadSessionById: (id: string) => void;
  // Local named sessions (new)
  onListLocalSessions: () => void;
  onSwitchLocalSession: (name: string) => void;
  onNewLocalSession: (name: string) => void;
  onDeleteLocalSession: (name: string) => void;
  onRenameLocalSession: (oldName: string, newName: string) => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export const SessionsPanel: React.FC<Props> = ({
  onSwitchToChat, isConnected,
  onListSessions, onDeleteSession, onLoadSessionById,
  onListLocalSessions, onSwitchLocalSession, onNewLocalSession,
  onDeleteLocalSession, onRenameLocalSession,
}) => {
  const { messages, sessionList, localSessions, activeSessionName } = useAgentStore();
  const [confirmDeleteName, setConfirmDeleteName] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<{ ok?: string; err?: string } | null>(null);
  const [showNewDialog, setShowNewDialog] = useState(false);
  const [newSessionName, setNewSessionName] = useState('');
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');

  const handleSaved = (path: string) => {
    setSaveStatus({ ok: isTauri() ? `已保存到: ${path}` : '已下载' });
    setTimeout(() => setSaveStatus(null), 4000);
  };
  const handleSaveError = (err: string) => {
    setSaveStatus({ err });
    setTimeout(() => setSaveStatus(null), 5000);
  };

  // Auto-load lists when panel becomes active and connected
  useEffect(() => {
    if (isConnected) {
      onListLocalSessions();
      onListSessions();
    }
  }, [isConnected]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleCreateSession = () => {
    const name = newSessionName.trim();
    if (!name) return;
    if (name === '_active') { alert('_active is a reserved name'); return; }
    if (!/^[a-zA-Z0-9_-]+$/.test(name)) { alert('Name can only contain letters, numbers, hyphens, and underscores'); return; }
    onNewLocalSession(name);
    setShowNewDialog(false);
    setNewSessionName('');
    onSwitchToChat();
  };

  const handleSwitch = (name: string) => {
    onSwitchLocalSession(name);
    onSwitchToChat();
  };

  const handleDelete = (name: string) => {
    if (confirmDeleteName === name) {
      onDeleteLocalSession(name);
      setConfirmDeleteName(null);
    } else {
      setConfirmDeleteName(name);
    }
  };

  const handleRename = (oldName: string) => {
    const newName = renameValue.trim();
    if (!newName) { setRenameTarget(null); return; }
    onRenameLocalSession(oldName, newName);
    setRenameTarget(null);
    setRenameValue('');
  };

  const formatDate = (s: string) => {
    try { return new Date(s).toLocaleString(); } catch { return s; }
  };

  return (
    <div className="page">
      <h2 className="page-title">会话管理</h2>
      <p className="page-sub">管理本地命名会话 · 每个工程可拥有多个独立会话</p>

      {/* ── Active session card ──────────────────────────────────────────────── */}
      <section className="group">
        <p className="group-label">当前会话</p>
        <div className="card row accent-left">
          <div className="fill" style={{ minWidth: 120 }}>
            <p style={{ fontSize: 15, fontWeight: 600, color: 'var(--text)' }}>
              {activeSessionName || 'default'}
              <span style={{ fontSize: 11, color: 'var(--accent)', marginLeft: 8, fontWeight: 400 }}>(活跃)</span>
            </p>
            <p style={{ fontSize: 12, color: 'var(--text3)', marginTop: 2 }}>
              {messages.filter(m => m.role !== 'system').length} 条消息
            </p>
          </div>
          <button className="btn-primary" onClick={() => setShowNewDialog(true)}>+ 新建</button>
        </div>
      </section>

      {/* ── New session dialog ────────────────────────────────────────────────── */}
      {showNewDialog && (
        <div className="modal-backdrop">
          <div className="modal wide">
            <p style={{ fontSize: 15, fontWeight: 600, color: 'var(--text)', marginBottom: 16 }}>新建会话</p>
            <input
              className="field"
              autoFocus
              value={newSessionName}
              onChange={e => setNewSessionName(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') handleCreateSession(); if (e.key === 'Escape') setShowNewDialog(false); }}
              placeholder="会话名称（如 feature-login）"
              style={{ fontSize: 14 }}
            />
            <div className="form-actions">
              <button className="btn-secondary" onClick={() => setShowNewDialog(false)}>取消</button>
              <button className="btn-primary" onClick={handleCreateSession} disabled={!newSessionName.trim()}>创建</button>
            </div>
          </div>
        </div>
      )}

      {/* ── Local named sessions list ─────────────────────────────────────────── */}
      <section className="group">
        <div className="group-head">
          <p className="group-label">本地会话 ({localSessions.length})</p>
          <button className="btn-ghost" onClick={onListLocalSessions} disabled={!isConnected}>↻ 刷新</button>
        </div>

        {!isConnected && <div className="empty-box">连接到 Agent 后可管理本地会话</div>}
        {isConnected && localSessions.length === 0 && (
          <div className="empty-box">暂无本地会话 · 点击"+ 新建"创建</div>
        )}

        <div className="list-stack">
          {localSessions.map((s: SessionMeta) => {
            const name = s.session_name || s.id;
            const isActive = name === activeSessionName;
            const isRenaming = renameTarget === name;
            return (
              <div key={name} className={`session-item${isActive ? ' active' : ''}`}>
                <div className="fill">
                  {isRenaming ? (
                    <div className="session-rename">
                      <input
                        className="field"
                        autoFocus
                        defaultValue={name}
                        onChange={e => setRenameValue(e.target.value)}
                        onKeyDown={e => {
                          if (e.key === 'Enter') handleRename(name);
                          if (e.key === 'Escape') setRenameTarget(null);
                        }}
                        style={{ flex: 1, padding: '4px 8px', borderColor: 'var(--accent)' }}
                      />
                      <button className="btn-primary" onClick={() => handleRename(name)}>确认</button>
                      <button className="btn-secondary" onClick={() => setRenameTarget(null)}>取消</button>
                    </div>
                  ) : (
                    <p className="session-name">
                      {isActive && <span className="star">★</span>}
                      {name}
                    </p>
                  )}
                  <p className="session-sub">{s.summary || '(空会话)'}</p>
                  <div className="session-meta">
                    <span>💬 {s.message_count} 条消息</span>
                    <span>🕒 {formatDate(s.updated_at)}</span>
                  </div>
                </div>

                <div className="session-actions">
                  {confirmDeleteName === name ? (
                    <>
                      <span style={{ fontSize: 12, color: 'var(--red)', marginRight: 2 }}>确认删除?</span>
                      <button className="btn-danger" onClick={() => handleDelete(name)}>确认</button>
                      <button className="btn-secondary" onClick={() => setConfirmDeleteName(null)}>取消</button>
                    </>
                  ) : (
                    <>
                      {!isActive && (
                        <button className="btn-primary" onClick={() => handleSwitch(name)} disabled={!isConnected}>
                          切换
                        </button>
                      )}
                      <button className="btn-secondary" onClick={() => { setRenameTarget(name); setRenameValue(name); }}>
                        重命名
                      </button>
                      {!isActive && (
                        <button className="btn-secondary" style={{ color: 'var(--red)' }} onClick={() => handleDelete(name)}>
                          删除
                        </button>
                      )}
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </section>

      {/* ── Export current chat ──────────────────────────────────────────── */}
      <section className="group">
        <p className="group-label">导出当前对话</p>
        <div className="card row">
          <div className="fill" style={{ minWidth: 160 }}>
            <p style={{ fontSize: 13, color: 'var(--text)', fontWeight: 500 }}>
              当前对话 · {messages.filter(m => m.role !== 'system').length} 条消息
            </p>
            <p style={{ fontSize: 12, color: 'var(--text3)', marginTop: 2 }}>
              {saveStatus?.ok
                ? <span className="text-ok">{saveStatus.ok}</span>
                : saveStatus?.err
                  ? <span className="text-err">保存失败: {saveStatus.err}</span>
                  : '将聊天记录导出到本地文件'}
            </p>
          </div>
          <div className="row" style={{ gap: 8 }}>
            <button
              className="btn-primary"
              onClick={() => exportSessionAsMarkdown({ messages }, handleSaved, handleSaveError)}
              disabled={messages.length === 0}
            >↓ Markdown</button>
            <button
              className="btn-secondary"
              onClick={() => exportSessionAsJson({ messages }, handleSaved, handleSaveError)}
              disabled={messages.length === 0}
            >↓ JSON</button>
          </div>
        </div>
      </section>

      {/* ── Global session history (existing) ─────────────────────────────────── */}
      <section className="group">
        <div className="group-head">
          <p className="group-label">全局历史会话 ({sessionList.length})</p>
          <button className="btn-ghost" onClick={onListSessions} disabled={!isConnected}>↻ 刷新</button>
        </div>
        {isConnected && sessionList.length === 0 && <div className="empty-box">暂无全局历史会话</div>}
        <div className="list-stack">
          {sessionList.map((s: SessionMeta) => (
            <div key={s.id} className="session-item">
              <div className="fill">
                <p className="session-name">{s.summary || '(无摘要)'}</p>
                <p className="session-sub mono">📂 {s.working_dir}</p>
                <div className="session-meta">
                  <span>🕒 {formatDate(s.updated_at)}</span>
                  <span>💬 {s.message_count} 条消息</span>
                </div>
              </div>
              <div className="session-actions">
                <button className="btn-primary" onClick={() => { onLoadSessionById(s.id); onSwitchToChat(); }} disabled={!isConnected}>
                  切换
                </button>
                <button className="btn-secondary" style={{ color: 'var(--red)' }} onClick={() => onDeleteSession(s.id)}>
                  删除
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
};
