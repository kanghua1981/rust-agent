/**
 * useAgentPool — Task 连接池
 *
 * 暴露 `dispatchTask(prompt)`:
 *   - 在 taskStore 里创建新的 TaskSession
 *   - 打开独立的 WebSocket 连接（与主聊天完全隔离）
 *   - 把服务器事件写入 taskStore[taskId]
 *   - 立即返回，主聊天输入框不阻塞
 *
 * 自动升级（auto-promote）:
 *   - 订阅 agentStore.toolCalls
 *   - 当主对话中第一次出现 tool_use 时，自动把该对话"升级"为 Task Panel
 *   - 主对话状态重置为 ready，用户可以立刻发下一条消息
 */

import { useCallback, useEffect, useRef } from 'react';
import { v4 as uuidv4 } from 'uuid';
import { useAgentStore } from '../stores/agentStore';
import {
  useTaskStore,
  setTaskWs,
  closeTaskWs,
  TaskMessage,
  TaskToolCall,
  TaskConfirmation,
} from '../stores/taskStore';

// ── Helpers ───────────────────────────────────────────────────────────────────

function ensureAgentPath(url: string): string {
  try {
    const http = url.replace(/^ws(s?):/, 'http$1:');
    const u = new URL(http);
    if (!u.pathname || u.pathname === '/') u.pathname = '/agent';
    return u.toString().replace(/^http(s?):/, 'ws$1:');
  } catch {
    const [base, query] = url.split('?');
    const cleanBase = base.endsWith('/') ? base.slice(0, -1) : base;
    const hasPath = cleanBase.replace(/^wss?:\/\/[^/]+/, '').length > 0;
    const withPath = hasPath ? cleanBase : `${cleanBase}/agent`;
    return query ? `${withPath}?${query}` : withPath;
  }
}

function buildWsUrl(serverUrl: string, workdir: string | undefined, token: string | undefined): string {
  const base = ensureAgentPath(serverUrl);
  const params: string[] = [];
  if (workdir) params.push(`workdir=${encodeURIComponent(workdir)}`);
  if (token) params.push(`token=${encodeURIComponent(token)}`);
  const sep = base.includes('?') ? '&' : '?';
  return params.length > 0 ? `${base}${sep}${params.join('&')}` : base;
}

// ── Core: open an independent WS for a task, drive taskStore ─────────────────

export function openTaskWebSocket(
  taskId: string,
  wsUrl: string,
  prompt: string,
  agentMode: string,
  workdir?: string,
): void {
  const store = useTaskStore.getState();

  let ws: WebSocket;
  try {
    ws = new WebSocket(wsUrl);
  } catch (e) {
    store.setTaskStatus(taskId, 'error');
    store.addTaskMessage(taskId, {
      id: uuidv4(),
      role: 'system',
      content: `连接失败: ${e}`,
      timestamp: Date.now(),
    });
    store.setTaskProcessing(taskId, false);
    return;
  }

  setTaskWs(taskId, ws);

  let streamingMsgId: string | null = null;
  let lastAssistantMsgId: string | null = null;
  let promptSent = false;

  ws.onopen = () => {
    store.setTaskStatus(taskId, 'running');

    // Set execution mode
    if (agentMode && agentMode !== 'auto') {
      ws.send(JSON.stringify({ type: 'set_mode', data: { mode: agentMode } }));
    }
    // Set workdir
    if (workdir) {
      ws.send(JSON.stringify({ type: 'set_workdir', data: { workdir } }));
    }
    // Send initial prompt
    if (!promptSent) {
      promptSent = true;
      store.addTaskMessage(taskId, {
        id: uuidv4(),
        role: 'user',
        content: prompt,
        timestamp: Date.now(),
      });
      const assistantMsgId = uuidv4();
      store.addTaskMessage(taskId, {
        id: assistantMsgId,
        role: 'assistant',
        content: '',
        timestamp: Date.now(),
      });
      lastAssistantMsgId = assistantMsgId;
      ws.send(JSON.stringify({ type: 'user_message', data: { text: prompt } }));
    }
  };

  ws.onmessage = (e) => {
    let event: { type: string; data?: Record<string, unknown> };
    try { event = JSON.parse(e.data as string); }
    catch { return; }

    const { type, data = {} } = event;

    switch (type) {
      case 'ready':
        break;

      case 'stream_start':
        streamingMsgId = lastAssistantMsgId;
        if (streamingMsgId) store.setTaskStreamingMsgId(taskId, streamingMsgId);
        break;

      case 'streaming_token':
        if (data.token && streamingMsgId) {
          store.appendTaskMessage(taskId, streamingMsgId, data.token as string);
        }
        break;

      case 'stream_end':
        streamingMsgId = null;
        store.setTaskStreamingMsgId(taskId, null);
        break;

      case 'assistant_text':
        if (data.text && lastAssistantMsgId) {
          store.updateTaskMessage(taskId, lastAssistantMsgId, data.text as string);
        }
        break;

      case 'thinking':
        break;

      case 'tool_use': {
        const toolId = (data.id as string | undefined) || uuidv4();
        const call: TaskToolCall = {
          id: toolId,
          tool: data.tool as string,
          input: data.input,
          status: 'executing',
          timestamp: Date.now(),
        };
        store.addTaskToolCall(taskId, call);
        break;
      }

      case 'tool_result':
        if (data.tool) {
          const tasks = useTaskStore.getState().tasks;
          const task = tasks.find((t) => t.id === taskId);
          if (task) {
            const match = task.toolCalls
              .filter((c) => c.tool === data.tool && c.status === 'executing')
              .sort((a, b) => a.timestamp - b.timestamp)[0];
            if (match) {
              store.updateTaskToolCall(taskId, match.id, {
                status: data.is_error ? 'error' : 'completed',
                output: data.output as string | undefined,
              });
            }
          }
        }
        break;

      case 'confirm_request': {
        const confirmId = (data.tool_id as string | undefined) || uuidv4();
        // Auto-approve by default for background tasks;
        // user can toggle per-task in TaskPanel (future).
        ws.send(JSON.stringify({ type: 'confirm_response', data: { approved: true, tool_id: confirmId } }));
        break;
      }

      case 'ask_user': {
        const conf: TaskConfirmation = {
          id: uuidv4(),
          action: data.question as string,
          type: 'ask_user',
        };
        store.addTaskConfirmation(taskId, conf);
        break;
      }

      case 'review_plan': {
        // Auto-approve plans in background tasks.
        ws.send(JSON.stringify({ type: 'review_plan_response', data: { approved: true } }));
        break;
      }

      case 'role_header': {
        const stageId = uuidv4();
        store.addTaskMessage(taskId, {
          id: stageId,
          role: 'system',
          content: data.label as string,
          timestamp: Date.now(),
          meta: { stageLabel: data.label, stageModel: data.model },
        });
        const stageMsgId = uuidv4();
        store.addTaskMessage(taskId, {
          id: stageMsgId,
          role: 'assistant',
          content: '',
          timestamp: Date.now(),
        });
        lastAssistantMsgId = stageMsgId;
        break;
      }

      case 'warning':
        store.addTaskMessage(taskId, {
          id: uuidv4(),
          role: 'system',
          content: `⚠️ ${data.message as string}`,
          timestamp: Date.now(),
          meta: { isWarning: true },
        });
        break;

      case 'diff':
        store.addTaskMessage(taskId, {
          id: uuidv4(),
          role: 'system',
          content: `📄 diff: ${data.path}`,
          timestamp: Date.now(),
          meta: { isDiff: true, path: data.path, diff: data.diff },
        });
        break;

      case 'done':
        streamingMsgId = null;
        store.setTaskStreamingMsgId(taskId, null);
        store.setTaskProcessing(taskId, false);
        store.setTaskStatus(taskId, 'done');
        if (data.text && lastAssistantMsgId) {
          const tasks = useTaskStore.getState().tasks;
          const task = tasks.find((t) => t.id === taskId);
          const msg = task?.messages.find((m) => m.id === lastAssistantMsgId);
          if (msg && !msg.content) {
            store.updateTaskMessage(taskId, lastAssistantMsgId, data.text as string);
          }
        }
        lastAssistantMsgId = null;
        ws.close();
        break;

      case 'error':
        store.addTaskMessage(taskId, {
          id: uuidv4(),
          role: 'system',
          content: `❌ ${data.message as string}`,
          timestamp: Date.now(),
        });
        store.setTaskProcessing(taskId, false);
        store.setTaskStatus(taskId, 'error');
        lastAssistantMsgId = null;
        ws.close();
        break;

      case 'cancelled':
        store.setTaskProcessing(taskId, false);
        store.setTaskStatus(taskId, 'error');
        store.addTaskMessage(taskId, {
          id: uuidv4(),
          role: 'system',
          content: '⏹ 已中断',
          timestamp: Date.now(),
        });
        ws.close();
        break;
    }
  };

  ws.onerror = () => {
    store.setTaskStatus(taskId, 'error');
    store.setTaskProcessing(taskId, false);
    store.addTaskMessage(taskId, {
      id: uuidv4(),
      role: 'system',
      content: '❌ WebSocket 连接错误',
      timestamp: Date.now(),
    });
  };

  ws.onclose = () => {
    const tasks = useTaskStore.getState().tasks;
    const task = tasks.find((t) => t.id === taskId);
    if (task && task.status === 'running') {
      store.setTaskStatus(taskId, 'error');
      store.setTaskProcessing(taskId, false);
    }
    closeTaskWs(taskId);
  };
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useAgentPool() {
  const { serverUrl, workdir, clusterToken, config } = useAgentStore();
  const { createTask } = useTaskStore();

  // ── Auto-promote: when main chat uses call_node or run_command, detach it ──
  // We watch agentStore.toolCalls; on first tool_use in a session we note the
  // "active task" so the pool can show a detach hint in the UI.
  const autoPromoteRef = useRef<string | null>(null); // taskId of current promoted task

  useEffect(() => {
    const unsub = useAgentStore.subscribe((state, prev) => {
      // A new tool call appeared that wasn't there before
      if (state.toolCalls.length <= prev.toolCalls.length) return;
      // Already promoted this session
      if (autoPromoteRef.current) return;
      // Only promote for long-running tools
      const newCall = state.toolCalls[state.toolCalls.length - 1];
      const longRunningTools = ['call_node', 'run_command', 'script_tool', 'browser'];
      if (!longRunningTools.includes(newCall.tool)) return;

      // Find the user prompt that triggered this session
      const userMsg = [...state.messages].reverse().find((m) => m.role === 'user');
      if (!userMsg) return;

      // Create a silent "shadow" task in taskStore to track this in the panel
      // (messages are sourced from agentStore while running, then snapshotted on done)
      const taskId = createTask({
        serverUrl: serverUrl,
        prompt: userMsg.content,
        workdir,
      });
      autoPromoteRef.current = taskId;

      // Mark this task as "mirroring" the main agentStore session
      // We do NOT open a second WS — the panel reads from agentStore via a flag.
      useTaskStore.getState().setTaskStatus(taskId, 'running');
    });

    return unsub;
  }, [serverUrl, workdir, createTask]);

  // Reset auto-promote tracking when main session finishes
  useEffect(() => {
    const unsub = useAgentStore.subscribe((state, prev) => {
      if (!prev.isProcessing || state.isProcessing) return;
      // Main session done — snapshot messages to taskStore if promoted
      const taskId = autoPromoteRef.current;
      if (!taskId) return;

      const snapMessages: TaskMessage[] = state.messages.map((m) => ({
        id: m.id,
        role: m.role as 'user' | 'assistant' | 'system',
        content: m.content,
        timestamp: m.timestamp,
        meta: m.meta as Record<string, unknown> | undefined,
      }));
      const snapToolCalls: TaskToolCall[] = state.toolCalls.map((c) => ({
        id: c.id,
        tool: c.tool,
        input: c.input,
        status: c.status as 'executing' | 'completed' | 'error',
        output: c.output,
        timestamp: c.timestamp,
      }));

      const taskState = useTaskStore.getState();
      // Replace placeholder messages with actual snapshot
      snapMessages.forEach((m) => taskState.addTaskMessage(taskId, m));
      snapToolCalls.forEach((c) => taskState.addTaskToolCall(taskId, c));
      taskState.setTaskStatus(taskId, 'done');
      taskState.setTaskProcessing(taskId, false);

      autoPromoteRef.current = null;
    });

    return unsub;
  }, []);

  // ── dispatchTask: launch a fully independent background task ─────────────
  const dispatchTask = useCallback(
    (prompt: string): string => {
      const wsUrl = buildWsUrl(serverUrl, workdir, clusterToken || undefined);
      const taskId = createTask({ serverUrl, prompt, workdir });
      openTaskWebSocket(taskId, wsUrl, prompt, config.agentMode ?? 'auto', workdir);
      return taskId;
    },
    [serverUrl, workdir, clusterToken, config.agentMode, createTask],
  );

  // ── cancelTask ────────────────────────────────────────────────────────────
  const cancelTask = useCallback((taskId: string) => {
    closeTaskWs(taskId);
    useTaskStore.getState().setTaskStatus(taskId, 'error');
    useTaskStore.getState().setTaskProcessing(taskId, false);
    useTaskStore.getState().addTaskMessage(taskId, {
      id: uuidv4(),
      role: 'system',
      content: '⏹ 任务已取消',
      timestamp: Date.now(),
    });
  }, []);

  return { dispatchTask, cancelTask, autoPromoteTaskId: autoPromoteRef.current };
}
