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

  const btnBase: React.CSSProperties = {
    padding: '4px 10px', borderRadius: '6px', fontSize: '12px',
    fontWeight: '500', cursor: 'pointer', border: '1px solid var(--border)',
  };

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '24px', maxWidth: '760px', margin: '0 auto', width: '100%' }}>
      <h2 style={{ fontSize: '18px', fontWeight: '600', color: 'var(--text)', marginBottom: '4px' }}>会话管理</h2>
      <p style={{ fontSize: '13px', color: 'var(--text3)', marginBottom: '20px' }}>
        管理本地命名会话 · 每个工程可拥有多个独立会话
      </p>

      {/* ── Active session card ──────────────────────────────────────────────── */}
      <section style={{ marginBottom: '24px' }}>
        <p style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: '10px' }}>
          当前会话
        </p>
        <div style={{
          background: 'var(--bg2)', border: '1px solid var(--accent)', borderLeft: '4px solid var(--accent)',
          borderRadius: '10px', padding: '16px',
          display: 'flex', alignItems: 'center', gap: '16px', flexWrap: 'wrap',
        }}>
          <div style={{ flex: 1, minWidth: '120px' }}>
            <p style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)' }}>
              {activeSessionName || 'default'}
              <span style={{ fontSize: '11px', color: 'var(--accent)', marginLeft: '8px', fontWeight: '400' }}>(活跃)</span>
            </p>
            <p style={{ fontSize: '12px', color: 'var(--text3)', marginTop: '2px' }}>
              {messages.filter(m => m.role !== 'system').length} 条消息
            </p>
          </div>
          <button
            onClick={() => setShowNewDialog(true)}
            style={{ ...btnBase, background: 'var(--accent)', color: '#fff', borderColor: 'var(--accent)' }}
          >
            + 新建
          </button>
        </div>
      </section>

      {/* ── New session dialog ────────────────────────────────────────────────── */}
      {showNewDialog && (
        <div style={{
          position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.4)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100,
        }}>
          <div style={{
            background: 'var(--bg)', border: '1px solid var(--border)', borderRadius: '12px',
            padding: '24px', minWidth: '320px', boxShadow: '0 8px 32px rgba(0,0,0,0.3)',
          }}>
            <p style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', marginBottom: '16px' }}>新建会话</p>
            <input
              autoFocus
              value={newSessionName}
              onChange={e => setNewSessionName(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') handleCreateSession(); if (e.key === 'Escape') setShowNewDialog(false); }}
              placeholder="会话名称（如 feature-login）"
              style={{
                width: '100%', padding: '8px 12px', borderRadius: '8px', border: '1px solid var(--border)',
                background: 'var(--bg2)', color: 'var(--text)', fontSize: '14px', marginBottom: '16px',
              }}
            />
            <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
              <button onClick={() => setShowNewDialog(false)} style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--text2)' }}>
                取消
              </button>
              <button onClick={handleCreateSession} disabled={!newSessionName.trim()}
                style={{
                  ...btnBase, background: newSessionName.trim() ? 'var(--accent)' : 'var(--bg3)',
                  color: newSessionName.trim() ? '#fff' : 'var(--text3)',
                }}>
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Local named sessions list ─────────────────────────────────────────── */}
      <section style={{ marginBottom: '28px' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '10px' }}>
          <p style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
            本地会话 ({localSessions.length})
          </p>
          <button
            onClick={onListLocalSessions}
            disabled={!isConnected}
            style={{ ...btnBase, background: 'var(--bg3)', color: isConnected ? 'var(--text2)' : 'var(--text3)', fontSize: '11px' }}
          >
            ↻ 刷新
          </button>
        </div>

        {!isConnected && (
          <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text3)', fontSize: '13px', background: 'var(--bg2)', borderRadius: '10px', border: '1px solid var(--border)' }}>
            连接到 Agent 后可管理本地会话
          </div>
        )}

        {isConnected && localSessions.length === 0 && (
          <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text3)', fontSize: '13px', background: 'var(--bg2)', borderRadius: '10px', border: '1px solid var(--border)' }}>
            暂无本地会话 · 点击"+ 新建"创建
          </div>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          {localSessions.map((s: SessionMeta) => {
            const name = s.session_name || s.id;
            const isActive = name === activeSessionName;
            const isRenaming = renameTarget === name;
            return (
              <div key={name} style={{
                background: isActive ? 'var(--accent-subtle, var(--bg2))' : 'var(--bg2)',
                border: isActive ? '1px solid var(--accent)' : '1px solid var(--border)',
                borderRadius: '10px', padding: '14px 16px',
                display: 'flex', alignItems: 'flex-start', gap: '12px',
              }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  {isRenaming ? (
                    <div style={{ display: 'flex', gap: '8px', marginBottom: '4px' }}>
                      <input
                        autoFocus
                        defaultValue={name}
                        onChange={e => setRenameValue(e.target.value)}
                        onKeyDown={e => {
                          if (e.key === 'Enter') handleRename(name);
                          if (e.key === 'Escape') setRenameTarget(null);
                        }}
                        style={{
                          flex: 1, padding: '4px 8px', borderRadius: '6px', border: '1px solid var(--accent)',
                          background: 'var(--bg)', color: 'var(--text)', fontSize: '13px',
                        }}
                      />
                      <button onClick={() => handleRename(name)} style={{ ...btnBase, background: 'var(--accent)', color: '#fff' }}>确认</button>
                      <button onClick={() => setRenameTarget(null)} style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--text2)' }}>取消</button>
                    </div>
                  ) : (
                    <p style={{
                      fontSize: '13px', fontWeight: '500', color: 'var(--text)',
                      overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                      marginBottom: '4px',
                    }}>
                      {isActive && <span style={{ color: 'var(--accent)', marginRight: '4px' }}>★</span>}
                      {name}
                    </p>
                  )}
                  <p style={{
                    fontSize: '11px', color: 'var(--text3)',
                    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                    marginBottom: '4px',
                  }}>
                    {s.summary || '(空会话)'}
                  </p>
                  <div style={{ display: 'flex', gap: '12px', fontSize: '11px', color: 'var(--text3)' }}>
                    <span>💬 {s.message_count} 条消息</span>
                    <span>🕒 {formatDate(s.updated_at)}</span>
                  </div>
                </div>

                <div style={{ display: 'flex', gap: '6px', flexShrink: 0, alignItems: 'center' }}>
                  {confirmDeleteName === name ? (
                    <>
                      <span style={{ fontSize: '12px', color: 'var(--red)', marginRight: '2px' }}>确认删除?</span>
                      <button onClick={() => handleDelete(name)} style={{ ...btnBase, background: 'var(--red)', color: '#fff', borderColor: 'var(--red)' }}>确认</button>
                      <button onClick={() => setConfirmDeleteName(null)} style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--text2)' }}>取消</button>
                    </>
                  ) : (
                    <>
                      {!isActive && (
                        <button onClick={() => handleSwitch(name)} disabled={!isConnected}
                          style={{
                            ...btnBase, background: isConnected ? 'var(--accent)' : 'var(--bg3)',
                            color: isConnected ? '#fff' : 'var(--text3)', borderColor: isConnected ? 'var(--accent)' : 'var(--border)',
                          }}>
                          切换
                        </button>
                      )}
                      <button onClick={() => { setRenameTarget(name); setRenameValue(name); }}
                        style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--text2)' }}>
                        重命名
                      </button>
                      {!isActive && (
                        <button onClick={() => handleDelete(name)}
                          style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--red)' }}>
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
      <section style={{ marginBottom: '28px' }}>
        <p style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: '10px' }}>
          导出当前对话
        </p>
        <div style={{
          background: 'var(--bg2)', border: '1px solid var(--border)',
          borderRadius: '10px', padding: '16px',
          display: 'flex', alignItems: 'center', gap: '16px', flexWrap: 'wrap',
        }}>
          <div style={{ flex: 1, minWidth: '160px' }}>
            <p style={{ fontSize: '13px', color: 'var(--text)', fontWeight: '500' }}>
              当前对话 · {messages.filter(m => m.role !== 'system').length} 条消息
            </p>
            <p style={{ fontSize: '12px', color: 'var(--text3)', marginTop: '2px' }}>
              {saveStatus?.ok
                ? <span style={{ color: '#4caf50' }}>{saveStatus.ok}</span>
                : saveStatus?.err
                  ? <span style={{ color: '#f44336' }}>保存失败: {saveStatus.err}</span>
                  : '将聊天记录导出到本地文件'}
            </p>
          </div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              onClick={() => exportSessionAsMarkdown({ messages }, handleSaved, handleSaveError)}
              disabled={messages.length === 0}
              style={{ ...btnBase, background: messages.length ? 'var(--accent)' : 'var(--bg3)', color: messages.length ? '#fff' : 'var(--text3)' }}
            >↓ Markdown</button>
            <button
              onClick={() => exportSessionAsJson({ messages }, handleSaved, handleSaveError)}
              disabled={messages.length === 0}
              style={{ ...btnBase, background: 'var(--bg3)', color: messages.length ? 'var(--text)' : 'var(--text3)' }}
            >↓ JSON</button>
          </div>
        </div>
      </section>

      {/* ── Global session history (existing) ─────────────────────────────────── */}
      <section>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '10px' }}>
          <p style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
            全局历史会话 ({sessionList.length})
          </p>
          <button onClick={onListSessions} disabled={!isConnected}
            style={{ ...btnBase, background: 'var(--bg3)', color: isConnected ? 'var(--text2)' : 'var(--text3)', fontSize: '11px' }}>
            ↻ 刷新
          </button>
        </div>
        {isConnected && sessionList.length === 0 && (
          <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text3)', fontSize: '13px', background: 'var(--bg2)', borderRadius: '10px', border: '1px solid var(--border)' }}>
            暂无全局历史会话
          </div>
        )}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          {sessionList.map((s: SessionMeta) => (
            <div key={s.id} style={{
              background: 'var(--bg2)', border: '1px solid var(--border)',
              borderRadius: '10px', padding: '14px 16px',
              display: 'flex', alignItems: 'flex-start', gap: '12px',
            }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <p style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginBottom: '4px' }}>
                  {s.summary || '(无摘要)'}
                </p>
                <p style={{ fontSize: '11px', color: 'var(--text3)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginBottom: '4px' }}>
                  📂 {s.working_dir}
                </p>
                <div style={{ display: 'flex', gap: '12px', fontSize: '11px', color: 'var(--text3)' }}>
                  <span>🕒 {formatDate(s.updated_at)}</span>
                  <span>💬 {s.message_count} 条消息</span>
                </div>
              </div>
              <div style={{ display: 'flex', gap: '6px', flexShrink: 0, alignItems: 'center' }}>
                <button onClick={() => { onLoadSessionById(s.id); onSwitchToChat(); }} disabled={!isConnected}
                  style={{ ...btnBase, background: isConnected ? 'var(--accent)' : 'var(--bg3)', color: isConnected ? '#fff' : 'var(--text3)' }}>
                  切换
                </button>
                <button onClick={() => onDeleteSession(s.id)}
                  style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--red)' }}>
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
