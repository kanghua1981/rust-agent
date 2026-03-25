/**
 * taskStore — 管理所有独立运行的 Task 会话
 *
 * 每个 Task 拥有：
 *  - 独立的 WebSocket 连接（引用保存在 _wsSessions Map 里，不持久化）
 *  - 独立的消息流 / 工具调用 / 确认队列
 *  - 状态机: connecting → running → done | error
 *
 * 主对话（agentStore）完全不受影响，两套状态完全隔离。
 */

import { create } from 'zustand';
import { v4 as uuidv4 } from 'uuid';

// ── Types ─────────────────────────────────────────────────────────────────────

export type TaskStatus = 'connecting' | 'running' | 'done' | 'error';

export interface TaskMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  meta?: Record<string, unknown>;
}

export interface TaskToolCall {
  id: string;
  tool: string;
  input: unknown;
  status: 'executing' | 'completed' | 'error';
  output?: string;
  timestamp: number;
}

export interface TaskConfirmation {
  id: string;
  action: string;
  details?: string;
  type: 'confirm' | 'ask_user' | 'review_plan';
}

export interface TaskSession {
  id: string;
  title: string;           // 第一条用户消息摘要
  status: TaskStatus;
  messages: TaskMessage[];
  toolCalls: TaskToolCall[];
  pendingConfirmations: TaskConfirmation[];
  isProcessing: boolean;
  streamingMessageId: string | null;
  startedAt: number;
  endedAt?: number;
  collapsed: boolean;
  serverUrl: string;       // ws:// URL（用于面板 header 显示）
  prompt: string;          // 原始 prompt
}

// ── Store ─────────────────────────────────────────────────────────────────────

interface TaskStoreState {
  tasks: TaskSession[];

  // Lifecycle
  createTask: (params: { serverUrl: string; prompt: string; workdir?: string }) => string;
  removeTask: (id: string) => void;

  // Status
  setTaskStatus: (id: string, status: TaskStatus) => void;

  // Messages
  addTaskMessage: (id: string, msg: TaskMessage) => void;
  updateTaskMessage: (id: string, msgId: string, content: string) => void;
  appendTaskMessage: (id: string, msgId: string, token: string) => void;

  // Tool calls
  addTaskToolCall: (id: string, call: TaskToolCall) => void;
  updateTaskToolCall: (id: string, callId: string, updates: Partial<TaskToolCall>) => void;

  // Confirmations
  addTaskConfirmation: (id: string, conf: TaskConfirmation) => void;
  removeTaskConfirmation: (id: string, confId: string) => void;

  // Streaming
  setTaskStreamingMsgId: (id: string, msgId: string | null) => void;
  setTaskProcessing: (id: string, val: boolean) => void;

  // UI
  toggleTaskCollapsed: (id: string) => void;
}

export const useTaskStore = create<TaskStoreState>((set) => ({
  tasks: [],

  createTask: ({ serverUrl, prompt, workdir: _workdir }) => {
    const id = uuidv4();
    const title = prompt.length > 60 ? prompt.slice(0, 57) + '…' : prompt;
    const task: TaskSession = {
      id,
      title,
      status: 'connecting',
      messages: [],
      toolCalls: [],
      pendingConfirmations: [],
      isProcessing: true,
      streamingMessageId: null,
      startedAt: Date.now(),
      collapsed: false,
      serverUrl,
      prompt,
    };
    set((s) => ({ tasks: [...s.tasks, task] }));
    return id;
  },

  removeTask: (id) =>
    set((s) => ({ tasks: s.tasks.filter((t) => t.id !== id) })),

  setTaskStatus: (id, status) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? { ...t, status, endedAt: status === 'done' || status === 'error' ? Date.now() : t.endedAt }
          : t,
      ),
    })),

  addTaskMessage: (id, msg) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id ? { ...t, messages: [...t.messages, msg] } : t,
      ),
    })),

  updateTaskMessage: (id, msgId, content) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? { ...t, messages: t.messages.map((m) => (m.id === msgId ? { ...m, content } : m)) }
          : t,
      ),
    })),

  appendTaskMessage: (id, msgId, token) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? {
              ...t,
              messages: t.messages.map((m) =>
                m.id === msgId ? { ...m, content: m.content + token } : m,
              ),
            }
          : t,
      ),
    })),

  addTaskToolCall: (id, call) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id ? { ...t, toolCalls: [...t.toolCalls, call] } : t,
      ),
    })),

  updateTaskToolCall: (id, callId, updates) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? {
              ...t,
              toolCalls: t.toolCalls.map((c) => (c.id === callId ? { ...c, ...updates } : c)),
            }
          : t,
      ),
    })),

  addTaskConfirmation: (id, conf) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? { ...t, pendingConfirmations: [...t.pendingConfirmations, conf] }
          : t,
      ),
    })),

  removeTaskConfirmation: (id, confId) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id
          ? { ...t, pendingConfirmations: t.pendingConfirmations.filter((c) => c.id !== confId) }
          : t,
      ),
    })),

  setTaskStreamingMsgId: (id, msgId) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id ? { ...t, streamingMessageId: msgId } : t,
      ),
    })),

  setTaskProcessing: (id, val) =>
    set((s) => ({
      tasks: s.tasks.map((t) => (t.id === id ? { ...t, isProcessing: val } : t)),
    })),

  toggleTaskCollapsed: (id) =>
    set((s) => ({
      tasks: s.tasks.map((t) =>
        t.id === id ? { ...t, collapsed: !t.collapsed } : t,
      ),
    })),
}));

// ── Live WS session map (outside React, no re-render overhead) ────────────────

const _wsSessions = new Map<string, WebSocket>();

export function getTaskWs(taskId: string): WebSocket | undefined {
  return _wsSessions.get(taskId);
}

export function setTaskWs(taskId: string, ws: WebSocket): void {
  _wsSessions.set(taskId, ws);
}

export function closeTaskWs(taskId: string): void {
  const ws = _wsSessions.get(taskId);
  if (ws && ws.readyState !== WebSocket.CLOSED) ws.close();
  _wsSessions.delete(taskId);
}
