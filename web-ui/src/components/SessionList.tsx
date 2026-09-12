import React, { useEffect, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';

interface Props {
  isConnected: boolean;
  onSwitchToChat: () => void;
  onListLocalSessions: () => void;
  onSwitchLocalSession: (name: string) => void;
  onNewLocalSession: (name: string) => void;
  onDeleteLocalSession: (name: string) => void;
  onRenameLocalSession: (oldName: string, newName: string) => void;
}

const NAME_RE = /^[a-zA-Z0-9_-]+$/;

/** Sidebar session list — sessions are the entry point; the workdir is an attribute of one. */
export const SessionList: React.FC<Props> = ({
  isConnected, onSwitchToChat, onListLocalSessions,
  onSwitchLocalSession, onNewLocalSession, onDeleteLocalSession, onRenameLocalSession,
}) => {
  const localSessions = useAgentStore(s => s.localSessions);
  const activeSessionName = useAgentStore(s => s.activeSessionName);

  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState('');
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  useEffect(() => {
    if (isConnected) onListLocalSessions();
  }, [isConnected, onListLocalSessions]);

  const submitCreate = () => {
    const name = draft.trim();
    if (!name) return;
    if (name === '_active') { alert('_active 是保留名称'); return; }
    if (!NAME_RE.test(name)) { alert('名称只能包含字母、数字、连字符和下划线'); return; }
    onNewLocalSession(name);
    setCreating(false);
    setDraft('');
    onSwitchToChat();
  };

  const submitRename = (oldName: string) => {
    const next = renameValue.trim();
    if (next && next !== oldName) onRenameLocalSession(oldName, next);
    setRenameTarget(null);
    setRenameValue('');
  };

  return (
    <div className="sidebar-section grow">
      <div className="side-head">
        <span className="side-title">会话 ({localSessions.length})</span>
        <button className="side-add" onClick={() => { setCreating(true); setDraft(''); }} title="新建会话">＋</button>
      </div>

      {creating && (
        <div className="sess-rename">
          <input
            className="sess-input"
            autoFocus
            value={draft}
            placeholder="会话名称"
            onChange={e => setDraft(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter') submitCreate();
              if (e.key === 'Escape') { setCreating(false); setDraft(''); }
            }}
          />
          <button className="sess-icon-btn" onClick={submitCreate} title="创建">✓</button>
          <button className="sess-icon-btn" onClick={() => { setCreating(false); setDraft(''); }} title="取消">✕</button>
        </div>
      )}

      {!isConnected ? (
        <div className="side-empty">连接后可管理会话</div>
      ) : localSessions.length === 0 && !creating ? (
        <div className="side-empty">暂无会话，点 ＋ 新建</div>
      ) : (
        <div className="sess-list">
          {localSessions.map(s => {
            const name = s.session_name || s.id;
            const isActive = name === activeSessionName;

            if (renameTarget === name) {
              return (
                <div key={name} className="sess-rename">
                  <input
                    className="sess-input"
                    autoFocus
                    defaultValue={name}
                    onChange={e => setRenameValue(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter') submitRename(name);
                      if (e.key === 'Escape') setRenameTarget(null);
                    }}
                  />
                  <button className="sess-icon-btn" onClick={() => submitRename(name)} title="确认">✓</button>
                  <button className="sess-icon-btn" onClick={() => setRenameTarget(null)} title="取消">✕</button>
                </div>
              );
            }

            return (
              <div
                key={name}
                className={`sess-item${isActive ? ' active' : ''}`}
                onClick={() => { onSwitchLocalSession(name); onSwitchToChat(); }}
                title={`${name} · ${s.message_count} 条消息`}
              >
                {isActive && <span className="sess-star">★</span>}
                <span className="sess-name">{name}</span>

                {confirmDelete === name ? (
                  <span className="sess-confirm" onClick={e => e.stopPropagation()}>
                    删除?
                    <button onClick={() => { onDeleteLocalSession(name); setConfirmDelete(null); }}>是</button>
                    <button onClick={() => setConfirmDelete(null)}>否</button>
                  </span>
                ) : (
                  <>
                    <span className="sess-count">{s.message_count}</span>
                    <span className="sess-actions" onClick={e => e.stopPropagation()}>
                      <button
                        className="sess-icon-btn"
                        title="重命名"
                        onClick={() => { setRenameTarget(name); setRenameValue(name); }}
                      >✎</button>
                      {!isActive && (
                        <button
                          className="sess-icon-btn danger"
                          title="删除"
                          onClick={() => setConfirmDelete(name)}
                        >🗑</button>
                      )}
                    </span>
                  </>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
