import { create } from 'zustand';
import { persist, subscribeWithSelector } from 'zustand/middleware';
import { Message, ToolCall, ConnectionStatus, AgentConfig, FileInfo, SessionInfo, SessionMeta, ConfigPreset, VirtualNodeInfo, ConnectionHistory, TokenUsage, PluginInfo, ConnectionSlot } from '../types/agent';
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

// ── Helpers: create empty slot & extract slot state from flat store ──

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
  };
}

/** Fields from the flat state that belong to a connection slot. */
const SLOT_FIELDS = [
  'connectionStatus', 'serverUrl', 'workdir', 'connectedWorkdir',
  'messages', 'toolCalls', 'pendingConfirmations', 'diffs',
  'isProcessing', 'streamingMessageId', 'thinkingMessageId', 'currentMessage',
  'sessionInfo', 'sessionList', 'sandboxBackend', 'pendingChanges',
  'sandboxChangesData', 'tokenUsage', 'nodeList', 'plugins',
] as const;

function extractSlot(state: AgentState): ConnectionSlot {
  const id = state.activeConnectionId ?? 'default';
  return {
    id,
    label: state.connections[id]?.label ?? '',
    ...Object.fromEntries(SLOT_FIELDS.map(f => [f, (state as any)[f]])) as any,
  } as ConnectionSlot;
}

function applySlot(slot: ConnectionSlot): Partial<AgentState> {
  const partial: any = {};
  for (const f of SLOT_FIELDS) {
    partial[f] = (slot as any)[f];
  }
  return partial;
}

// ── State interface ──

interface AgentState {
  // ── Multi-connection slots ──
  connections: Record<string, ConnectionSlot>;
  activeConnectionId: string | null;

  // ── Active connection proxy state (flat, for backward compat selectors) ──
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

  tokenUsage: TokenUsage | null;

  plugins: PluginInfo[];

  // ── Global shared state (not per-connection) ──
  clusterToken: string;
  currentPath: string;
  fileList: FileInfo[];
  config: AgentConfig;
  presets: ConfigPreset[];
  connectionHistory: ConnectionHistory[];

  // ── Connection lifecycle actions ──
  createConnectionSlot: (id: string, label: string, serverUrl: string, workdir?: string) => void;
  removeConnectionSlot: (id: string) => void;
  setActiveConnection: (id: string) => void;
  /** Save current active state into slot without switching. Called before creating new slot. */
  _saveActiveSlot: () => void;
  /** Update a specific slot's data (and flat proxy if it's the active slot).
   *  This is the primary write path for per-slot WebSocket connections. */
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
  addPreset: (preset: Omit<ConfigPreset, 'id' | 'createdAt'>) => void;
  updatePreset: (id: string, preset: Partial<ConfigPreset>) => void;
  deletePreset: (id: string) => void;
  applyPreset: (id: string) => void;
  clearSession: () => void;
  setNodeList: (nodes: VirtualNodeInfo[]) => void;
  setClusterToken: (token: string) => void;
  setConnectedWorkdir: (workdir: string | null) => void;
  setTokenUsage: (usage: TokenUsage) => void;
  addConnectionHistory: (serverUrl: string, workdir?: string) => void;
  removeConnectionHistory: (id: string) => void;
  clearConnectionHistory: () => void;
  setPlugins: (plugins: PluginInfo[]) => void;
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

  tokenUsage: null as TokenUsage | null,

  plugins: [] as PluginInfo[],

  // ── Global shared ──
  clusterToken: '',
  currentPath: '.',
  fileList: [] as FileInfo[],
  presets: (persistedConfig.presets || []) as ConfigPreset[],
  connectionHistory: [] as ConnectionHistory[],
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
    (set, get) => ({
      ...initialState,

      // ── Connection slot management ──

      _saveActiveSlot: () => {
        const state = get();
        const id = state.activeConnectionId;
        if (!id) return;
        const slot = extractSlot(state);
        set({ connections: { ...state.connections, [id]: slot } });
      },

      _updateSlot: (slotId, updater) => {
        const state = get();
        const oldSlot = state.connections[slotId];
        if (!oldSlot) return;
        const updatedSlot = updater(oldSlot);
        const updated = { ...state.connections, [slotId]: updatedSlot };
        if (slotId === state.activeConnectionId) {
          // Active slot — also mirror to flat proxy so UI reacts
          set({ connections: updated, ...applySlot(updatedSlot) });
        } else {
          // Inactive slot — only update the connections map
          set({ connections: updated });
        }
      },

      createConnectionSlot: (id, label, serverUrl, workdir) => {
        const state = get();
        // Save current active slot first
        const currentId = state.activeConnectionId;
        if (currentId) {
          const currentSlot = extractSlot(state);
          const updated = { ...state.connections, [currentId]: currentSlot };
          // Create new slot
          updated[id] = createEmptySlot(id, label, serverUrl, workdir);
          set({ connections: updated });
        } else {
          set({ connections: { ...state.connections, [id]: createEmptySlot(id, label, serverUrl, workdir) } });
        }
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
              ...applySlot(nextSlot),
            });
          } else {
            // No connections left; keep an empty placeholder (no URL — not localhost)
            const emptySlot = createEmptySlot(defaultId, '', '');
            set({
              connections: { [defaultId]: emptySlot },
              activeConnectionId: defaultId,
              ...applySlot(emptySlot),
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
        // Save current
        const currentId = state.activeConnectionId;
        const updated = { ...state.connections };
        if (currentId) {
          updated[currentId] = extractSlot(state);
        }
        // Load target — reconstruct full slot from persisted metadata
        // (persist strips slots to {label,serverUrl,workdir}; missing fields
        // like messages/toolCalls would become undefined and crash the UI)
        let target = updated[id];
        if (!target) return;
        if (!Array.isArray((target as any).messages)) {
          target = createEmptySlot(id, target.label || '', target.serverUrl, target.workdir);
          updated[id] = target;
        }
        set({
          connections: updated,
          activeConnectionId: id,
          ...applySlot(target),
        });
      },

      // ── Existing actions (operate on flat proxy, all unchanged) ──

      setConnectionStatus: (status) => set({ connectionStatus: status }),
      setServerUrl: (url) => {
        set({ serverUrl: url, config: { ...get().config, serverUrl: url } });
      },
      setWorkdir: (workdir) => set({ workdir: workdir || undefined }),

      // ── Sliding window limits ──
      MAX_MESSAGES: 300,
      MAX_TOOL_CALLS: 500,
      MAX_DIFFS: 300,

      addMessage: (message) =>
        set((state) => {
          const messages = [...state.messages, message];
          if (messages.length > 300) {
            return { messages: messages.slice(messages.length - 300) };
          }
          return { messages };
        }),

      updateMessage: (id, content) =>
        set((state) => ({
          messages: state.messages.map((msg) =>
            msg.id === id ? { ...msg, content } : msg
          ),
        })),

      appendToMessage: (id, token) =>
        set((state) => ({
          messages: state.messages.map((msg) =>
            msg.id === id ? { ...msg, content: msg.content + token } : msg
          ),
        })),

      setCurrentMessage: (message) => set({ currentMessage: message }),
      setIsProcessing: (processing) => set({ isProcessing: processing }),
      setStreamingMessageId: (id) => set({ streamingMessageId: id }),
      setThinkingMessageId: (id) => set({ thinkingMessageId: id }),

      appendToThinking: (id, token) =>
        set((state) => ({
          messages: state.messages.map((msg) =>
            msg.id === id ? { ...msg, thinking: (msg.thinking || '') + token } : msg
          ),
        })),

      addToolCall: (toolCall) =>
        set((state) => {
          const toolCalls = [...state.toolCalls, toolCall];
          if (toolCalls.length > 500) {
            return { toolCalls: toolCalls.slice(toolCalls.length - 500) };
          }
          return { toolCalls };
        }),

      updateToolCall: (id, updates) =>
        set((state) => ({
          toolCalls: state.toolCalls.map((call) =>
            call.id === id ? { ...call, ...updates } : call
          ),
        })),

      addPendingConfirmation: (confirmation) =>
        set((state) => ({
          pendingConfirmations: [...state.pendingConfirmations, confirmation],
        })),

      removePendingConfirmation: (id) =>
        set((state) => ({
          pendingConfirmations: state.pendingConfirmations.filter((c) => c.id !== id),
        })),

      addDiff: (diff) =>
        set((state) => {
          const diffs = [...state.diffs, diff];
          if (diffs.length > 300) {
            return { diffs: diffs.slice(diffs.length - 300) };
          }
          return { diffs };
        }),

      setSandboxBackend: (backend) => set({ sandboxBackend: backend }),
      setPendingChanges: (count) => set({ pendingChanges: count }),
      setSandboxChangesData: (data) => set({ sandboxChangesData: data }),

      setCurrentPath: (path) => set({ currentPath: path }),
      setFileList: (files) => set({ fileList: files }),
      setSessionInfo: (info) => set({ sessionInfo: info }),
      setSessionList: (list) => set({ sessionList: list }),
      removeSessionFromList: (id) => set((state) => ({
        sessionList: state.sessionList.filter(s => s.id !== id),
      })),

      setSessionRestoreAvailable: (info) => set({ sessionRestoreAvailable: info }),

      setConfig: (config) =>
        set((state) => ({ config: { ...state.config, ...config } })),

      addPreset: (preset) => {
        // Validate required fields
        if (!preset.name?.trim() || !preset.serverUrl?.trim()) return;
        try { new URL(preset.serverUrl.replace(/^ws/, 'http')); } catch { return; }
        const newPreset: ConfigPreset = {
          ...preset,
          id: Date.now().toString(),
          createdAt: Date.now(),
        };
        set((state) => ({ presets: [...state.presets, newPreset] }));
      },

      updatePreset: (id, updates) =>
        set((state) => ({
          presets: state.presets.map(p => p.id === id ? { ...p, ...updates } : p),
        })),

      deletePreset: (id) =>
        set((state) => ({
          presets: state.presets.filter(p => p.id !== id),
        })),

      applyPreset: (id) => {
        const state = get();
        const preset = state.presets.find(p => p.id === id);
        if (preset) {
          set({
            serverUrl: preset.serverUrl,
            workdir: preset.workdir,
            config: {
              ...state.config,
              serverUrl: preset.serverUrl,
              workdir: preset.workdir,
              ...(preset.model !== undefined && preset.model !== '' ? { model: preset.model } : {}),
              autoApprove: preset.autoApprove,
              agentMode: preset.agentMode,
              isolation: preset.isolation || 'container',
            },
          });
        }
      },

      setNodeList: (nodes) => set({ nodeList: nodes }),
      setClusterToken: (token) => set({ clusterToken: token }),
      setConnectedWorkdir: (workdir) => set({ connectedWorkdir: workdir }),

      setTokenUsage: (usage) => set({ tokenUsage: usage }),

      addConnectionHistory: (serverUrl, workdir) => {
        const now = Date.now();
        set((state) => {
          const existingIndex = state.connectionHistory.findIndex(
            h => h.serverUrl === serverUrl && h.workdir === workdir
          );
          if (existingIndex >= 0) {
            const updatedHistory = [...state.connectionHistory];
            updatedHistory[existingIndex] = {
              ...updatedHistory[existingIndex],
              lastConnectedAt: now,
              connectionCount: updatedHistory[existingIndex].connectionCount + 1
            };
            return { connectionHistory: updatedHistory };
          } else {
            const newHistory: ConnectionHistory = {
              id: Date.now().toString(),
              serverUrl,
              workdir,
              connectedAt: now,
              lastConnectedAt: now,
              connectionCount: 1
            };
            return { connectionHistory: [newHistory, ...state.connectionHistory].slice(0, 20) };
          }
        });
      },

      removeConnectionHistory: (id) =>
        set((state) => ({
          connectionHistory: state.connectionHistory.filter(h => h.id !== id),
        })),

      clearConnectionHistory: () => set({ connectionHistory: [] }),

      setPlugins: (plugins) => set({ plugins }),

      clearSession: () => set({
        messages: [],
        toolCalls: [],
        pendingConfirmations: [],
        diffs: [],
        isProcessing: false,
        streamingMessageId: null,
        thinkingMessageId: null,
        currentMessage: '',
        tokenUsage: null,
        sessionRestoreAvailable: null,
      }),

      reset: () => set((state) => ({
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
        sessionRestoreAvailable: null,
        tokenUsage: null,
      })),
    }),
    {
      name: 'rust-agent-config',
      partialize: (state) => {
        // Persist settings only; connection slots are ephemeral (recreated each session)
        return {
          serverUrl: state.serverUrl,
          workdir: state.workdir,
          clusterToken: state.clusterToken,
          config: state.config,
          presets: state.presets,
          nodeList: state.nodeList,
          connectionHistory: state.connectionHistory,
        };
      },
    }
  )
  )
);
