import React, { useState } from 'react';
import type { SessionMeta } from '../types/agent';

interface Props {
  /** Sessions belonging to the workspace this list is nested under. */
  sessions: SessionMeta[];
  activeSessionName: string | null;
  onSwitchToChat: () => void;
  onSwitchLocalSession: (name: string) => void;
  onNewLocalSession: (name: string) => void;
  onDeleteLocalSession: (name: string) => void;
  onRenameLocalSession: (oldName: string, newName: string) => void;
}

const NAME_RE = /^[a-zA-Z0-9_-]+$/;

/**
 * Session rows nested under a workspace. Switching, renaming, deleting and
 * creating all act on the session, never on the workspace.
 */
export const SessionList: React.FC<Props> = ({
  sessions, activeSessionName, onSwitchToChat,
  onSwitchLocalSession, onNewLocalSession, onDeleteLocalSession, onRenameLocalSession,
}) => {
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState('');
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

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
    <div className="sess-group">
      {sessions.map(s => {
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
                  <button
                    className="sess-icon-btn danger"
                    title="删除"
                    onClick={() => setConfirmDelete(name)}
                  >🗑</button>
                </span>
              </>
            )}
          </div>
        );
      })}

      {creating ? (
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
      ) : (
        <div className="sess-new" onClick={() => { setCreating(true); setDraft(''); }}>
          <span>＋</span><span>新会话</span>
        </div>
      )}
    </div>
  );
};
