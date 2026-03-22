import { useEffect, useRef, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ClientMessage, ServerEvent, ToolCall } from '../types/agent';
import { v4 as uuidv4 } from 'uuid';

export const useWebSocket = () => {
  const wsRef = useRef<WebSocket | null>(null);
  const streamingMsgIdRef = useRef<string | null>(null);
  const lastAssistantMsgIdRef = useRef<string | null>(null);

  const {
    connectionStatus,
    serverUrl,
    workdir,
    config,
    setConnectionStatus,
    addMessage,
    updateMessage,
    appendToMessage,
    setIsProcessing,
    setStreamingMessageId,
    addToolCall,
    updateToolCall,
    addPendingConfirmation,
    removePendingConfirmation,
    addDiff,
    setSessionInfo,
    setSessionList,
    removeSessionFromList,
    clearSession,
  } = useAgentStore();

  const sendRaw = useCallback((message: ClientMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
      return true;
    }
    return false;
  }, []);

  const sendUserMessage = useCallback((text: string) => {
    const userMsgId = uuidv4();
    addMessage({ id: userMsgId, role: 'user', content: text, timestamp: Date.now() });

    const ok = sendRaw({
      type: 'user_message',
      data: { text, workdir, model: config.model },
      id: userMsgId,
    });

    if (ok) {
      setIsProcessing(true);
      const assistantMsgId = uuidv4();
      addMessage({ id: assistantMsgId, role: 'assistant', content: '', timestamp: Date.now() });
      lastAssistantMsgIdRef.current = assistantMsgId;
      return assistantMsgId;
    }
    return null;
  }, [sendRaw, workdir, config.model, addMessage, setIsProcessing]);

  const confirmToolCall = useCallback((toolId: string, approved: boolean) => {
    sendRaw({ type: 'confirm_response', data: { approved, tool_id: toolId } });
    removePendingConfirmation(toolId);
  }, [sendRaw, removePendingConfirmation]);

  const answerQuestion = useCallback((answer: string) => {
    sendRaw({ type: 'ask_user_response', data: { answer } });
  }, [sendRaw]);

  const reviewPlan = useCallback((approved: boolean, feedback?: string) => {
    sendRaw({ type: 'review_plan_response', data: { approved, feedback } });
  }, [sendRaw]);

  const setSandbox = useCallback((enabled: boolean) => {
    sendRaw({ type: 'set_sandbox', data: { enabled } });
  }, [sendRaw]);

  const setWorkdirRemote = useCallback((newWorkdir: string) => {
    sendRaw({ type: 'set_workdir', data: { workdir: newWorkdir } });
  }, [sendRaw]);

  const setModelRemote = useCallback((model: string) => {
    sendRaw({ type: 'set_model', data: { model } });
  }, [sendRaw]);

  const loadSession = useCallback(() => {
    sendRaw({ type: 'load_session', data: {} });
  }, [sendRaw]);

  const newSession = useCallback(() => {
    sendRaw({ type: 'new_session', data: {} });
  }, [sendRaw]);

  const listSessions = useCallback(() => {
    sendRaw({ type: 'list_sessions', data: {} });
  }, [sendRaw]);

  const deleteSession = useCallback((id: string) => {
    sendRaw({ type: 'delete_session', data: { id } });
  }, [sendRaw]);

  const loadSessionById = useCallback((id: string) => {
    clearSession();
    sendRaw({ type: 'load_session_by_id', data: { id } });
  }, [sendRaw, clearSession]);

  const handleServerEvent = useCallback((event: ServerEvent) => {
    switch (event.type) {
      case 'ready':
        console.log('[ws] agent ready, version:', event.data.version);
        break;

      case 'thinking':
        break;

      case 'stream_start':
        streamingMsgIdRef.current = lastAssistantMsgIdRef.current;
        if (streamingMsgIdRef.current) setStreamingMessageId(streamingMsgIdRef.current);
        break;

      case 'streaming_token':
        if (event.data?.token && streamingMsgIdRef.current) {
          appendToMessage(streamingMsgIdRef.current, event.data.token);
        }
        break;

      case 'stream_end':
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        break;

      case 'assistant_text':
        if (event.data?.text && lastAssistantMsgIdRef.current) {
          updateMessage(lastAssistantMsgIdRef.current, event.data.text);
        }
        break;

      case 'tool_use': {
        const toolId = event.data.id || uuidv4();
        const toolCall: ToolCall = {
          id: toolId,
          tool: event.data.tool,
          input: event.data.input,
          status: 'executing',
          timestamp: Date.now(),
          messageId: lastAssistantMsgIdRef.current ?? undefined,
        };
        addToolCall(toolCall);
        if (!config.autoApprove) {
          addPendingConfirmation({
            id: toolId,
            action: `调用工具: ${event.data.tool}`,
            details: JSON.stringify(event.data.input, null, 2),
            type: 'confirm',
          });
        }
        break;
      }

      case 'tool_result':
        if (event.data?.tool) {
          // Find oldest executing tool call with this name (server sends no id)
          const storeState = useAgentStore.getState();
          const match = storeState.toolCalls
            .filter(c => c.tool === event.data.tool && c.status === 'executing')
            .sort((a, b) => a.timestamp - b.timestamp)[0];
          if (match) {
            updateToolCall(match.id, {
              status: event.data.is_error ? 'error' : 'completed',
              output: event.data.output,
            });
          }
        }
        break;

      case 'confirm_request': {
        const confirmId = event.data.tool_id || uuidv4();
        if (config.autoApprove) {
          // 自动批准，直接回复服务器
          sendRaw({ type: 'confirm_response', data: { approved: true, tool_id: confirmId } });
        } else {
          addPendingConfirmation({
            id: confirmId,
            action: event.data.action,
            details: event.data.details,
            type: 'confirm',
          });
        }
        break;
      }

      case 'ask_user':
        addPendingConfirmation({
          id: uuidv4(),
          action: event.data.question,
          type: 'ask_user',
        });
        break;

      case 'review_plan':
        addPendingConfirmation({
          id: uuidv4(),
          action: '请审阅执行计划',
          details: event.data.plan,
          type: 'review_plan',
        });
        break;

      case 'diff':
        addDiff({ id: uuidv4(), path: event.data.path, diff: event.data.diff, timestamp: Date.now() });
        break;

      case 'done':
        setIsProcessing(false);
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        if (event.data?.text && lastAssistantMsgIdRef.current) {
          const state = useAgentStore.getState();
          const msg = state.messages.find(m => m.id === lastAssistantMsgIdRef.current);
          if (msg && !msg.content) updateMessage(lastAssistantMsgIdRef.current, event.data.text);
        }
        lastAssistantMsgIdRef.current = null;
        break;

      case 'error':
        console.error('[ws] error:', event.data.message);
        setIsProcessing(false);
        addMessage({ id: uuidv4(), role: 'system', content: `错误: ${event.data.message}`, timestamp: Date.now() });
        break;

      case 'warning':
      case 'context_warning':
        console.warn('[ws] warning:', event.data.message);
        break;

      case 'role_header':
        // Pipeline stage banner: insert a styled system message so the user
        // can see which role is now active (Planner / Executor / Checker).
        addMessage({
          id: uuidv4(),
          role: 'system',
          content: `${event.data.label}`,
          timestamp: Date.now(),
          meta: { stageLabel: event.data.label, stageModel: event.data.model },
        });
        // Each pipeline stage gets its own assistant bubble.
        {
          const stageMsgId = uuidv4();
          addMessage({ id: stageMsgId, role: 'assistant', content: '', timestamp: Date.now() });
          lastAssistantMsgIdRef.current = stageMsgId;
        }
        break;

      case 'stage_end':
        // Mark an empty stage bubble as done so it's removed; just clear refs.
        if (lastAssistantMsgIdRef.current) {
          const st = useAgentStore.getState();
          const stageMsg = st.messages.find(m => m.id === lastAssistantMsgIdRef.current);
          // If stage produced no text yet (content empty), drop it so we don't leave blank bubbles.
          if (stageMsg && !stageMsg.content) {
            // Replace with a thin stage-end divider message.
            updateMessage(lastAssistantMsgIdRef.current, '');
          }
        }
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        break;

      case 'pong':
        break;

      case 'session_info':
        setSessionInfo(event.data);
        break;

      case 'sessions_list':
        setSessionList(event.data.sessions);
        break;

      case 'session_deleted':
        removeSessionFromList(event.data.id);
        break;

      case 'session_cleared':
        // Clear local frontend state when server starts new session
        clearSession();
        break;

      case 'session_restored': {
        // Populate the chat with the restored history.
        const { messages: restored } = event.data;
        restored.forEach((m: { id: string; role: string; content: string }) => {
          addMessage({
            id: m.id,
            role: m.role as 'user' | 'assistant' | 'system',
            content: m.content,
            timestamp: Date.now(),
          });
        });
        // Reset last assistant msg ref so next stream attaches correctly.
        lastAssistantMsgIdRef.current = null;
        break;
      }
    }
  }, [
    config.autoApprove, sendRaw, appendToMessage, updateMessage, addMessage,
    addToolCall, updateToolCall, addPendingConfirmation, addDiff,
    setIsProcessing, setStreamingMessageId, setSessionInfo, setSessionList, removeSessionFromList, clearSession,
  ]);

  // Sync execution mode to server whenever it changes or connection is established.
  const agentMode = config.agentMode ?? 'auto';
  useEffect(() => {
    if (connectionStatus === 'connected') {
      sendRaw({ type: 'set_mode', data: { mode: agentMode as 'auto' | 'simple' | 'plan' | 'pipeline' } });
      // 连接建立时发送工作目录
      if (workdir) {
        sendRaw({ type: 'set_workdir', data: { workdir } });
      }
    }
  }, [agentMode, connectionStatus, sendRaw, workdir]);

  const connect = useCallback(() => {
    wsRef.current?.close();
    setConnectionStatus('connecting');
    try {
      const ws = new WebSocket(serverUrl);
      wsRef.current = ws;
      ws.onopen = () => { setConnectionStatus('connected'); };
      ws.onclose = () => { setConnectionStatus('disconnected'); streamingMsgIdRef.current = null; setStreamingMessageId(null); setIsProcessing(false); };
      ws.onerror = () => { setConnectionStatus('error'); };
      ws.onmessage = (e) => {
        try { handleServerEvent(JSON.parse(e.data) as ServerEvent); }
        catch (err) { console.error('[ws] parse error:', err); }
      };
    } catch (err) {
      setConnectionStatus('error');
      console.error('[ws] connect failed:', err);
    }
  }, [serverUrl, setConnectionStatus, handleServerEvent, setIsProcessing, setStreamingMessageId]);

  const disconnect = useCallback(() => {
    // Guard: only update global state if this hook instance actually owns a WebSocket.
    // Multiple components (SettingsPanel, SessionsPanel, etc.) each call useWebSocket()
    // and get their own wsRef (initially null). Without this guard, when those components
    // unmount their cleanup calls disconnect() which would unconditionally set the global
    // connectionStatus to 'disconnected' even though the real WS (owned by App.tsx) is
    // still open.
    if (!wsRef.current) return;
    wsRef.current.close();
    wsRef.current = null;
    setConnectionStatus('disconnected');
    setSessionInfo(null);
  }, [setConnectionStatus, setSessionInfo]);

  useEffect(() => () => { disconnect(); }, [disconnect]);

  return {
    connectionStatus,
    connect,
    disconnect,
    sendUserMessage,
    confirmToolCall,
    answerQuestion,
    reviewPlan,
    setSandbox,
    setWorkdirRemote,
    setModelRemote,
    loadSession,
    newSession,
    listSessions,
    deleteSession,
    loadSessionById,
    isConnected: connectionStatus === 'connected',
  };
};
