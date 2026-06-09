import { create } from 'zustand';
import { persist, subscribeWithSelector } from 'zustand/middleware';
import { Message, ToolCall, ConnectionStatus, AgentConfig, FileInfo, SessionInfo, SessionMeta, ConfigPreset, VirtualNodeInfo, ConnectionHistory, TokenUsage, PluginInfo, ConnectionSlot, ModelInfo, EndpointInfo, WorkflowDef, WorkflowRunResult } from '../types/agent';
import { getDefaultServerUrl, getDefaultWorkdir, isDesktopApp } from '../utils/environment';

export interface SandboxFileChange {
  path: string;
  kind: 'modified' | 'created' | 'deleted' | 'unchanged';
  original_size: number | null;
  current_size: number | null;
  diff: string | null;
}

export interface DiffEntry {
  id: string;
  path: string;
  diff: string;
  timestamp: number;
}

export interface PendingConfirmation {
  id: string;
  action: string;
  details?: string;
  type: 'confirm' | 'ask_user' | 'review_plan';
}

// ── Helpers: create empty slot ──

function createEmptySlot(id: string, label: string, serverUrl: string, workdir?: string): ConnectionSlot {
  return {
    id, label, serverUrl, workdir,
    connectionStatus: 'disconnected',
    connectedWorkdir: null,
    messages: [],
    toolCalls: [],
    pendingConfirmations: [],
    diffs: [],
    isProcessing: false,
    streamingMessageId: null,
    thinkingMessageId: null,
    currentMessage: '',
    sessionInfo: null,
    sessionList: [],
    sandboxBackend: 'disabled',
    pendingChanges: 0,
    sandboxChangesData: null,
    tokenUsage: null,
    nodeList: [],
    plugins: [],
    sessionRestoreAvailable: null,
    availableModels: [],
    activeModel: null,
    endpoints: [],
  };
}

// ── State interface ──

interface AgentState {
  // ── Multi-connection slots ──
  connections: Record<string, ConnectionSlot>;
  activeConnectionId: string | null;

  // ── Active connection proxy state (flat, for backward compat selectors) ──
  // These are always kept in sync with connections[activeConnectionId].
  // Mutations write to BOTH (connections[id] + flat proxy) simultaneously.
  connectionStatus: ConnectionStatus;
  serverUrl: string;
  workdir?: string;
  connectedWorkdir: string | null;

  messages: Message[];
  currentMessage: string;
  isProcessing: boolean;
  streamingMessageId: string | null;
  thinkingMessageId: string | null;

  toolCalls: ToolCall[];
  pendingConfirmations: PendingConfirmation[];

  diffs: DiffEntry[];

  sandboxBackend: string;
  pendingChanges: number;
  sandboxChangesData: SandboxFileChange[] | null;

  sessionInfo: SessionInfo | null;
  sessionList: SessionMeta[];
  sessionRestoreAvailable: { message_count: number } | null;

  nodeList: VirtualNodeInfo[];
  peerList: any[];  // PeerInfo[] — list of configured remote peer servers

  tokenUsage: TokenUsage | null;

  plugins: PluginInfo[];

  availableModels: ModelInfo[];
  activeModel: string | null;
  endpoints: EndpointInfo[];

  // ── Global shared state (not per-connection) ──
  clusterToken: string;
  currentPath: string;
  fileList: FileInfo[];
  config: AgentConfig;
  presets: ConfigPreset[];
  connectionHistory: ConnectionHistory[];
  workflows: WorkflowDef[];
  activeRun: WorkflowRunResult | null;

  // ── Connection lifecycle actions ──
  createConnectionSlot: (id: string, label: string, serverUrl: string, workdir?: string) => void;
  removeConnectionSlot: (id: string) => void;
  setActiveConnection: (id: string) => void;
  /** Save current flat proxy state into the active slot (metadata sync). */
  _saveActiveSlot: () => void;
  /** Update a specific slot's data. If it's the active slot, also mirror to flat proxy. */
  _updateSlot: (slotId: string, updater: (slot: ConnectionSlot) => ConnectionSlot) => void;

  // ── Existing actions (operate on active connection proxy) ──
  setConnectionStatus: (status: ConnectionStatus) => void;
  setServerUrl: (url: string) => void;
  setWorkdir: (workdir: string) => void;
  addMessage: (message: Message) => void;
  updateMessage: (id: string, content: string) => void;
  appendToMessage: (id: string, token: string) => void;
  setCurrentMessage: (message: string) => void;
  setIsProcessing: (processing: boolean) => void;
  setStreamingMessageId: (id: string | null) => void;
  setThinkingMessageId: (id: string | null) => void;
  appendToThinking: (id: string, token: string) => void;
  addToolCall: (toolCall: ToolCall) => void;
  updateToolCall: (id: string, updates: Partial<ToolCall>) => void;
  addPendingConfirmation: (confirmation: PendingConfirmation) => void;
  removePendingConfirmation: (id: string) => void;
  addDiff: (diff: DiffEntry) => void;
  setSandboxBackend: (backend: string) => void;
  setPendingChanges: (count: number) => void;
  setSandboxChangesData: (data: SandboxFileChange[] | null) => void;
  setCurrentPath: (path: string) => void;
  setFileList: (files: FileInfo[]) => void;
  setSessionInfo: (info: SessionInfo | null) => void;
  setSessionList: (list: SessionMeta[]) => void;
  removeSessionFromList: (id: string) => void;
  setSessionRestoreAvailable: (info: { message_count: number } | null) => void;
  setConfig: (config: Partial<AgentConfig>) => void;
  addPreset: (preset: ConfigPreset) => void;
  updatePreset: (id: string, preset: Partial<ConfigPreset>) => void;
  deletePreset: (id: string) => void;
  setPresets: (presets: ConfigPreset[]) => void;
  applyPreset: (id: string) => void;
  setWorkflows: (workflows: WorkflowDef[]) => void;
  addWorkflow: (wf: WorkflowDef) => void;
  updateWorkflow: (id: string, wf: Partial<WorkflowDef>) => void;
  deleteWorkflow: (id: string) => void;
  clearSession: () => void;
  setNodeList: (nodes: VirtualNodeInfo[]) => void;
  setPeerList: (peers: any[]) => void;
  setClusterToken: (token: string) => void;
  setConnectedWorkdir: (workdir: string | null) => void;
  setTokenUsage: (usage: TokenUsage) => void;
  addConnectionHistory: (serverUrl: string, workdir?: string) => void;
  removeConnectionHistory: (id: string) => void;
  clearConnectionHistory: () => void;
  setPlugins: (plugins: PluginInfo[]) => void;
  setAvailableModels: (models: ModelInfo[]) => void;
  setActiveModel: (alias: string | null) => void;
  setActiveRun: (run: WorkflowRunResult | null) => void;
  reset: () => void;
}

// ── Persisted config loader ──

const loadPersistedConfig = (): Partial<AgentConfig & { serverUrl: string; workdir?: string; presets: ConfigPreset[] }> => {
  const defaultServerUrl = getDefaultServerUrl();

  try {
    const stored = localStorage.getItem('rust-agent-config');
    if (stored) {
      const parsed = JSON.parse(stored);
      const data = parsed.state ?? parsed;
      return {
        serverUrl: data.serverUrl || defaultServerUrl,
        autoApprove: data.config?.autoApprove ?? false,
        agentMode: data.config?.agentMode || 'auto',
        workdir: data.workdir,
        presets: data.presets || [],
      };
    }
  } catch (e) {
    console.warn('Failed to load persisted config:', e);
  }
  return {
    serverUrl: defaultServerUrl,
    presets: []
  };
};

const persistedConfig = loadPersistedConfig();

const defaultId = 'default';
const defaultUrl = persistedConfig.serverUrl || getDefaultServerUrl();

const initialSlot = createEmptySlot(defaultId, '', defaultUrl, persistedConfig.workdir);

const initialState = {
  // ── Connections ──
  connections: { [defaultId]: initialSlot } as Record<string, ConnectionSlot>,
  activeConnectionId: defaultId as string | null,

  // ── Active proxy ──
  connectionStatus: 'disconnected' as ConnectionStatus,
  serverUrl: defaultUrl,
  workdir: persistedConfig.workdir,
  connectedWorkdir: null as string | null,

  messages: [] as Message[],
  currentMessage: '',
  isProcessing: false,
  streamingMessageId: null as string | null,
  thinkingMessageId: null as string | null,

  toolCalls: [] as ToolCall[],
  pendingConfirmations: [] as PendingConfirmation[],

  diffs: [] as DiffEntry[],

  sandboxBackend: 'disabled',
  pendingChanges: 0,
  sandboxChangesData: null as SandboxFileChange[] | null,

  sessionInfo: null as SessionInfo | null,
  sessionList: [] as SessionMeta[],
  sessionRestoreAvailable: null as { message_count: number } | null,

  nodeList: [] as VirtualNodeInfo[],
  peerList: [] as any[],

  tokenUsage: null as TokenUsage | null,

  plugins: [] as PluginInfo[],

  availableModels: [] as ModelInfo[],
  activeModel: null as string | null,
  endpoints: [] as import('../types/agent').EndpointInfo[],

  // ── Global shared ──
  clusterToken: '',
  currentPath: '.',
  fileList: [] as FileInfo[],
  presets: (persistedConfig.presets || []) as ConfigPreset[],
  connectionHistory: [] as ConnectionHistory[],
  workflows: [] as WorkflowDef[],
  activeRun: null as WorkflowRunResult | null,
  config: {
    serverUrl: defaultUrl,
    autoApprove: persistedConfig.autoApprove ?? false,
    agentMode: persistedConfig.agentMode || ('auto' as const),
    newSessionOnConnect: persistedConfig.newSessionOnConnect ?? false,
  } as AgentConfig,
};

export const useAgentStore = create<AgentState>()(
  subscribeWithSelector(
    persist(
    (set, get) => {

      // ── Helper: sync a flat-proxy field update to the active slot ──
      // All "light" setters use this to write to BOTH places simultaneously.
      const syncActiveSlot = (flatUpdates: Partial<AgentState>) => {
        const state = get() as any;
        const id = state.activeConnectionId;
        if (!id || !state.connections?.[id]) return flatUpdates;
        const slotUpdates: any = {};
        // Map flat fields to slot fields (same names for most)
        for (const key of Object.keys(flatUpdates)) {
          if (key in state.connections[id]) {
            slotUpdates[key] = (flatUpdates as any)[key];
          }
        }
        if (Object.keys(slotUpdates).length === 0) return flatUpdates;
        return {
          ...flatUpdates,
          connections: {
            ...state.connections,
            [id]: { ...state.connections[id], ...slotUpdates },
          },
        };
      };

      // ── Helper: sync a single heavy flat-proxy field to the active slot ──
      // Used by addMessage, appendToMessage, addToolCall, etc.
      // Writes the SAME reference to both flat proxy and connections[activeId].
      const syncHeavyField = <K extends 'messages' | 'toolCalls' | 'diffs' | 'pendingConfirmations'>(
        field: K,
        value: any,
      ) => (state: AgentState): any => {
        const slotId = state.activeConnectionId;
        const result: any = { [field]: value };
        if (slotId && state.connections[slotId]) {
          result.connections = {
            ...state.connections,
            [slotId]: { ...state.connections[slotId], [field]: value },
          };
        }
        return result;
      };

      return {
      ...initialState,

      // ── Connection slot management ──

      _saveActiveSlot: () => {
        const state = get();
        const id = state.activeConnectionId;
        if (!id || !state.connections[id]) return;
        // Save current flat proxy metadata into the slot
        const slot = state.connections[id];
        const updated = {
          ...slot,
          connectionStatus: state.connectionStatus,
          serverUrl: state.serverUrl,
          workdir: state.workdir,
          connectedWorkdir: state.connectedWorkdir,
          sandboxBackend: state.sandboxBackend,
          pendingChanges: state.pendingChanges,
          sandboxChangesData: state.sandboxChangesData,
          sessionInfo: state.sessionInfo,
          sessionList: state.sessionList,
          sessionRestoreAvailable: state.sessionRestoreAvailable,
          nodeList: state.nodeList,
          tokenUsage: state.tokenUsage,
          plugins: state.plugins,
          availableModels: state.availableModels,
          activeModel: state.activeModel,
        };
        set({ connections: { ...state.connections, [id]: updated } });
      },

      _updateSlot: (slotId, updater) => {
        const state = get();
        const oldSlot = state.connections[slotId];
        if (!oldSlot) return;
        const updatedSlot = updater(oldSlot);
        const updated = { ...state.connections, [slotId]: updatedSlot };
        if (slotId === state.activeConnectionId) {
          // Active slot — mirror heavy fields to flat proxy so UI reacts
          set({
            connections: updated,
            messages: updatedSlot.messages,
            toolCalls: updatedSlot.toolCalls,
            pendingConfirmations: updatedSlot.pendingConfirmations,
            diffs: updatedSlot.diffs,
            isProcessing: updatedSlot.isProcessing,
            streamingMessageId: updatedSlot.streamingMessageId,
            thinkingMessageId: updatedSlot.thinkingMessageId,
            connectionStatus: updatedSlot.connectionStatus,
            serverUrl: updatedSlot.serverUrl,
            workdir: updatedSlot.workdir,
            connectedWorkdir: updatedSlot.connectedWorkdir,
            sandboxBackend: updatedSlot.sandboxBackend,
            pendingChanges: updatedSlot.pendingChanges,
            sandboxChangesData: updatedSlot.sandboxChangesData,
            sessionInfo: updatedSlot.sessionInfo,
            sessionList: updatedSlot.sessionList,
            sessionRestoreAvailable: updatedSlot.sessionRestoreAvailable,
            nodeList: updatedSlot.nodeList,
            tokenUsage: updatedSlot.tokenUsage,
            plugins: updatedSlot.plugins,
            availableModels: updatedSlot.availableModels,
            activeModel: updatedSlot.activeModel,
          });
        } else {
          // Inactive slot — only update the connections map
          set({ connections: updated });
        }
      },

      createConnectionSlot: (id, label, serverUrl, workdir) => {
        const state = get();
        const { activeConnectionId, ...rest } = state;
        const updated = { ...state.connections, [id]: createEmptySlot(id, label, serverUrl, workdir) };
        // Also save current active slot state before creating new one
        const currentId = state.activeConnectionId;
        if (currentId && updated[currentId]) {
          // Update current slot metadata before adding new slot
          updated[currentId] = {
            ...updated[currentId],
            connectionStatus: state.connectionStatus,
            serverUrl: state.serverUrl,
            workdir: state.workdir,
          };
        }
        set({ connections: updated });
      },

      removeConnectionSlot: (id) => {
        const state = get();
        const { [id]: _removed, ...rest } = state.connections;
        const remaining = Object.keys(rest);

        if (id === state.activeConnectionId) {
          if (remaining.length > 0) {
            // Switch to another connection
            const nextId = remaining[0];
            const nextSlot = rest[nextId];
            set({
              connections: rest,
              activeConnectionId: nextId,
              // Populate flat proxy from next slot
              connectionStatus: nextSlot.connectionStatus,
              serverUrl: nextSlot.serverUrl,
              workdir: nextSlot.workdir,
              connectedWorkdir: nextSlot.connectedWorkdir,
              messages: nextSlot.messages,
              toolCalls: nextSlot.toolCalls,
              pendingConfirmations: nextSlot.pendingConfirmations,
              diffs: nextSlot.diffs,
              isProcessing: nextSlot.isProcessing,
              streamingMessageId: nextSlot.streamingMessageId,
              thinkingMessageId: nextSlot.thinkingMessageId,
              currentMessage: nextSlot.currentMessage,
              sandboxBackend: nextSlot.sandboxBackend,
              pendingChanges: nextSlot.pendingChanges,
              sandboxChangesData: nextSlot.sandboxChangesData,
              sessionInfo: nextSlot.sessionInfo,
              sessionList: nextSlot.sessionList,
              sessionRestoreAvailable: nextSlot.sessionRestoreAvailable,
              nodeList: nextSlot.nodeList,
              tokenUsage: nextSlot.tokenUsage,
              plugins: nextSlot.plugins,
              availableModels: nextSlot.availableModels,
              activeModel: nextSlot.activeModel,
            });
          } else {
            // No connections left; keep an empty placeholder
            const emptySlot = createEmptySlot(defaultId, '', '');
            set({
              connections: { [defaultId]: emptySlot },
              activeConnectionId: defaultId,
              connectionStatus: 'disconnected',
              serverUrl: '',
              workdir: undefined,
              connectedWorkdir: null,
              messages: [],
              toolCalls: [],
              pendingConfirmations: [],
              diffs: [],
              isProcessing: false,
              streamingMessageId: null,
              thinkingMessageId: null,
              currentMessage: '',
              sandboxBackend: 'disabled',
              pendingChanges: 0,
              sandboxChangesData: null,
              sessionInfo: null,
              sessionList: [],
              sessionRestoreAvailable: null,
              nodeList: [],
              tokenUsage: null,
              plugins: [],
              availableModels: [],
              activeModel: null,
              endpoints: [],
            });
          }
        } else {
          // Just remove the inactive slot
          set({ connections: rest });
        }
      },

      setActiveConnection: (id) => {
        const state = get();
        if (id === state.activeConnectionId) return;

        // Save current active slot metadata
        const updated = { ...state.connections };
        const currentId = state.activeConnectionId;
        if (currentId && updated[currentId]) {
          updated[currentId] = {
            ...updated[currentId],
            connectionStatus: state.connectionStatus,
            serverUrl: state.serverUrl,
            workdir: state.workdir,
            connectedWorkdir: state.connectedWorkdir,
            sandboxBackend: state.sandboxBackend,
            pendingChanges: state.pendingChanges,
            sessionInfo: state.sessionInfo,
            sessionList: state.sessionList,
            sessionRestoreAvailable: state.sessionRestoreAvailable,
            nodeList: state.nodeList,
            tokenUsage: state.tokenUsage,
            plugins: state.plugins,
          };
        }

        // Load target slot
        let target = updated[id];
        if (!target) return;
        // Ensure target has proper structure
        if (!Array.isArray((target as any).messages)) {
          target = createEmptySlot(id, target.label || '', target.serverUrl, target.workdir);
          updated[id] = target;
        }

        // Populate flat proxy from target slot
        set({
          connections: updated,
          activeConnectionId: id,
          connectionStatus: target.connectionStatus,
          serverUrl: target.serverUrl,
          workdir: target.workdir,
          connectedWorkdir: target.connectedWorkdir,
          messages: target.messages,
          toolCalls: target.toolCalls,
          pendingConfirmations: target.pendingConfirmations,
          diffs: target.diffs,
          isProcessing: target.isProcessing,
          streamingMessageId: target.streamingMessageId,
          thinkingMessageId: target.thinkingMessageId,
          currentMessage: target.currentMessage,
          sandboxBackend: target.sandboxBackend,
          pendingChanges: target.pendingChanges,
          sandboxChangesData: target.sandboxChangesData,
          sessionInfo: target.sessionInfo,
          sessionList: target.sessionList,
          sessionRestoreAvailable: target.sessionRestoreAvailable,
          nodeList: target.nodeList,
          tokenUsage: target.tokenUsage,
          plugins: target.plugins,
          availableModels: target.availableModels,
          activeModel: target.activeModel,
        });
      },

      // ── Light setters (sync to both flat proxy + active slot) ──

      setConnectionStatus: (status) =>
        set(syncActiveSlot({ connectionStatus: status })),

      setServerUrl: (url) => {
        set(syncActiveSlot({
          serverUrl: url,
          config: { ...get().config, serverUrl: url },
        }));
      },

      setWorkdir: (workdir) =>
        set(syncActiveSlot({ workdir: workdir || undefined })),

      setCurrentMessage: (message) => set({ currentMessage: message }),
      setIsProcessing: (processing) =>
        set(syncActiveSlot({ isProcessing: processing })),

      setStreamingMessageId: (id) =>
        set(syncActiveSlot({ streamingMessageId: id })),

      setThinkingMessageId: (id) =>
        set(syncActiveSlot({ thinkingMessageId: id })),

      setSandboxBackend: (backend) =>
        set(syncActiveSlot({ sandboxBackend: backend })),

      setPendingChanges: (count) =>
        set(syncActiveSlot({ pendingChanges: count })),

      setSandboxChangesData: (data) =>
        set(syncActiveSlot({ sandboxChangesData: data })),

      setSessionInfo: (info) =>
        set(syncActiveSlot({ sessionInfo: info })),

      setSessionList: (list) =>
        set(syncActiveSlot({ sessionList: list })),

      setSessionRestoreAvailable: (info) =>
        set(syncActiveSlot({ sessionRestoreAvailable: info })),

      setNodeList: (nodes) =>
        set(syncActiveSlot({ nodeList: nodes })),

      setPeerList: (peers) =>
        set({ peerList: peers }),

      setTokenUsage: (usage) =>
        set(syncActiveSlot({ tokenUsage: usage })),

      setConnectedWorkdir: (workdir) =>
        set(syncActiveSlot({ connectedWorkdir: workdir })),

      setPlugins: (plugins) =>
        set(syncActiveSlot({ plugins })),

      setAvailableModels: (models) =>
        set(syncActiveSlot({ availableModels: models })),

      setActiveModel: (alias) =>
        set(syncActiveSlot({ activeModel: alias })),

      // ── Heavy mutations (write to BOTH flat proxy + connections[activeId]) ──

      addMessage: (message) =>
        set((state) => {
          const messages = [...state.messages, message];
          const trimmed = messages.length > 300 ? messages.slice(messages.length - 300) : messages;
          return syncHeavyField('messages', trimmed)(state);
        }),

      updateMessage: (id, content) =>
        set((state) => {
          const msgs = state.messages;
          // Streaming message is almost always the last one — scan backward
          for (let i = msgs.length - 1; i >= 0; i--) {
            if (msgs[i].id === id) {
              const copy = [...msgs];
              copy[i] = { ...copy[i], content };
              return syncHeavyField('messages', copy)(state);
            }
          }
          return {};
        }),

      appendToMessage: (id, token) =>
        set((state) => {
          const msgs = state.messages;
          for (let i = msgs.length - 1; i >= 0; i--) {
            if (msgs[i].id === id) {
              const copy = [...msgs];
              copy[i] = { ...copy[i], content: copy[i].content + token };
              return syncHeavyField('messages', copy)(state);
            }
          }
          return {};
        }),

      appendToThinking: (id, token) =>
        set((state) => {
          const msgs = state.messages;
          for (let i = msgs.length - 1; i >= 0; i--) {
            if (msgs[i].id === id) {
              const copy = [...msgs];
              copy[i] = { ...copy[i], thinking: (copy[i].thinking || '') + token };
              return syncHeavyField('messages', copy)(state);
            }
          }
          return {};
        }),

      addToolCall: (toolCall) =>
        set((state) => {
          const toolCalls = [...state.toolCalls, toolCall];
          const trimmed = toolCalls.length > 500 ? toolCalls.slice(toolCalls.length - 500) : toolCalls;
          return syncHeavyField('toolCalls', trimmed)(state);
        }),

      updateToolCall: (id, updates) =>
        set((state) => {
          const tcs = state.toolCalls;
          for (let i = tcs.length - 1; i >= 0; i--) {
            if (tcs[i].id === id) {
              const copy = [...tcs];
              copy[i] = { ...copy[i], ...updates };
              return syncHeavyField('toolCalls', copy)(state);
            }
          }
          return {};
        }),

      addPendingConfirmation: (confirmation) =>
        set((state) => syncHeavyField('pendingConfirmations',
          [...state.pendingConfirmations, confirmation])(state)),

      removePendingConfirmation: (id) =>
        set((state) => syncHeavyField('pendingConfirmations',
          state.pendingConfirmations.filter((c) => c.id !== id))(state)),

      addDiff: (diff) =>
        set((state) => {
          const diffs = [...state.diffs, diff];
          const trimmed = diffs.length > 300 ? diffs.slice(diffs.length - 300) : diffs;
          return syncHeavyField('diffs', trimmed)(state);
        }),

      // ── Global actions (not per-connection) ──

      setCurrentPath: (path) => set({ currentPath: path }),
      setFileList: (files) => set({ fileList: files }),

      removeSessionFromList: (id) =>
        set((state) => ({
          sessionList: state.sessionList.filter((s) => s.id !== id),
        })),

      setConfig: (partial) =>
        set((state) => ({ config: { ...state.config, ...partial } })),

      addPreset: (preset) =>
        set((state) => ({
          presets: [
            ...state.presets,
            {
              ...preset,
              id: preset.id || `preset_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
              createdAt: preset.createdAt || Date.now(),
            },
          ],
        })),

      updatePreset: (id, preset) =>
        set((state) => ({
          presets: state.presets.map((p) =>
            p.id === id ? { ...p, ...preset } : p
          ),
        })),

      deletePreset: (id) =>
        set((state) => ({
          presets: state.presets.filter((p) => p.id !== id),
        })),

      setPresets: (presets) =>
        set({ presets }),

      setWorkflows: (workflows) =>
        set({ workflows }),

      addWorkflow: (wf) =>
        set((state) => ({
          workflows: [...state.workflows.filter(w => w.id !== wf.id), wf],
        })),

      updateWorkflow: (id, partial) =>
        set((state) => ({
          workflows: state.workflows.map((w) =>
            w.id === id ? { ...w, ...partial } : w
          ),
        })),

      deleteWorkflow: (id) =>
        set((state) => ({
          workflows: state.workflows.filter((w) => w.id !== id),
        })),

      setActiveRun: (run) =>
        set({ activeRun: run }),

      applyPreset: (id) => {
        const state = get();
        const preset = state.presets.find((p) => p.id === id);
        if (!preset) return;

        // Resolve nodeRef if preset references a server-side Node
        let resolvedWorkdir = preset.workdir;
        let resolvedIsolation = preset.isolation ?? state.config.isolation;
        let resolvedExecMode = preset.agentMode;

        if (preset.nodeRef && state.nodeList.length > 0) {
          const node = state.nodeList.find(n => n.id === preset.nodeRef);
          if (node) {
            resolvedWorkdir = node.workdir;               // Node overrides workdir
            resolvedIsolation = node.isolation ?? (node.sandbox ? 'sandbox' : 'container');
            resolvedExecMode = (node.exec_mode || preset.agentMode) as 'auto' | 'simple' | 'plan' | 'pipeline';
          }
        }

        const updates: any = {
          serverUrl: preset.serverUrl,
          workdir: resolvedWorkdir,
          config: {
            ...state.config,
            model: preset.model ?? state.config.model,
            autoApprove: preset.autoApprove,
            agentMode: resolvedExecMode as 'auto' | 'simple' | 'plan' | 'pipeline',
            isolation: resolvedIsolation,
            newSessionOnConnect: preset.newSessionOnConnect ?? state.config.newSessionOnConnect,
          },
        };
        set(syncActiveSlot(updates));
      },

      clearSession: () =>
        set((state) => {
          const slotId = state.activeConnectionId;
          const result: any = {
            messages: [],
            toolCalls: [],
            pendingConfirmations: [],
            diffs: [],
            isProcessing: false,
            streamingMessageId: null,
            thinkingMessageId: null,
            currentMessage: '',
            sessionInfo: null,
            sessionList: [],
            sessionRestoreAvailable: null,
            availableModels: state.availableModels,
            activeModel: state.activeModel,
          };
          if (slotId && state.connections[slotId]) {
            result.connections = {
              ...state.connections,
              [slotId]: {
                ...state.connections[slotId],
                messages: [],
                toolCalls: [],
                pendingConfirmations: [],
                diffs: [],
                isProcessing: false,
                streamingMessageId: null,
                thinkingMessageId: null,
                currentMessage: '',
                sessionInfo: null,
                sessionList: [],
                sessionRestoreAvailable: null,
                availableModels: state.connections[slotId].availableModels,
                activeModel: state.connections[slotId].activeModel,
              },
            };
          }
          return result;
        }),

      setClusterToken: (token) => set({ clusterToken: token }),

      addConnectionHistory: (serverUrl, workdir) =>
        set((state) => {
          const existing = state.connectionHistory.find(
            (h) => h.serverUrl === serverUrl && h.workdir === workdir
          );
          if (existing) {
            return {
              connectionHistory: state.connectionHistory.map((h) =>
                h.id === existing.id
                  ? { ...h, lastConnectedAt: Date.now(), connectionCount: h.connectionCount + 1 }
                  : h
              ),
            };
          }
          return {
            connectionHistory: [
              ...state.connectionHistory,
              {
                id: `hist_${Date.now()}`,
                serverUrl,
                workdir,
                connectedAt: Date.now(),
                lastConnectedAt: Date.now(),
                connectionCount: 1,
              },
            ],
          };
        }),

      removeConnectionHistory: (id) =>
        set((state) => ({
          connectionHistory: state.connectionHistory.filter((h) => h.id !== id),
        })),

      clearConnectionHistory: () => set({ connectionHistory: [] }),

      reset: () =>
        set((state) => ({
          connectionStatus: 'disconnected',
          messages: [],
          toolCalls: [],
          pendingConfirmations: [],
          diffs: [],
          isProcessing: false,
          streamingMessageId: null,
          thinkingMessageId: null,
          currentMessage: '',
          sessionInfo: null,
          sessionList: [],
          sessionRestoreAvailable: null,
          connectedWorkdir: null,
          sandboxBackend: 'disabled',
          pendingChanges: 0,
          sandboxChangesData: null,
          nodeList: [],
          tokenUsage: null,
          plugins: [],
          availableModels: [],
          activeModel: null,
          endpoints: [],
        })),
      };
    },
    {
      name: 'rust-agent-connections',
      version: 3,
      partialize: (state) => ({
        connections: state.connections,
        activeConnectionId: state.activeConnectionId,
        serverUrl: state.serverUrl,
        workdir: state.workdir,
        config: state.config,
        presets: state.presets,
        connectionHistory: state.connectionHistory,
        clusterToken: state.clusterToken,
      }),
      merge: (persisted: any, current) => ({
        ...current,
        ...persisted,
        // Ensure connections map is properly hydrated with full slot objects
        connections: persisted.connections
          ? Object.fromEntries(
              Object.entries(persisted.connections as Record<string, any>).map(([k, v]: [string, any]) => {
                const empty = createEmptySlot(k, v.label ?? '', v.serverUrl ?? '', v.workdir);
                return [k, { ...empty, ...v }];
              })
            )
          : current.connections,
      }),
    }
  ))
);
