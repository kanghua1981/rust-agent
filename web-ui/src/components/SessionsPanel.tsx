import React, { useEffect, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useWebSocket } from '../hooks/useWebSocket';
import type { SessionMeta } from '../types/agent';

interface Props {
  onSwitchToChat: () => void;
}

// ── Export helpers ────────────────────────────────────────────────────────────

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

function exportAsMarkdown(messages: ReturnType<typeof useAgentStore.getState>['messages']) {
  const lines: string[] = [`# Agent 对话导出\n\n> 导出时间: ${new Date().toLocaleString()}\n`];
  for (const msg of messages) {
    if (msg.role === 'system') continue;
    const label = msg.role === 'user' ? '**用户**' : '**助手**';
    lines.push(`---\n\n${label}\n\n${msg.content}\n`);
  }
  const blob = new Blob([lines.join('\n')], { type: 'text/markdown;charset=utf-8' });
  downloadBlob(blob, `agent-chat-${Date.now()}.md`);
}

function exportAsJson(messages: ReturnType<typeof useAgentStore.getState>['messages']) {
  const data = {
    exported_at: new Date().toISOString(),
    messages: messages
      .filter(m => m.role !== 'system')
      .map(m => ({ role: m.role, content: m.content, timestamp: m.timestamp })),
  };
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json;charset=utf-8' });
  downloadBlob(blob, `agent-chat-${Date.now()}.json`);
}

// ── Component ─────────────────────────────────────────────────────────────────

export const SessionsPanel: React.FC<Props> = ({ onSwitchToChat }) => {
  const { messages, sessionList } = useAgentStore();
  const { listSessions, deleteSession, loadSessionById, isConnected } = useWebSocket();
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  // Auto-load list when panel becomes active and connected
  useEffect(() => {
    if (isConnected) listSessions();
  }, [isConnected]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleDelete = (id: string) => {
    if (confirmDeleteId === id) {
      deleteSession(id);
      setConfirmDeleteId(null);
    } else {
      setConfirmDeleteId(id);
    }
  };

  const handleLoad = (id: string) => {
    loadSessionById(id);
    onSwitchToChat();
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
      <p style={{ fontSize: '13px', color: 'var(--text3)', marginBottom: '20px' }}>导出当前对话或管理历史会话记录</p>

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
              将聊天记录导出到本地文件
            </p>
          </div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              onClick={() => exportAsMarkdown(messages)}
              disabled={messages.length === 0}
              style={{
                ...btnBase,
                background: messages.length ? 'var(--accent)' : 'var(--bg3)',
                color: messages.length ? '#fff' : 'var(--text3)',
                borderColor: messages.length ? 'var(--accent)' : 'var(--border)',
              }}
            >
              ↓ Markdown
            </button>
            <button
              onClick={() => exportAsJson(messages)}
              disabled={messages.length === 0}
              style={{
                ...btnBase,
                background: 'var(--bg3)', color: messages.length ? 'var(--text)' : 'var(--text3)',
              }}
            >
              ↓ JSON
            </button>
          </div>
        </div>
      </section>

      {/* ── Session history ────────────────────────────────────────────────── */}
      <section>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '10px' }}>
          <p style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
            历史会话 ({sessionList.length})
          </p>
          <button
            onClick={listSessions}
            disabled={!isConnected}
            style={{ ...btnBase, background: 'var(--bg3)', color: isConnected ? 'var(--text2)' : 'var(--text3)', fontSize: '11px' }}
          >
            ↻ 刷新
          </button>
        </div>

        {!isConnected && (
          <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text3)', fontSize: '13px', background: 'var(--bg2)', borderRadius: '10px', border: '1px solid var(--border)' }}>
            连接到 Agent 后可浏览历史会话
          </div>
        )}

        {isConnected && sessionList.length === 0 && (
          <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text3)', fontSize: '13px', background: 'var(--bg2)', borderRadius: '10px', border: '1px solid var(--border)' }}>
            暂无历史会话
          </div>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          {sessionList.map((s: SessionMeta) => (
            <SessionCard
              key={s.id}
              session={s}
              isConfirmDelete={confirmDeleteId === s.id}
              onLoad={() => handleLoad(s.id)}
              onDelete={() => handleDelete(s.id)}
              onCancelDelete={() => setConfirmDeleteId(null)}
              formatDate={formatDate}
              isConnected={isConnected}
            />
          ))}
        </div>
      </section>
    </div>
  );
};

// ── Session card sub-component ────────────────────────────────────────────────

interface CardProps {
  session: SessionMeta;
  isConfirmDelete: boolean;
  onLoad: () => void;
  onDelete: () => void;
  onCancelDelete: () => void;
  formatDate: (s: string) => string;
  isConnected: boolean;
}

const SessionCard: React.FC<CardProps> = ({
  session, isConfirmDelete, onLoad, onDelete, onCancelDelete, formatDate, isConnected,
}) => {
  const btnBase: React.CSSProperties = {
    padding: '4px 10px', borderRadius: '6px', fontSize: '12px',
    fontWeight: '500', cursor: 'pointer', border: '1px solid var(--border)',
  };

  return (
    <div style={{
      background: 'var(--bg2)', border: '1px solid var(--border)',
      borderRadius: '10px', padding: '14px 16px',
      display: 'flex', alignItems: 'flex-start', gap: '12px',
    }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <p style={{
          fontSize: '13px', fontWeight: '500', color: 'var(--text)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          marginBottom: '4px',
        }}>
          {session.summary || '(无摘要)'}
        </p>
        <p style={{
          fontSize: '11px', color: 'var(--text3)', fontFamily: 'monospace',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          marginBottom: '4px',
        }}>
          📂 {session.working_dir}
        </p>
        <div style={{ display: 'flex', gap: '12px', fontSize: '11px', color: 'var(--text3)' }}>
          <span>🕒 {formatDate(session.updated_at)}</span>
          <span>💬 {session.message_count} 条消息</span>
        </div>
      </div>

      <div style={{ display: 'flex', gap: '6px', flexShrink: 0, alignItems: 'center' }}>
        {isConfirmDelete ? (
          <>
            <span style={{ fontSize: '12px', color: 'var(--red)', marginRight: '2px' }}>确认删除?</span>
            <button
              onClick={onDelete}
              style={{ ...btnBase, background: 'var(--red)', color: '#fff', borderColor: 'var(--red)' }}
            >确认</button>
            <button
              onClick={onCancelDelete}
              style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--text2)' }}
            >取消</button>
          </>
        ) : (
          <>
            <button
              onClick={onLoad}
              disabled={!isConnected}
              style={{
                ...btnBase,
                background: isConnected ? 'var(--accent)' : 'var(--bg3)',
                color: isConnected ? '#fff' : 'var(--text3)',
                borderColor: isConnected ? 'var(--accent)' : 'var(--border)',
              }}
            >加载</button>
            <button
              onClick={onDelete}
              style={{ ...btnBase, background: 'var(--bg3)', color: 'var(--red)' }}
            >删除</button>
          </>
        )}
      </div>
    </div>
  );
};
