/**
 * TaskPanel — 单个后台 Task 的面板
 *
 * 读取 taskStore 里对应的 TaskSession，
 * 渲染消息流、工具调用状态和任务状态徽章。
 */

import React, { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import { useTaskStore, TaskSession, TaskMessage, TaskToolCall, closeTaskWs } from '../stores/taskStore';
import { exportSessionAsMarkdown, exportSessionAsJson } from '../utils/export';

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
  const label = task.status === 'running' ? elapsed : {
    connecting: '连接中', done: '完成', error: '错误',
  }[task.status];

  return (
    <div className={`task-badge ${task.status}`}>
      <span className="dot" />
      {label}
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
  const args = typeof call.input === 'object' && call.input !== null
    ? JSON.stringify(call.input as object)
    : null;

  return (
    <div className={`task-tool ${call.status}`}>
      <span style={{ flexShrink: 0 }}>{icon}</span>
      <span className="task-tool-body">
        <span className="task-tool-name">{call.tool}</span>
        {args && (
          <span className="task-tool-args">{args.slice(0, 80)}{args.length > 80 ? '…' : ''}</span>
        )}
      </span>
      {call.status === 'executing' && (
        <span className="spin" style={{ color: 'var(--yellow)', fontSize: 13, flexShrink: 0 }}>⟳</span>
      )}
    </div>
  );
};

// ── Message row (memo'd) ─────────────────────────────────────────────────────

interface TaskMessageRowProps {
  msg: TaskMessage;
  isStreaming: boolean;
  /** Pre-built minute-bucket index: Map<minuteBucket, TaskToolCall[]> */
  toolCallIndex: Map<number, TaskToolCall[]>;
}

export const TaskMessageRow = React.memo<TaskMessageRowProps>(({
  msg, isStreaming, toolCallIndex,
}) => {
  if (msg.role === 'system') {
    if (msg.meta?.stageLabel) {
      const stageIcons: Record<string, string> = {
        Planner: '🎯', Executor: '⚡', Checker: '✅', Router: '🔀',
      };
      const icon = stageIcons[msg.meta.stageLabel as string] ?? '🔵';
      return (
        <div className="task-stage">
          <div className="line" />
          <span className="tag">{icon} {msg.meta.stageLabel as string}</span>
          <div className="line" />
        </div>
      );
    }
    return (
      <div className="task-note"><span>{msg.content}</span></div>
    );
  }

  if (msg.role === 'user') {
    return (
      <div className="task-msg-user">
        <div className="task-bubble-user">{msg.content}</div>
      </div>
    );
  }

  // O(1) lookup: check current + adjacent minute-buckets
  const msgMinute = Math.floor(msg.timestamp / 60000);
  let related: TaskToolCall[] = [];
  for (let b = msgMinute - 1; b <= msgMinute + 1; b++) {
    const bucket = toolCallIndex.get(b);
    if (bucket) {
      for (const c of bucket) {
        if (Math.abs(c.timestamp - msg.timestamp) < 60000) {
          related.push(c);
        }
      }
    }
  }

  // ReactMarkdown output cached on content + streaming flag
  const markdownEl = useMemo(
    () => <ReactMarkdown>{msg.content + (isStreaming ? '▋' : '')}</ReactMarkdown>,
    [msg.content, isStreaming],
  );

  return (
    <div className="task-msg-bot">
      <div className="task-avatar">🤖</div>
      <div className="fill">
        {related.length > 0 && (
          <div style={{ marginBottom: 4 }}>
            {related.map((c) => <ToolCallRow key={c.id} call={c} />)}
          </div>
        )}
        {msg.content && <div className="task-bubble-bot">{markdownEl}</div>}
      </div>
    </div>
  );
});
TaskMessageRow.displayName = 'TaskMessageRow';

// ── TaskFocusModal — full-screen expanded view ───────────────────────────────

const TaskFocusModal: React.FC<{ task: TaskSession; onClose: () => void }> = ({ task, onClose }) => {
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const isNearBottom = useRef(true);
  const [exportStatus, setExportStatus] = useState<string | null>(null);

  // 预建工具调用索引：Map<分钟桶, TaskToolCall[]>，使每条消息匹配变为 O(1)
  const toolCallIndex = useMemo(() => {
    const idx = new Map<number, TaskToolCall[]>();
    for (const c of task.toolCalls) {
      const bucket = Math.floor(c.timestamp / 60000);
      const arr = idx.get(bucket);
      if (arr) arr.push(c);
      else idx.set(bucket, [c]);
    }
    return idx;
  }, [task.toolCalls]);

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
  const tone = task.status === 'running' ? 'running' : task.status === 'error' ? 'error' : 'done';
  const cancelTask = () => {
    closeTaskWs(task.id);
    useTaskStore.getState().setTaskStatus(task.id, 'error');
    useTaskStore.getState().setTaskProcessing(task.id, false);
  };

  return (
    <div className="task-focus-backdrop" onClick={onClose}>
      <div className={`task-focus ${tone}`} onClick={(e) => e.stopPropagation()}>
        {/* Modal header */}
        <div className="task-focus-head">
          <div className={`task-focus-dot ${tone}`} />
          <span className="task-focus-title">{task.title}</span>
          <StatusBadge task={task} />
          {!stopped && (
            <button className="btn-cancel" onClick={cancelTask} title="取消任务">■ 取消</button>
          )}
          <button className="task-close-btn" onClick={onClose} title="关闭（Esc）">×</button>
        </div>

        {/* Prompt bar */}
        <div className="task-focus-prompt">
          <span className="lbl">任务：</span>{task.prompt}
        </div>

        {/* Scrollable body */}
        <div
          className="task-focus-body"
          ref={scrollRef}
          onScroll={() => {
            const el = scrollRef.current;
            if (el) isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 200;
          }}
        >
          {task.messages.map((msg) => (
            <TaskMessageRow
              key={msg.id}
              msg={msg}
              isStreaming={task.streamingMessageId === msg.id}
              toolCallIndex={toolCallIndex}
            />
          ))}
          <div ref={bottomRef} />
        </div>

        {/* Footer stat bar */}
        <div className="task-focus-foot">
          <span>消息: {task.messages.filter(m => m.role !== 'system').length}</span>
          <span>工具调用: {task.toolCalls.length}</span>
          <span className="grow">连接: {task.serverUrl}</span>
          {exportStatus && <span className="text-ok" style={{ flexShrink: 0 }}>{exportStatus}</span>}
          <button
            className="btn-tiny"
            onClick={() => exportSessionAsMarkdown({ messages: task.messages, toolCalls: task.toolCalls, extraHeader: `> 任务: ${task.prompt}` }, (p) => { setExportStatus(`✓ ${p}`); setTimeout(() => setExportStatus(null), 3000); }, (e) => setExportStatus(`❌ ${e}`))}
            title="导出为 Markdown"
          >↓ MD</button>
          <button
            className="btn-tiny"
            onClick={() => exportSessionAsJson({ messages: task.messages, toolCalls: task.toolCalls, extraHeader: `> 任务: ${task.prompt}` }, (p) => { setExportStatus(`✓ ${p}`); setTimeout(() => setExportStatus(null), 3000); }, (e) => setExportStatus(`❌ ${e}`))}
            title="导出为 JSON"
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

  // 预建工具调用索引
  const toolCallIndex = useMemo(() => {
    const idx = new Map<number, TaskToolCall[]>();
    for (const c of task.toolCalls) {
      const bucket = Math.floor(c.timestamp / 60000);
      const arr = idx.get(bucket);
      if (arr) arr.push(c);
      else idx.set(bucket, [c]);
    }
    return idx;
  }, [task.toolCalls]);

  const tone = task.status === 'running' ? 'running' : task.status === 'error' ? 'error' : '';
  const cancelTask = (e: React.MouseEvent) => {
    e.stopPropagation();
    closeTaskWs(taskId);
    useTaskStore.getState().setTaskStatus(taskId, 'error');
    useTaskStore.getState().setTaskProcessing(taskId, false);
  };

  return (
    <div className={`task-panel ${tone}`}>
      {/* Header */}
      <div
        className={`task-panel-head${task.collapsed ? ' collapsed' : ''}`}
        onClick={() => toggleTaskCollapsed(taskId)}
      >
        <span className="task-chevron">{task.collapsed ? '▶' : '▼'}</span>
        <span className="task-panel-title">{task.title}</span>
        <StatusBadge task={task} />
        <button className="task-icon-btn" style={{ fontSize: 13 }} onClick={onOpenFocus} title="展开查看详情">⤢</button>
        <button
          className="task-icon-btn" style={{ fontSize: 12 }}
          onClick={(e) => { e.stopPropagation(); exportSessionAsMarkdown({ messages: task.messages, toolCalls: task.toolCalls, extraHeader: `> 任务: ${task.prompt}` }, () => {}, () => {}); }}
          title="导出 Markdown"
        >↓</button>
        {!stopped && (
          <button className="task-icon-btn" style={{ fontSize: 14 }} onClick={cancelTask} title="取消任务">■</button>
        )}
        <button
          className="task-icon-btn" style={{ fontSize: 16 }}
          onClick={(e) => { e.stopPropagation(); onClose(taskId); }}
          title="关闭面板"
        >×</button>
      </div>

      {/* Body */}
      {!task.collapsed && (
        <div
          className="task-panel-body"
          ref={scrollRef}
          onScroll={() => {
            const el = scrollRef.current;
            if (el) isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 150;
          }}
        >
          {task.messages.map((msg) => (
            <TaskMessageRow
              key={msg.id}
              msg={msg}
              isStreaming={task.streamingMessageId === msg.id}
              toolCallIndex={toolCallIndex}
            />
          ))}
          <div ref={bottomRef} />
        </div>
      )}
    </div>
  );
};
