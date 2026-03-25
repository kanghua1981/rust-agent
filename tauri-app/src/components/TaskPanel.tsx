/**
 * TaskPanel — 单个后台 Task 的面板
 *
 * 读取 taskStore 里对应的 TaskSession，
 * 渲染消息流、工具调用状态和任务状态徽章。
 */

import React, { useEffect, useRef, useState, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import { useTaskStore, TaskSession, TaskMessage, TaskToolCall, closeTaskWs } from '../stores/taskStore';

// ── Export helpers (mirrors SessionsPanel logic, adapted for TaskMessage[]) ─────

const isTauri = () => typeof window !== 'undefined' && '__TAURI__' in window;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const tauriInvoke = (): ((cmd: string, args?: Record<string, unknown>) => Promise<any>) | null => {
  if (!isTauri()) return null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (window as any).__TAURI__.core?.invoke ?? (window as any).__TAURI__.tauri?.invoke ?? null;
};
function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a'); a.href = url; a.download = filename; a.click();
  URL.revokeObjectURL(url);
}
async function saveViaTauri(content: string, filename: string): Promise<string> {
  const invoke = tauriInvoke()!;
  const homeDir = await invoke('get_home_dir');
  const filePath = `${homeDir}/Downloads/${filename}`;
  await invoke('write_file', { path: filePath, content });
  return filePath;
}

async function exportTaskAsMarkdown(task: TaskSession, onSaved: (p: string) => void, onError: (e: string) => void) {
  const lines: string[] = [
    `# 任务会话导出\n\n> 任务: ${task.prompt}\n> 导出时间: ${new Date().toLocaleString()}\n`,
  ];
  for (const msg of task.messages) {
    if (msg.role === 'system') continue;
    const label = msg.role === 'user' ? '**用户**' : '**助手**';
    lines.push(`---\n\n${label}\n\n${msg.content}\n`);
  }
  if (task.toolCalls.length) {
    lines.push(`---\n\n## 工具调用记录\n`);
    for (const c of task.toolCalls) {
      lines.push(`- **${c.tool}** (${c.status}): \`${JSON.stringify(c.input).slice(0, 120)}\`\n`);
    }
  }
  const filename = `task-${Date.now()}.md`;
  const text = lines.join('\n');
  if (isTauri()) { try { onSaved(await saveViaTauri(text, filename)); } catch(e) { onError(String(e)); } }
  else { downloadBlob(new Blob([text], { type: 'text/markdown;charset=utf-8' }), filename); onSaved(filename); }
}

async function exportTaskAsJson(task: TaskSession, onSaved: (p: string) => void, onError: (e: string) => void) {
  const data = {
    exported_at: new Date().toISOString(),
    prompt: task.prompt,
    status: task.status,
    server: task.serverUrl,
    started_at: new Date(task.startedAt).toISOString(),
    ended_at: task.endedAt ? new Date(task.endedAt).toISOString() : null,
    messages: task.messages
      .filter(m => m.role !== 'system')
      .map(m => ({ role: m.role, content: m.content, timestamp: m.timestamp })),
    tool_calls: task.toolCalls.map(c => ({ tool: c.tool, status: c.status, input: c.input, output: c.output })),
  };
  const filename = `task-${Date.now()}.json`;
  const text = JSON.stringify(data, null, 2);
  if (isTauri()) { try { onSaved(await saveViaTauri(text, filename)); } catch(e) { onError(String(e)); } }
  else { downloadBlob(new Blob([text], { type: 'application/json;charset=utf-8' }), filename); onSaved(filename); }
}

// ── Elapsed timer helper ──────────────────────────────────────────────────────

function useElapsed(startedAt: number, stopped: boolean): string {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (stopped) return;
    const timer = setInterval(() => setElapsed(Date.now() - startedAt), 1000);
    return () => clearInterval(timer);
  }, [startedAt, stopped]);
  const total = stopped ? 0 : elapsed;
  const m = Math.floor(total / 60000);
  const s = Math.floor((total % 60000) / 1000);
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

// ── Status badge ──────────────────────────────────────────────────────────────

const StatusBadge: React.FC<{ task: TaskSession }> = ({ task }) => {
  const stopped = task.status === 'done' || task.status === 'error';
  const elapsed = useElapsed(task.startedAt, stopped);

  const cfg = {
    connecting: { color: '#f59e0b', dot: '#f59e0b', label: '连接中' },
    running:    { color: '#10b981', dot: '#10b981', label: elapsed },
    done:       { color: '#6b7280', dot: '#10b981', label: '完成' },
    error:      { color: '#ef4444', dot: '#ef4444', label: '错误' },
  }[task.status];

  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: '5px',
      fontSize: '11px', color: cfg.color,
      background: 'var(--bg3)',
      border: `1px solid ${cfg.color}33`,
      borderRadius: '12px', padding: '2px 8px',
      flexShrink: 0,
    }}>
      <span style={{
        width: '6px', height: '6px', borderRadius: '50%',
        background: cfg.dot,
        boxShadow: task.status === 'running' ? `0 0 5px ${cfg.dot}` : 'none',
        display: 'inline-block',
      }} />
      {cfg.label}
    </div>
  );
};

// ── Mini tool call row ────────────────────────────────────────────────────────

const ToolCallRow: React.FC<{ call: TaskToolCall }> = ({ call }) => {
  const iconMap: Record<string, string> = {
    read_file: '📖', write_file: '✏️', edit_file: '✏️',
    run_command: '🔨', search: '🔍', list_dir: '📂',
    call_node: '🤝', browser: '🌐', think: '💭',
    multi_edit_file: '✏️', batch_read: '📖',
  };
  const icon = iconMap[call.tool] ?? '🔧';
  const statusColor = { executing: '#f59e0b', completed: '#10b981', error: '#ef4444' }[call.status];

  return (
    <div style={{
      display: 'flex', alignItems: 'flex-start', gap: '6px',
      padding: '4px 8px',
      background: 'var(--bg3)',
      borderRadius: '6px',
      margin: '2px 0',
      fontSize: '12px',
      borderLeft: `2px solid ${statusColor}`,
    }}>
      <span style={{ flexShrink: 0 }}>{icon}</span>
      <span style={{ color: 'var(--text2)', flex: 1, wordBreak: 'break-all' }}>
        <span style={{ color: 'var(--accent)', fontWeight: '500' }}>{call.tool}</span>
        {typeof call.input === 'object' && call.input !== null && (
          <span style={{ color: 'var(--text3)', marginLeft: '4px' }}>
            {JSON.stringify(call.input as object).slice(0, 80)}
            {JSON.stringify(call.input as object).length > 80 ? '…' : ''}
          </span>
        )}
      </span>
      {call.status === 'executing' && (
        <span className="spin" style={{ color: '#f59e0b', fontSize: '13px', flexShrink: 0 }}>⟳</span>
      )}
    </div>
  );
};

// ── Message row ───────────────────────────────────────────────────────────────

const TaskMessageRow: React.FC<{ msg: TaskMessage; isStreaming: boolean; toolCalls: TaskToolCall[] }> = ({
  msg, isStreaming, toolCalls,
}) => {
  if (msg.role === 'system') {
    if (msg.meta?.stageLabel) {
      const stageIcons: Record<string, string> = {
        Planner: '🎯', Executor: '⚡', Checker: '✅', Router: '🔀',
      };
      const icon = stageIcons[msg.meta.stageLabel as string] ?? '🔵';
      return (
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px', padding: '6px 0 2px' }}>
          <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
          <span style={{
            fontSize: '10px', fontWeight: '600', color: 'var(--accent)',
            background: 'rgba(99,102,241,0.1)', border: '1px solid rgba(99,102,241,0.3)',
            borderRadius: '10px', padding: '1px 8px',
          }}>{icon} {msg.meta.stageLabel as string}</span>
          <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
        </div>
      );
    }
    return (
      <div style={{ display: 'flex', justifyContent: 'center', padding: '2px 0' }}>
        <span style={{
          fontSize: '11px', color: 'var(--text3)',
          background: 'var(--bg3)', border: '1px solid var(--border)',
          borderRadius: '16px', padding: '2px 10px',
        }}>{msg.content}</span>
      </div>
    );
  }

  if (msg.role === 'user') {
    return (
      <div style={{ display: 'flex', justifyContent: 'flex-end', padding: '4px 0' }}>
        <div style={{
          maxWidth: '85%', background: 'var(--accent)',
          color: '#fff', borderRadius: '12px 12px 0 12px',
          padding: '8px 12px', fontSize: '13px', lineHeight: '1.5',
        }}>
          {msg.content}
        </div>
      </div>
    );
  }

  // Assistant message
  const related = toolCalls.filter(
    (c) => Math.abs(c.timestamp - msg.timestamp) < 60000,
  );

  return (
    <div style={{ display: 'flex', gap: '8px', padding: '4px 0', alignItems: 'flex-start' }}>
      <div style={{
        width: '24px', height: '24px', borderRadius: '50%', flexShrink: 0,
        background: 'linear-gradient(135deg, #0ea5e9, #6366f1)',
        display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '11px',
      }}>🤖</div>
      <div style={{ flex: 1, minWidth: 0 }}>
        {related.length > 0 && (
          <div style={{ marginBottom: '4px' }}>
            {related.map((c) => <ToolCallRow key={c.id} call={c} />)}
          </div>
        )}
        {msg.content && (
          <div style={{
            background: 'var(--surface)',
            borderRadius: '0 12px 12px 12px',
            padding: '8px 12px',
            fontSize: '13px',
            lineHeight: '1.6',
            color: 'var(--text)',
          }}>
            <ReactMarkdown>{msg.content + (isStreaming ? '▋' : '')}</ReactMarkdown>
          </div>
        )}
      </div>
    </div>
  );
};

// ── TaskFocusModal — full-screen expanded view ───────────────────────────────

const TaskFocusModal: React.FC<{ task: TaskSession; onClose: () => void }> = ({ task, onClose }) => {
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const isNearBottom = useRef(true);
  const [exportStatus, setExportStatus] = useState<string | null>(null);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);

  // Auto-scroll
  useEffect(() => {
    if (isNearBottom.current) bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [task.messages.length, task.toolCalls.length]);

  const stopped = task.status === 'done' || task.status === 'error';
  const accentColor = task.status === 'running' ? '#10b981' : task.status === 'error' ? '#ef4444' : '#6b7280';

  return (
    <div
      style={{
        position: 'fixed', inset: 0, zIndex: 1000,
        background: 'rgba(0,0,0,0.6)',
        backdropFilter: 'blur(4px)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        padding: '24px',
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: '100%', maxWidth: '860px', height: '100%', maxHeight: '86vh',
          display: 'flex', flexDirection: 'column',
          background: 'var(--bg2)',
          border: `1px solid ${accentColor}55`,
          borderRadius: '14px',
          overflow: 'hidden',
          boxShadow: `0 0 40px ${accentColor}22, 0 24px 60px rgba(0,0,0,0.5)`,
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal header */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: '10px',
          padding: '14px 18px',
          background: 'var(--bg3)',
          borderBottom: '1px solid var(--border)',
          flexShrink: 0,
        }}>
          <div style={{
            width: '8px', height: '8px', borderRadius: '50%',
            background: accentColor,
            boxShadow: task.status === 'running' ? `0 0 8px ${accentColor}` : 'none',
            flexShrink: 0,
          }} />
          <span style={{ flex: 1, fontSize: '14px', fontWeight: '600', color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {task.title}
          </span>
          <StatusBadge task={task} />
          {!stopped && (
            <button
              onClick={() => { closeTaskWs(task.id); useTaskStore.getState().setTaskStatus(task.id, 'error'); useTaskStore.getState().setTaskProcessing(task.id, false); }}
              title="取消任务"
              style={{
                background: 'rgba(239,68,68,0.1)', border: '1px solid rgba(239,68,68,0.3)',
                color: '#ef4444', fontSize: '13px', borderRadius: '6px',
                cursor: 'pointer', padding: '3px 8px', flexShrink: 0,
              }}
            >■ 取消</button>
          )}
          <button
            onClick={onClose}
            title="关闭（Esc）"
            style={{
              background: 'var(--bg3)', border: '1px solid var(--border)',
              color: 'var(--text2)', fontSize: '18px', borderRadius: '8px',
              cursor: 'pointer', padding: '2px 8px', flexShrink: 0, lineHeight: 1,
            }}
          >×</button>
        </div>

        {/* Prompt bar */}
        <div style={{
          padding: '10px 18px 8px',
          borderBottom: '1px solid var(--border)',
          flexShrink: 0,
          fontSize: '12px', color: 'var(--text3)',
        }}>
          <span style={{ color: 'var(--text2)' }}>任务：</span>{task.prompt}
        </div>

        {/* Scrollable body */}
        <div
          ref={scrollRef}
          onScroll={() => {
            const el = scrollRef.current;
            if (el) isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 200;
          }}
          style={{
            flex: 1, overflowY: 'auto',
            padding: '16px 20px',
            display: 'flex', flexDirection: 'column', gap: '4px',
          }}
        >
          {task.messages.map((msg) => (
            <TaskMessageRow
              key={msg.id}
              msg={msg}
              isStreaming={task.streamingMessageId === msg.id}
              toolCalls={task.toolCalls}
            />
          ))}
          <div ref={bottomRef} />
        </div>

        {/* Footer stat bar */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: '16px',
          padding: '8px 18px',
          borderTop: '1px solid var(--border)',
          background: 'var(--bg3)',
          flexShrink: 0,
          fontSize: '11px', color: 'var(--text3)',
        }}>
          <span>消息: {task.messages.filter(m => m.role !== 'system').length}</span>
          <span>工具调用: {task.toolCalls.length}</span>
          <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>连接: {task.serverUrl}</span>
          {exportStatus && (
            <span style={{ color: '#10b981', flexShrink: 0 }}>{exportStatus}</span>
          )}
          <button
            onClick={() => exportTaskAsMarkdown(task, (p) => { setExportStatus(`✓ ${p}`); setTimeout(() => setExportStatus(null), 3000); }, (e) => setExportStatus(`❌ ${e}`))}
            title="导出为 Markdown"
            style={{
              background: 'var(--bg2)', border: '1px solid var(--border)',
              color: 'var(--text2)', borderRadius: '6px',
              fontSize: '11px', padding: '3px 8px', cursor: 'pointer', flexShrink: 0,
            }}
          >↓ MD</button>
          <button
            onClick={() => exportTaskAsJson(task, (p) => { setExportStatus(`✓ ${p}`); setTimeout(() => setExportStatus(null), 3000); }, (e) => setExportStatus(`❌ ${e}`))}
            title="导出为 JSON"
            style={{
              background: 'var(--bg2)', border: '1px solid var(--border)',
              color: 'var(--text2)', borderRadius: '6px',
              fontSize: '11px', padding: '3px 8px', cursor: 'pointer', flexShrink: 0,
            }}
          >↓ JSON</button>
          <span style={{ flexShrink: 0 }}>Esc 关闭</span>
        </div>
      </div>
    </div>
  );
};

// ── TaskPanel ─────────────────────────────────────────────────────────────────

interface Props {
  taskId: string;
  onClose: (id: string) => void;
}

export const TaskPanel: React.FC<Props> = ({ taskId, onClose }) => {
  const task = useTaskStore((s) => s.tasks.find((t) => t.id === taskId));
  const [focused, setFocused] = useState(false);
  const openFocus = useCallback((e: React.MouseEvent) => { e.stopPropagation(); setFocused(true); }, []);
  const closeFocus = useCallback(() => setFocused(false), []);

  if (!task) return null;

  const stopped = task.status === 'done' || task.status === 'error';

  return (
    <>
      {focused && <TaskFocusModal task={task} onClose={closeFocus} />}
      <TaskPanelInner task={task} taskId={taskId} onClose={onClose} stopped={stopped}
        onOpenFocus={openFocus} />
    </>
  );
};

const TaskPanelInner: React.FC<{
  task: TaskSession; taskId: string; stopped: boolean;
  onClose: (id: string) => void;
  onOpenFocus: (e: React.MouseEvent) => void;
}> = ({ task, taskId, stopped, onClose, onOpenFocus }) => {
  const { toggleTaskCollapsed } = useTaskStore();
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const isNearBottom = useRef(true);

  useEffect(() => {
    if (isNearBottom.current) bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [task.messages.length, task.toolCalls.length]);

  const panelBorder = task.status === 'running'
    ? '1px solid rgba(16,185,129,0.4)'
    : task.status === 'error'
    ? '1px solid rgba(239,68,68,0.3)'
    : '1px solid var(--border)';

  return (
    <div style={{
      display: 'flex', flexDirection: 'column',
      background: 'var(--bg2)',
      border: panelBorder,
      borderRadius: '10px',
      overflow: 'hidden',
      boxShadow: task.status === 'running' ? '0 0 12px rgba(16,185,129,0.1)' : 'none',
      transition: 'box-shadow 0.3s',
    }}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: '8px',
        padding: '8px 12px',
        background: 'var(--bg3)',
        borderBottom: task.collapsed ? 'none' : '1px solid var(--border)',
        cursor: 'pointer',
        userSelect: 'none',
      }}
        onClick={() => toggleTaskCollapsed(taskId)}
      >
        <span style={{ fontSize: '12px', color: 'var(--text3)', flexShrink: 0 }}>
          {task.collapsed ? '▶' : '▼'}
        </span>
        <span style={{
          flex: 1, fontSize: '12px', fontWeight: '500', color: 'var(--text)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>
          {task.title}
        </span>
        <StatusBadge task={task} />
        <button
          onClick={onOpenFocus}
          title="展开查看详情"
          style={{
            background: 'transparent', border: 'none',
            color: 'var(--text3)', fontSize: '13px',
            cursor: 'pointer', padding: '0 2px', flexShrink: 0,
            lineHeight: 1,
          }}
        >⤢</button>
        <button
          onClick={(e) => { e.stopPropagation(); exportTaskAsMarkdown(task, () => {}, () => {}); }}
          title="导出导出 Markdown"
          style={{
            background: 'transparent', border: 'none',
            color: 'var(--text3)', fontSize: '12px',
            cursor: 'pointer', padding: '0 2px', flexShrink: 0,
            lineHeight: 1,
          }}
        >↓</button>
        {!stopped && (
          <button
            onClick={(e) => { e.stopPropagation(); closeTaskWs(taskId); useTaskStore.getState().setTaskStatus(taskId, 'error'); useTaskStore.getState().setTaskProcessing(taskId, false); }}
            title="取消任务"
            style={{
              background: 'transparent', border: 'none',
              color: 'var(--text3)', fontSize: '14px',
              cursor: 'pointer', padding: '0 2px', flexShrink: 0,
              lineHeight: 1,
            }}
          >■</button>
        )}
        <button
          onClick={(e) => { e.stopPropagation(); onClose(taskId); }}
          title="关闭面板"
          style={{
            background: 'transparent', border: 'none',
            color: 'var(--text3)', fontSize: '16px',
            cursor: 'pointer', padding: '0 2px', flexShrink: 0,
            lineHeight: 1,
          }}
        >×</button>
      </div>

      {/* Body */}
      {!task.collapsed && (
        <div
          ref={scrollRef}
          onScroll={() => {
            const el = scrollRef.current;
            if (el) isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 150;
          }}
          style={{
            flex: 1,
            maxHeight: '320px',
            overflowY: 'auto',
            padding: '10px 12px',
            display: 'flex',
            flexDirection: 'column',
            gap: '2px',
          }}
        >
          {task.messages.map((msg) => (
            <TaskMessageRow
              key={msg.id}
              msg={msg}
              isStreaming={task.streamingMessageId === msg.id}
              toolCalls={task.toolCalls}
            />
          ))}
          <div ref={bottomRef} />
        </div>
      )}
    </div>
  );
};
