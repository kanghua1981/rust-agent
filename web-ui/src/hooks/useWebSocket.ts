import { useEffect, useRef, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ClientMessage, ServerEvent, ToolCall } from '../types/agent';
import { v4 as uuidv4 } from 'uuid';

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// Ensure the WebSocket URL targets /agent. Mirrors the Rust with_path() helper.
function ensureAgentPath(url: string): string {
  try {
    const http = url.replace(/^ws(s?):/, 'http$1:');
    const u = new URL(http);
    if (!u.pathname || u.pathname === '/') {
      u.pathname = '/agent';
    }
    return u.toString().replace(/^http(s?):/, 'ws$1:');
  } catch {
    const [base, query] = url.split('?');
    const cleanBase = base.endsWith('/') ? base.slice(0, -1) : base;
    const hasPath = cleanBase.replace(/^wss?:\/\/[^/]+/, '').length > 0;
    const withPath = hasPath ? cleanBase : `${cleanBase}/agent`;
    return query ? `${withPath}?${query}` : withPath;
  }
}

// ── Per-slot WebSocket connection state ──
// Each connection slot gets its own live WebSocket, so switching tabs
// doesn't disconnect — it just swaps which slot's data is displayed.
interface SlotConn {
  ws: WebSocket;
  streamingMsgId: string | null;
  thinkingMsgId: string | null;
  lastAssistantMsgId: string | null;
  tokenBuf: string;
  thinkingBuf: string;
  flushTimer: ReturnType<typeof setTimeout> | null;
  /** Timer that flushes buffered tokens for this *inactive* slot vua _updateSlot */
  inactiveFlushTimer: ReturnType<typeof setTimeout> | null;
  /** Flag: a done/error/cancelled event has been received; stop queueing */
  inactiveTerminated: boolean;
}

export const useWebSocket = () => {
  // ── All live connections, keyed by slot ID ──
  const connMapRef = useRef<Map<string, SlotConn>>(new Map());

  // ── Global refs — always point to the *active* connection's state ──
  const streamingMsgIdRef = useRef<string | null>(null);
  const thinkingMsgIdRef = useRef<string | null>(null);
  const lastAssistantMsgIdRef = useRef<string | null>(null);
  const tokenBufRef = useRef<string>('');
  const thinkingBufRef = useRef<string>('');
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Helpers: sync global refs to/from a specific connection ──
  // These are only needed for tab switching — save/restore the token buffering state
  // that lives outside the zustand store.
  const saveRefsToConn = (conn: SlotConn) => {
    conn.streamingMsgId = streamingMsgIdRef.current;
    conn.thinkingMsgId = thinkingMsgIdRef.current;
    conn.lastAssistantMsgId = lastAssistantMsgIdRef.current;
    conn.tokenBuf = tokenBufRef.current;
    conn.thinkingBuf = thinkingBufRef.current;
    if (flushTimerRef.current) { clearTimeout(flushTimerRef.current); flushTimerRef.current = null; }
    conn.flushTimer = null;
  };

  const loadRefsFromConn = (conn: SlotConn) => {
    // Flush any pending tokens for the outgoing connection first
    if (flushTimerRef.current) { clearTimeout(flushTimerRef.current); flushTimerRef.current = null; }
    streamingMsgIdRef.current = conn.streamingMsgId;
    thinkingMsgIdRef.current = conn.thinkingMsgId;
    lastAssistantMsgIdRef.current = conn.lastAssistantMsgId;
    tokenBufRef.current = conn.tokenBuf;
    thinkingBufRef.current = conn.thinkingBuf;
    // Don't restore a timer — it would fire in the wrong context
  };

  // ── Token flushing (operates on global refs = active connection) ──
  const flushTokens = useCallback(() => {
    if (flushTimerRef.current) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    const textToken = tokenBufRef.current;
    const thinkToken = thinkingBufRef.current;
    tokenBufRef.current = '';
    thinkingBufRef.current = '';
    const st = useAgentStore.getState();
    const sId = streamingMsgIdRef.current;
    const tId = thinkingMsgIdRef.current;
    if (textToken && sId) {
      st.appendToMessage(sId, textToken);
    }
    if (thinkToken && tId) {
      st.appendToThinking(tId, thinkToken);
    }
  }, []);

  const scheduleFlush = useCallback(() => {
    if (!flushTimerRef.current) {
      flushTimerRef.current = setTimeout(flushTokens, 50);
    }
  }, [flushTokens]);

  // ── Helpers for store access ──
  const getActiveSlotId = () => useAgentStore.getState().activeConnectionId;
  const getActiveConn = (): SlotConn | undefined => {
    const id = getActiveSlotId();
    return id ? connMapRef.current.get(id) : undefined;
  };

  // ── Send on the *active* connection ──
  const sendRaw = useCallback((message: ClientMessage) => {
    const conn = getActiveConn();
    if (conn?.ws.readyState === WebSocket.OPEN) {
      conn.ws.send(JSON.stringify(message));
      return true;
    }
    return false;
  }, []);

  // ── Store actions (destructured for existing handleServerEvent compatibility) ──
  const {
    connectionStatus,
    serverUrl,
    clusterToken,
    workdir,
    config,
    setConnectionStatus,
    addMessage,
    updateMessage,
    setIsProcessing,
    setStreamingMessageId,
    setThinkingMessageId,
    addToolCall,
    updateToolCall,
    addPendingConfirmation,
    removePendingConfirmation,
    addDiff,
    setSessionInfo,
    setSessionList,
    removeSessionFromList,
    setSessionRestoreAvailable,
    clearSession,
    setSandboxBackend,
    setPendingChanges,
    setSandboxChangesData,
    setNodeList,
    setPeerList,
    setConnectedWorkdir,
    setTokenUsage,
    addConnectionHistory,
    setPlugins,
    setAvailableModels,
    setActiveModel,
    setPresets,
    setWorkflows,
    addWorkflow,
    updateWorkflow,
    deleteWorkflow,
    setActiveRun,
    activeConnectionId,
    connections,
    createConnectionSlot,
    removeConnectionSlot,
    _saveActiveSlot,
    _updateSlot,
  } = useAgentStore();

  const sendUserMessage = useCallback((text: string) => {
    const st = useAgentStore.getState();
    const userMsgId = uuidv4();
    addMessage({ id: userMsgId, role: 'user', content: text, timestamp: Date.now() });

    const ok = sendRaw({
      type: 'user_message',
      data: { text, workdir: st.workdir, model: st.config.model },
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
  }, [sendRaw, addMessage, setIsProcessing]);

  const sendCancel = useCallback(() => {
    sendRaw({ type: 'cancel', data: {} });
  }, [sendRaw]);

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

  const sandboxListChanges = useCallback(() => {
    sendRaw({ type: 'sandbox_list_changes', data: {} });
  }, [sendRaw]);

  const sandboxCommit = useCallback(() => {
    sendRaw({ type: 'sandbox_commit', data: {} });
  }, [sendRaw]);

  const sandboxCommitFile = useCallback((filePath: string) => {
    sendRaw({ type: 'sandbox_commit_file', data: { file_path: filePath } });
  }, [sendRaw]);

  const sandboxRollback = useCallback(() => {
    sendRaw({ type: 'sandbox_rollback', data: {} });
  }, [sendRaw]);

  const setWorkdirRemote = useCallback((newWorkdir: string) => {
    sendRaw({ type: 'set_workdir', data: { workdir: newWorkdir } });
  }, [sendRaw]);

  const setModelRemote = useCallback((model: string) => {
    useAgentStore.getState().setActiveModel(model);
    sendRaw({ type: 'set_model', data: { model } });
  }, [sendRaw]);

  // ── Model & endpoint management ────────────────────────────────────
  const fetchModels = useCallback((url: string, apiKey?: string) => {
    sendRaw({ type: 'fetch_models', data: { url, api_key: apiKey } });
  }, [sendRaw]);

  const addModel = useCallback((alias: string, model: string, endpoint: string) => {
    sendRaw({ type: 'add_model', data: { alias, model, endpoint } });
  }, [sendRaw]);

  const deleteModel = useCallback((alias: string) => {
    sendRaw({ type: 'delete_model', data: { alias } });
  }, [sendRaw]);

  const listEndpoints = useCallback(() => {
    sendRaw({ type: 'list_endpoints', data: {} });
  }, [sendRaw]);

  const addEndpoint = useCallback((name: string, provider: string, baseUrl: string, apiKey?: string) => {
    sendRaw({ type: 'add_endpoint', data: { name, provider, base_url: baseUrl, api_key: apiKey } });
  }, [sendRaw]);

  const deleteEndpoint = useCallback((name: string) => {
    sendRaw({ type: 'delete_endpoint', data: { name } });
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

  // ── Preset CRUD (global.db) ───────────────────────────────────────
  const listPresets = useCallback(() => {
    sendRaw({ type: 'list_presets', data: {} });
  }, [sendRaw]);

  const savePreset = useCallback((preset: any) => {
    // Map newSessionOnConnect → newSession for the Rust backend
    const serverPreset = { ...preset };
    if (preset.newSessionOnConnect !== undefined) {
      serverPreset.newSession = preset.newSessionOnConnect;
      delete serverPreset.newSessionOnConnect;
    }
    sendRaw({ type: 'save_preset', data: serverPreset });
  }, [sendRaw]);

  const deletePreset = useCallback((id: string) => {
    sendRaw({ type: 'delete_preset', data: { id } });
  }, [sendRaw]);

  // ── Node CRUD (global.db) ──────────────────────────────────────────
  const listNodes = useCallback(() => {
    sendRaw({ type: 'list_nodes', data: {} });
  }, [sendRaw]);

  const addNode = useCallback((node: any) => {
    // Ensure timestamps are present (Rust Node struct requires them)
    const now = new Date().toISOString();
    sendRaw({ type: 'add_node', data: { ...node, createdAt: node.createdAt || now, updatedAt: node.updatedAt || now } });
  }, [sendRaw]);

  const updateNode = useCallback((node: any) => {
    const now = new Date().toISOString();
    sendRaw({ type: 'update_node', data: { ...node, updatedAt: now } });
  }, [sendRaw]);

  const deleteNode = useCallback((id: string) => {
    sendRaw({ type: 'delete_node', data: { id } });
  }, [sendRaw]);

  // ── Peer CRUD (global.db) ───────────────────────────────────────────
  const listPeers = useCallback(() => {
    sendRaw({ type: 'list_peers', data: {} });
  }, [sendRaw]);

  const addPeer = useCallback((peer: any) => {
    const now = new Date().toISOString();
    sendRaw({ type: 'add_peer', data: { ...peer, createdAt: peer.createdAt || now, updatedAt: now } });
  }, [sendRaw]);

  const updatePeer = useCallback((peer: any) => {
    const now = new Date().toISOString();
    sendRaw({ type: 'update_peer', data: { ...peer, updatedAt: now } });
  }, [sendRaw]);

  const deletePeer = useCallback((id: string) => {
    sendRaw({ type: 'delete_peer', data: { id } });
  }, [sendRaw]);

  // ── Workflow CRUD (global.db) ─────────────────────────────────────
  const listWorkflows = useCallback(() => {
    sendRaw({ type: 'list_workflows', data: {} });
  }, [sendRaw]);

  const getWorkflow = useCallback((id: string) => {
    sendRaw({ type: 'get_workflow', data: { id } });
  }, [sendRaw]);

  const sendSaveWorkflow = useCallback((wf: any) => {
    sendRaw({ type: 'save_workflow', data: wf });
  }, [sendRaw]);

  const sendDeleteWorkflow = useCallback((id: string) => {
    sendRaw({ type: 'delete_workflow', data: { id } });
  }, [sendRaw]);

  const runWorkflow = useCallback((workflowId: string, task: string) => {
    sendRaw({ type: 'run_workflow', data: { workflowId, task } });
  }, [sendRaw]);

  const uploadFile = useCallback((name: string, content: string, mimeType?: string) => {
    const uploadMsgId = uuidv4();
    addMessage({
      id: uploadMsgId,
      role: 'system',
      content: `📎 正在上传: ${name}...`,
      timestamp: Date.now(),
    });
    return sendRaw({
      type: 'upload_file',
      data: { name, content, mime_type: mimeType },
    });
  }, [sendRaw, addMessage]);

  const listPlugins = useCallback(() => {
    sendRaw({ type: 'list_plugins', data: {} });
  }, [sendRaw]);

  const enablePlugin = useCallback((id: string) => {
    sendRaw({ type: 'enable_plugin', data: { id } });
  }, [sendRaw]);

  const disablePlugin = useCallback((id: string) => {
    sendRaw({ type: 'disable_plugin', data: { id } });
  }, [sendRaw]);

  // ── Core event handler — writes to the flat proxy ──
  // IMPORTANT: This function must be called while the global refs
  // (streamingMsgIdRef etc.) point to the correct connection's state.
  // For active-slot events this is naturally the case.
  // For inactive-slot events, processEventForSlot() swaps the refs first.
  const handleServerEvent = useCallback((event: ServerEvent) => {
    switch (event.type) {
      case 'ready':
        console.log('[ws] agent ready, version:', event.data.version);
        setSandboxBackend((event.data.sandbox_backend as 'overlay' | 'snapshot' | 'disabled') ?? 'disabled');
        setPendingChanges(0);
        setConnectedWorkdir(event.data.workdir ?? null);
        if (event.data.virtual_nodes) {
          setNodeList(event.data.virtual_nodes);
        }
        if (event.data.available_models) {
          setAvailableModels(event.data.available_models);
        }
        if (event.data.active_model) {
          setActiveModel(event.data.active_model);
        }
        break;

      case 'sandbox_status':
        setSandboxBackend(event.data.backend);
        if (event.data.pending_changes !== undefined) {
          setPendingChanges(event.data.pending_changes);
        }
        break;

      case 'sandbox_changes_result':
        setPendingChanges(event.data.pending_changes);
        setSandboxChangesData(event.data.files);
        break;

      case 'sandbox_commit_result':
        addMessage({
          id: uuidv4(), role: 'system',
          content: `✅ 沙盒提交完成：${event.data.modified} 文件修改，${event.data.created} 文件新建`,
          timestamp: Date.now(),
        });
        break;

      case 'sandbox_commit_file_result':
        addMessage({
          id: uuidv4(), role: 'system',
          content: `✅ 已提交文件：${event.data.file_path} ${event.data.created ? '（新建）' : '（修改）'}`,
          timestamp: Date.now(),
        });
        break;

      case 'sandbox_rollback_result':
        addMessage({
          id: uuidv4(), role: 'system',
          content: `↩️ 沙盒回滚完成：${event.data.restored} 文件恢复，${event.data.deleted} 文件删除${
            event.data.errors.length ? `（错误: ${event.data.errors.join(', ')}）` : ''
          }`,
          timestamp: Date.now(),
        });
        break;

      // ── Preset events (global.db) ─────────────────────────────────────
      case 'presets_list': {
        const serverPresets: any[] = event.data.presets || [];
        // Map Rust field names → TS ConfigPreset (newSession → newSessionOnConnect)
        const mapped = serverPresets.map((p: any) => ({
          ...p,
          newSessionOnConnect: p.newSession ?? p.newSessionOnConnect ?? false,
        }));
        // Merge with local presets (upsert by id) — never wipe local-only presets
        const st = useAgentStore.getState();
        const localPresets = st.presets;
        const merged = new Map<string, any>();
        for (const p of localPresets) merged.set(p.id, p);
        for (const p of mapped) merged.set(p.id, p); // server wins on conflict
        setPresets(Array.from(merged.values()));
        break;
      }

      case 'preset_saved': {
        const p = event.data.preset;
        if (!p) break;
        const mapped = {
          ...p,
          newSessionOnConnect: p.newSession ?? p.newSessionOnConnect ?? false,
        };
        const st = useAgentStore.getState();
        const existing = st.presets.findIndex((x: any) => x.id === mapped.id);
        if (existing >= 0) {
          st.updatePreset(mapped.id, mapped);
        } else {
          st.addPreset(mapped);
        }
        break;
      }

      case 'preset_deleted': {
        const st = useAgentStore.getState();
        st.deletePreset(event.data.id);
        break;
      }

      // ── Node events (server-managed workspaces) ─────────────────────────
      case 'nodes_list': {
        if (event.data.virtual_nodes) {
          setNodeList(event.data.virtual_nodes);
        }
        break;
      }

      case 'node_saved': {
        // The server sends back updated virtual_nodes after mutation
        if (event.data.virtual_nodes) {
          setNodeList(event.data.virtual_nodes);
        }
        break;
      }

      case 'node_deleted': {
        if (event.data.virtual_nodes) {
          setNodeList(event.data.virtual_nodes);
        }
        break;
      }

      // ── Peer events (global.db — remote agent server discovery) ──────
      case 'peers_list': {
        setPeerList(event.data.peers || []);
        break;
      }

      case 'peer_saved': {
        if (event.data.peers) {
          setPeerList(event.data.peers);
        }
        break;
      }

      case 'peer_deleted': {
        if (event.data.peers) {
          setPeerList(event.data.peers);
        }
        break;
      }

      // ── Workflow events (global.db) ────────────────────────────────────
      case 'workflows_list': {
        setWorkflows(event.data.workflows || []);
        break;
      }

      case 'workflow_loaded': {
        const wf = event.data.workflow;
        if (wf) {
          const st = useAgentStore.getState();
          const existing = st.workflows.findIndex((w: any) => w.id === wf.id);
          if (existing >= 0) {
            st.updateWorkflow(wf.id, wf);
          } else {
            st.addWorkflow(wf);
          }
        }
        break;
      }

      case 'workflow_saved': {
        const wf = event.data.workflow;
        if (wf) {
          const st = useAgentStore.getState();
          const existing = st.workflows.findIndex((w: any) => w.id === wf.id);
          if (existing >= 0) {
            st.updateWorkflow(wf.id, wf);
          } else {
            st.addWorkflow(wf);
          }
        }
        break;
      }

      case 'workflow_deleted': {
        const st = useAgentStore.getState();
        st.deleteWorkflow(event.data.id);
        break;
      }

      // ── Workflow execution events ─────────────────────────────────
      case 'workflow_started': {
        setActiveRun(null); // clear previous
        // Optionally show a notification
        console.log('Workflow started:', event.data);
        break;
      }

      case 'workflow_complete': {
        setActiveRun(event.data.run || null);
        break;
      }

      case 'workflow_error': {
        setActiveRun({
          id: '',
          workflowId: '',
          workflowName: '',
          status: 'error',
          task: '',
          totalTokens: 0,
          stageResults: [],
          errorMessage: event.data.message,
        } as any);
        break;
      }

      case 'upload_file_result':
        if (event.data.success) {
          addMessage({
            id: uuidv4(), role: 'system',
            content: `✅ 文件已上传: ${event.data.name} → ${event.data.path} (${formatFileSize(event.data.size ?? 0)})`,
            timestamp: Date.now(),
          });
        } else {
          addMessage({
            id: uuidv4(), role: 'system',
            content: `❌ 上传失败: ${event.data.name ?? 'unknown'} — ${event.data.error ?? '未知错误'}`,
            timestamp: Date.now(),
          });
        }
        break;

      case 'thinking':
        break;

      case 'thinking_start':
        thinkingMsgIdRef.current = lastAssistantMsgIdRef.current;
        if (thinkingMsgIdRef.current) setThinkingMessageId(thinkingMsgIdRef.current);
        break;

      case 'thinking_token':
        if (event.data?.token && thinkingMsgIdRef.current) {
          thinkingBufRef.current += event.data.token;
          scheduleFlush();
        }
        break;

      case 'thinking_end':
        flushTokens();
        thinkingMsgIdRef.current = null;
        setThinkingMessageId(null);
        break;

      case 'stream_start':
        streamingMsgIdRef.current = lastAssistantMsgIdRef.current;
        if (streamingMsgIdRef.current) setStreamingMessageId(streamingMsgIdRef.current);
        break;

      case 'streaming_token':
        if (event.data?.token && streamingMsgIdRef.current) {
          tokenBufRef.current += event.data.token;
          scheduleFlush();
        }
        break;

      case 'stream_end':
        flushTokens();
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        break;

      case 'assistant_text':
        if (event.data?.text && lastAssistantMsgIdRef.current) {
          updateMessage(lastAssistantMsgIdRef.current, event.data.text);
        }
        break;

      case 'tool_use': {
        const toolId = event.id || uuidv4();
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
          const storeState = useAgentStore.getState();
          const match = storeState.toolCalls
            .filter(c => c.tool === event.data.tool && c.status === 'executing')
            .sort((a, b) => a.timestamp - b.timestamp)[0];
          if (match) {
            const output = typeof event.data.output === 'string'
              ? event.data.output
              : String(event.data.output ?? '');
            updateToolCall(match.id, {
              status: event.data.is_error ? 'error' : 'completed',
              output,
            });
          }
        }
        break;

      case 'confirm_request': {
        const confirmId = event.data.tool_id || uuidv4();
        if (config.autoApprove) {
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
        if (event.data?.path != null) {
          const diffStr = typeof event.data.diff === 'string' ? event.data.diff : String(event.data.diff ?? '');
          addDiff({ id: uuidv4(), path: String(event.data.path), diff: diffStr, timestamp: Date.now() });
        }
        break;

      case 'done':
        flushTokens();
        setIsProcessing(false);
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        if (event.data?.pending_changes !== undefined) {
          setPendingChanges(event.data.pending_changes);
        }
        if (event.data?.input_tokens !== undefined || event.data?.output_tokens !== undefined) {
          setTokenUsage({
            input_tokens: event.data.input_tokens ?? 0,
            output_tokens: event.data.output_tokens ?? 0,
            role_usage: event.data.role_usage,
          });
        }
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

      case 'cancelled':
        flushTokens();
        setIsProcessing(false);
        streamingMsgIdRef.current = null;
        setStreamingMessageId(null);
        lastAssistantMsgIdRef.current = null;
        addMessage({ id: uuidv4(), role: 'system', content: '⏹ 已中断', timestamp: Date.now() });
        break;

      case 'warning':
      case 'context_warning':
        console.warn('[ws] warning:', event.data.message);
        break;

      case 'role_header':
        addMessage({
          id: uuidv4(),
          role: 'system',
          content: `${event.data.label}`,
          timestamp: Date.now(),
          meta: { stageLabel: event.data.label, stageModel: event.data.model },
        });
        {
          const stageMsgId = uuidv4();
          addMessage({ id: stageMsgId, role: 'assistant', content: '', timestamp: Date.now() });
          lastAssistantMsgIdRef.current = stageMsgId;
        }
        break;

      case 'stage_end':
        if (lastAssistantMsgIdRef.current) {
          const st = useAgentStore.getState();
          const stageMsg = st.messages.find(m => m.id === lastAssistantMsgIdRef.current);
          if (stageMsg && !stageMsg.content) {
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
        clearSession();
        break;

      case 'plugins_list':
        setPlugins(event.data.plugins ?? []);
        break;

      case 'model_changed':
        setActiveModel(event.data.alias);
        break;

      // ── Model & endpoint state sync ──────────────────────────────
      case 'model_state': {
        const st = useAgentStore.getState();
        if (event.data.models) st._updateSlot(st.activeConnectionId ?? 'default', (s) => ({ ...s, availableModels: event.data.models }));
        if (event.data.endpoints) st._updateSlot(st.activeConnectionId ?? 'default', (s) => ({ ...s, endpoints: event.data.endpoints }));
        if (event.data.default) st.setActiveModel(event.data.default as string);
        break;
      }

      case 'endpoints_list': {
        const st = useAgentStore.getState();
        st._updateSlot(st.activeConnectionId ?? 'default', (s) => ({ ...s, endpoints: event.data.endpoints }));
        break;
      }

      case 'models_fetched': {
        // Pass to the ModelsPanel via a global callback (avoids complex state wiring)
        const cb = (window as any).__onModelsFetched;
        if (cb) cb(event.data.models, event.data.source, event.data.url);
        break;
      }

      case 'model_added':
      case 'model_deleted':
      case 'endpoint_added':
      case 'endpoint_deleted':
        // model_state will follow with full sync — no per-event UI update needed
        break;

      case 'session_available': {
        // Auto-restore notification: a previous session exists but we
        // don't auto-load its messages.  Show a banner for the user to
        // explicitly restore (or dismiss).
        setSessionRestoreAvailable({
          message_count: event.data.message_count,
        });
        break;
      }

      case 'session_restored': {
        // Full restore — user explicitly requested it (or load_session command).
        // Clear the restore hint first, then load messages.
        setSessionRestoreAvailable(null);
        clearSession();
        const { messages: restored } = event.data;
        const now = Date.now();
        restored.forEach((m: { id: string; role: string; content: string; timestamp?: number }, i: number) => {
          const ts = m.timestamp ?? (now - (restored.length - i) * 1000);
          addMessage({
            id: m.id,
            role: m.role as 'user' | 'assistant' | 'system',
            content: m.content,
            timestamp: ts,
          });
        });
        lastAssistantMsgIdRef.current = null;
        break;
      }
    }
  }, [
    config.autoApprove, sendRaw, flushTokens, scheduleFlush,
    updateMessage, addMessage,
    addToolCall, updateToolCall, addPendingConfirmation, addDiff,
    setIsProcessing, setStreamingMessageId, setThinkingMessageId, setSessionInfo, setSessionList,
    removeSessionFromList, setSessionRestoreAvailable, clearSession, setSandboxBackend, setPendingChanges,
    setPlugins, setAvailableModels, setActiveModel,
  ]);

  // ── Batch-queue for inactive slots (avoids store swaps on every event) ──
  const inactiveQueuesRef = useRef<Map<string, ServerEvent[]>>(new Map());
  const inactiveTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  /** Flush accumulated token buffers for an INACTIVE slot directly via _updateSlot.
   *  No store swap — writes only to the connections map, not the flat proxy. */
  const flushInactiveTokens = useCallback((slotId: string) => {
    const conn = connMapRef.current.get(slotId);
    if (!conn) return;
    const textToken = conn.tokenBuf;
    const thinkToken = conn.thinkingBuf;
    if (!textToken && !thinkToken) return;
    conn.tokenBuf = '';
    conn.thinkingBuf = '';
    if (conn.inactiveFlushTimer) {
      clearTimeout(conn.inactiveFlushTimer);
      conn.inactiveFlushTimer = null;
    }

    const st = useAgentStore.getState();
    st._updateSlot(slotId, (slot) => {
      const msgs = slot.messages;
      let changed = false;
      const copy = [...msgs]; // shallow copy pointers (O(n) memory, no callback overhead)
      // Streaming messages are always at the end — scan backward
      if (textToken && conn.streamingMsgId) {
        for (let i = msgs.length - 1; i >= 0; i--) {
          if (msgs[i].id === conn.streamingMsgId) {
            copy[i] = { ...msgs[i], content: msgs[i].content + textToken };
            changed = true;
            break;
          }
        }
      }
      if (thinkToken && conn.thinkingMsgId) {
        for (let i = msgs.length - 1; i >= 0; i--) {
          if (msgs[i].id === conn.thinkingMsgId) {
            copy[i] = { ...msgs[i], thinking: (msgs[i].thinking || '') + thinkToken };
            changed = true;
            break;
          }
        }
      }
      return changed ? { ...slot, messages: copy } : slot;
    });
  }, []);

  /** Process all queued events for an inactive slot with ONE store swap.
   *  Called when the user switches to a previously inactive tab. */
  const flushInactiveBatch = useCallback((slotId: string) => {
    const queue = inactiveQueuesRef.current.get(slotId);
    if (!queue || queue.length === 0) return;
    inactiveQueuesRef.current.delete(slotId);
    inactiveTimersRef.current.delete(slotId);  // defensive cleanup

    const st = useAgentStore.getState();
    // Don't swap if the slot became active while we were waiting
    if (slotId === st.activeConnectionId) {
      for (const evt of queue) handleServerEvent(evt);
      const conn = connMapRef.current.get(slotId);
      if (conn) saveRefsToConn(conn);
      return;
    }

    const targetConn = connMapRef.current.get(slotId);
    if (!targetConn || targetConn.inactiveTerminated) return;

    const origId = st.activeConnectionId;
    const origConn = origId ? connMapRef.current.get(origId) : undefined;
    if (origConn) saveRefsToConn(origConn);

    // Switch to target slot — ONE swap for the whole batch
    loadRefsFromConn(targetConn);
    st.setActiveConnection(slotId);  // saves old slot internally

    for (const evt of queue) {
      handleServerEvent(evt);
    }

    saveRefsToConn(targetConn);

    if (origId) {
      if (origConn) loadRefsFromConn(origConn);
      st.setActiveConnection(origId);  // saves target slot internally
    }
  }, [handleServerEvent]);

  /** Accumulate events for an inactive slot — NO timer, no background processing.
   *  Events are flushed only when the user switches to the tab, eliminating the
   *  constant setActiveConnection swaps that were causing UI thrash on every
   *  200ms batch interval. */
  const scheduleInactiveBatch = useCallback((slotId: string, event: ServerEvent) => {
    const conn = connMapRef.current.get(slotId);
    if (!conn || conn.inactiveTerminated) return;

    let queue = inactiveQueuesRef.current.get(slotId);
    if (!queue) {
      queue = [];
      inactiveQueuesRef.current.set(slotId, queue);
    }
    // Cap to prevent unbounded growth if user never switches to this tab
    if (queue.length >= 500) queue.splice(0, 50);  // drop oldest events
    queue.push(event);
  }, []);

  // ── Process an event for a specific slot ──
  // Active slot: process directly. Inactive slot: batch/accumulate WITHOUT store swap.
  const processEventForSlot = useCallback((event: ServerEvent, slotId: string) => {
    const st = useAgentStore.getState();
    if (slotId === st.activeConnectionId) {
      // Active slot — refs already loaded, process directly
      handleServerEvent(event);
      const conn = connMapRef.current.get(slotId);
      if (conn) saveRefsToConn(conn);
      return;
    }

    // ── Inactive slot: avoid store swaps ──
    // All events are handled directly via _updateSlot, no store swap needed.
    const targetConn = connMapRef.current.get(slotId);
    if (!targetConn) return;

    // Streaming tokens: accumulate directly in SlotConn buffers (NO store swap!)
    if (event.type === 'streaming_token') {
      if (event.data?.token && targetConn.streamingMsgId) {
        targetConn.tokenBuf += event.data.token;
        if (!targetConn.inactiveFlushTimer) {
          targetConn.inactiveFlushTimer = setTimeout(() => flushInactiveTokens(slotId), 80);
        }
      }
      return;
    }

    if (event.type === 'thinking_token') {
      if (event.data?.token && targetConn.thinkingMsgId) {
        targetConn.thinkingBuf += event.data.token;
        if (!targetConn.inactiveFlushTimer) {
          targetConn.inactiveFlushTimer = setTimeout(() => flushInactiveTokens(slotId), 80);
        }
      }
      return;
    }

    // Ref-only events: update SlotConn refs directly, no store swap
    if (event.type === 'stream_start') {
      targetConn.streamingMsgId = targetConn.lastAssistantMsgId;
      // Also write streamingMessageId to the inactive slot
      st._updateSlot(slotId, s => ({ ...s, streamingMessageId: targetConn.streamingMsgId }));
      return;
    }
    if (event.type === 'stream_end') {
      flushInactiveTokens(slotId);
      targetConn.streamingMsgId = null;
      st._updateSlot(slotId, s => ({ ...s, streamingMessageId: null }));
      return;
    }
    if (event.type === 'thinking_start') {
      targetConn.thinkingMsgId = targetConn.lastAssistantMsgId;
      st._updateSlot(slotId, s => ({ ...s, thinkingMessageId: targetConn.thinkingMsgId }));
      return;
    }
    if (event.type === 'thinking_end') {
      flushInactiveTokens(slotId);
      targetConn.thinkingMsgId = null;
      st._updateSlot(slotId, s => ({ ...s, thinkingMessageId: null }));
      return;
    }
    if (event.type === 'thinking') {
      return; // no-op
    }

    // Terminal events: flush tokens, then batch-process with ONE store swap
    if (event.type === 'done' || event.type === 'error' || event.type === 'cancelled') {
      flushInactiveTokens(slotId);
      flushInactiveBatch(slotId);
      targetConn.inactiveTerminated = true;
      // Direct swap for terminal event — these need handleServerEvent to update flat proxy
      const origId = st.activeConnectionId;
      const origConn = origId ? connMapRef.current.get(origId) : undefined;
      if (origConn) saveRefsToConn(origConn);
      loadRefsFromConn(targetConn);
      st.setActiveConnection(slotId);
      handleServerEvent(event);
      saveRefsToConn(targetConn);
      if (origId) {
        if (origConn) loadRefsFromConn(origConn);
        st.setActiveConnection(origId);
      }
      return;
    }

    // All other events: queue for batched processing (single swap for the whole batch)
    scheduleInactiveBatch(slotId, event);
  }, [handleServerEvent, flushInactiveTokens, flushInactiveBatch, scheduleInactiveBatch]);

  // ── Connect a specific slot ──
  const connect = useCallback((slotId?: string) => {
    const st = useAgentStore.getState();
    const id = slotId || st.activeConnectionId;
    if (!id) {
      setConnectionStatus('disconnected');
      return;
    }

    // Defensive: sync flat proxy → slot so the slot always has the latest serverUrl/workdir
    // (e.g. when applyPreset updated the flat proxy but not the legacy 'default' slot)
    if (id === st.activeConnectionId) {
      st._saveActiveSlot();  // sync metadata to slot before connecting
    }

    const slot = st.connections[id];
    if (!slot || !slot.serverUrl) {
      setConnectionStatus('disconnected');
      return;
    }

    // Close existing WS for this slot if any
    const existing = connMapRef.current.get(id);
    if (existing) {
      existing.ws.close();
      connMapRef.current.delete(id);
    }

    const currentServerUrl = slot.serverUrl;
    const currentWorkdir = slot.workdir;
    const currentIsolation = st.config.isolation ?? 'container';
    const currentClusterToken = st.clusterToken;

    // Update flat proxy if this is the active slot
    if (id === st.activeConnectionId) {
      setConnectionStatus('connecting');
    } else {
      // Inactive slot: just update its stored status
      st._updateSlot(id, s => ({ ...s, connectionStatus: 'connecting' }));
    }

    try {
      const base = ensureAgentPath(currentServerUrl);
      const params: string[] = [];
      if (currentWorkdir) params.push(`workdir=${encodeURIComponent(currentWorkdir)}`);
      if (currentIsolation !== 'container') params.push(`mode=${currentIsolation}`);
      if (currentClusterToken) params.push(`token=${encodeURIComponent(currentClusterToken)}`);
      const sep = base.includes('?') ? '&' : '?';
      const wsUrl = params.length > 0 ? `${base}${sep}${params.join('&')}` : base;
      const ws = new WebSocket(wsUrl);

      const conn: SlotConn = {
        ws,
        streamingMsgId: null,
        thinkingMsgId: null,
        lastAssistantMsgId: null,
        tokenBuf: '',
        thinkingBuf: '',
        flushTimer: null,
        inactiveFlushTimer: null,
        inactiveTerminated: false,
      };

      connMapRef.current.set(id, conn);

      ws.onopen = () => {
        if (id === useAgentStore.getState().activeConnectionId) {
          setConnectionStatus('connected');
          // Clear any stale session-restore hint from a previous connection
          setSessionRestoreAvailable(null);
        } else {
          st._updateSlot(id, s => ({ ...s, connectionStatus: 'connected' }));
        }
        addConnectionHistory(currentServerUrl, currentWorkdir);
        ws.send(JSON.stringify({ type: 'list_plugins', data: {} }));

        // If user opted for a fresh session, send new_session after connect.
        // This runs BEFORE the server's auto-restore emits session_available,
        // so the restored conversation is immediately discarded.
        if (st.config.newSessionOnConnect) {
          ws.send(JSON.stringify({ type: 'new_session', data: {} }));
        }
      };

      ws.onclose = () => {
        if (connMapRef.current.get(id)?.ws !== ws) return; // stale close
        // Flush
        if (id === useAgentStore.getState().activeConnectionId) {
          flushTokens();
          setConnectionStatus('disconnected');
          setConnectedWorkdir(null);
          setStreamingMessageId(null);
          setIsProcessing(false);
          streamingMsgIdRef.current = null;
        }
        // Clean up connection map
        if (connMapRef.current.get(id)?.ws === ws) {
          connMapRef.current.delete(id);
        }
      };

      ws.onerror = () => {
        if (id === useAgentStore.getState().activeConnectionId) {
          setConnectionStatus('error');
        } else {
          st._updateSlot(id, s => ({ ...s, connectionStatus: 'error' }));
        }
      };

      ws.onmessage = (e) => {
        try {
          const event = JSON.parse(e.data) as ServerEvent;
          processEventForSlot(event, id);
        } catch (err) {
          console.error('[ws] parse error:', err);
        }
      };

      // If this is the active slot, wire up global refs
      if (id === st.activeConnectionId) {
        loadRefsFromConn(conn);
      }
    } catch (err) {
      if (id === st.activeConnectionId) {
        setConnectionStatus('error');
      } else {
        st._updateSlot(id, s => ({ ...s, connectionStatus: 'error' }));
      }
      console.error('[ws] connect failed:', err);
    }
  }, [setConnectionStatus, setIsProcessing, setStreamingMessageId, addConnectionHistory, flushTokens, processEventForSlot]);

  // ── Disconnect a specific slot ──
  const disconnect = useCallback((slotId?: string) => {
    const id = slotId || useAgentStore.getState().activeConnectionId;
    if (!id) return;

    const conn = connMapRef.current.get(id);
    if (!conn) return;

    // Save state
    if (id === useAgentStore.getState().activeConnectionId) {
      useAgentStore.getState()._saveActiveSlot();
      saveRefsToConn(conn);
    }

    // Clear any inactive-slot timers / queues
    if (conn.inactiveFlushTimer) { clearTimeout(conn.inactiveFlushTimer); }
    inactiveQueuesRef.current.delete(id);
    inactiveTimersRef.current.delete(id);

    conn.ws.close();
    connMapRef.current.delete(id);

    if (id === useAgentStore.getState().activeConnectionId) {
      setConnectionStatus('disconnected');
      setSessionInfo(null);
    }
  }, [setConnectionStatus, setSessionInfo]);

  // ── Switch active tab (no WS change!) ──
  // Called by ConnectionTabs or other UI when the user clicks a different tab.
  // Saves current refs to old connection, loads refs from new connection,
  // and swaps the flat proxy in the store.
  const switchToConnection = useCallback((id: string) => {
    const st = useAgentStore.getState();
    if (id === st.activeConnectionId) return;

    // Flush any pending inactive batches for the target before switching
    flushInactiveBatch(id);

    // Save current refs to old connection
    const oldId = st.activeConnectionId;
    const oldConn = oldId ? connMapRef.current.get(oldId) : undefined;
    if (oldConn) saveRefsToConn(oldConn);

    // Switch to new slot — setActiveConnection saves old slot internally
    st.setActiveConnection(id);

    // Load refs from new connection (if it has a live WS)
    const newConn = connMapRef.current.get(id);
    if (newConn) {
      // Reset terminated flag — slot is now active and should process events normally
      newConn.inactiveTerminated = false;
      // Clear any stale inactive queue entries and timers
      inactiveQueuesRef.current.delete(id);
      inactiveTimersRef.current.delete(id);
      if (newConn.inactiveFlushTimer) {
        clearTimeout(newConn.inactiveFlushTimer);
        newConn.inactiveFlushTimer = null;
      }

      loadRefsFromConn(newConn);
      // Flush any tokens buffered while inactive, then schedule flush
      if (newConn.tokenBuf || newConn.thinkingBuf) {
        flushTokens();
        scheduleFlush();
      }
    } else {
      // No live WS yet — reset refs
      streamingMsgIdRef.current = null;
      thinkingMsgIdRef.current = null;
      lastAssistantMsgIdRef.current = null;
      tokenBufRef.current = '';
      thinkingBufRef.current = '';
    }

    // If the new slot has no WS yet, auto-connect
    if (!newConn && st.connections[id]?.serverUrl) {
      connect(id);
    }
  }, [connect, flushInactiveBatch, flushTokens, scheduleFlush]);

  // ── Sync execution mode to server ──
  const agentMode = config.agentMode ?? 'auto';
  useEffect(() => {
    const st = useAgentStore.getState();
    if (st.connectionStatus === 'connected') {
      sendRaw({ type: 'set_mode', data: { mode: agentMode as 'auto' | 'simple' | 'plan' | 'pipeline' } });
      if (st.workdir) {
        sendRaw({ type: 'set_workdir', data: { workdir: st.workdir } });
      }
    }
  }, [agentMode, connectionStatus, sendRaw]);

  // ── Cleanup on unmount: close all connections ──
  useEffect(() => () => {
    // Clear all inactive timers and queues
    inactiveTimersRef.current.forEach(t => clearTimeout(t));
    inactiveTimersRef.current.clear();
    inactiveQueuesRef.current.clear();
    connMapRef.current.forEach(conn => {
      if (conn.inactiveFlushTimer) clearTimeout(conn.inactiveFlushTimer);
      try { conn.ws.close(); } catch {}
    });
    connMapRef.current.clear();
  }, []);

  return {
    connectionStatus,
    connect,
    disconnect,
    sendUserMessage,
    sendCancel,
    confirmToolCall,
    answerQuestion,
    reviewPlan,
    setSandbox,
    sandboxListChanges,
    sandboxCommit,
    sandboxCommitFile,
    sandboxRollback,
    setWorkdirRemote,
    setModelRemote,
    fetchModels,
    addModel,
    deleteModel,
    listEndpoints,
    addEndpoint,
    deleteEndpoint,
    loadSession,
    newSession,
    listSessions,
    deleteSession,
    loadSessionById,
    listPresets,
    savePreset,
    deletePreset,
    listNodes,
    addNode,
    updateNode,
    deleteNode,
    listPeers,
    addPeer,
    updatePeer,
    deletePeer,
    listWorkflows,
    getWorkflow,
    saveWorkflow: sendSaveWorkflow,
    deleteWorkflow: sendDeleteWorkflow,
    runWorkflow,
    uploadFile,
    listPlugins,
    enablePlugin,
    disablePlugin,
    switchToConnection,
    isConnected: connectionStatus === 'connected',
  };
};
