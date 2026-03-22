import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { Message, ToolCall, ConnectionStatus, AgentConfig, FileInfo, SessionInfo, SessionMeta, ConfigPreset } from '../types/agent';
import { getDefaultServerUrl, getDefaultWorkdir, isDesktopApp } from '../utils/environment';

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

interface AgentState {
  // 连接状态
  connectionStatus: ConnectionStatus;
  serverUrl: string;
  workdir?: string;
  
  // 对话状态
  messages: Message[];
  currentMessage: string;
  isProcessing: boolean;
  streamingMessageId: string | null;
  
  // 工具调用
  toolCalls: ToolCall[];
  pendingConfirmations: PendingConfirmation[];
  
  // Diff
  diffs: DiffEntry[];

  // Session info (saved on server)
  sessionInfo: SessionInfo | null;
  sessionList: SessionMeta[];
  
  // 文件浏览
  currentPath: string;
  fileList: FileInfo[];
  
  // 配置
  config: AgentConfig;
  
  // 配置预设
  presets: ConfigPreset[];
  
  // Actions
  setConnectionStatus: (status: ConnectionStatus) => void;
  setServerUrl: (url: string) => void;
  setWorkdir: (workdir: string) => void;
  addMessage: (message: Message) => void;
  updateMessage: (id: string, content: string) => void;
  appendToMessage: (id: string, token: string) => void;
  setCurrentMessage: (message: string) => void;
  setIsProcessing: (processing: boolean) => void;
  setStreamingMessageId: (id: string | null) => void;
  addToolCall: (toolCall: ToolCall) => void;
  updateToolCall: (id: string, updates: Partial<ToolCall>) => void;
  addPendingConfirmation: (confirmation: PendingConfirmation) => void;
  removePendingConfirmation: (id: string) => void;
  addDiff: (diff: DiffEntry) => void;
  setCurrentPath: (path: string) => void;
  setFileList: (files: FileInfo[]) => void;
  setSessionInfo: (info: SessionInfo | null) => void;
  setSessionList: (list: SessionMeta[]) => void;
  removeSessionFromList: (id: string) => void;
  setConfig: (config: Partial<AgentConfig>) => void;
  addPreset: (preset: Omit<ConfigPreset, 'id' | 'createdAt'>) => void;
  updatePreset: (id: string, preset: Partial<ConfigPreset>) => void;
  deletePreset: (id: string) => void;
  applyPreset: (id: string) => void;
  clearSession: () => void;
  reset: () => void;
}

// Load persisted config from localStorage
const loadPersistedConfig = (): Partial<AgentConfig & { serverUrl: string; workdir?: string; presets: ConfigPreset[] }> => {
  const defaultServerUrl = getDefaultServerUrl();
  
  try {
    const stored = localStorage.getItem('rust-agent-config');
    if (stored) {
      // Zustand persist saves as { state: { ... }, version: N }
      const parsed = JSON.parse(stored);
      const data = parsed.state ?? parsed; // support both wrapped and legacy flat format
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

const initialState = {
  connectionStatus: 'disconnected' as ConnectionStatus,
  serverUrl: persistedConfig.serverUrl || getDefaultServerUrl(),
  workdir: persistedConfig.workdir,
  messages: [],
  currentMessage: '',
  isProcessing: false,
  streamingMessageId: null,
  toolCalls: [],
  pendingConfirmations: [],
  diffs: [],
  currentPath: '.',
  fileList: [],
  sessionInfo: null,
  sessionList: [],
  presets: persistedConfig.presets || [],
  config: {
    serverUrl: persistedConfig.serverUrl || getDefaultServerUrl(),
    autoApprove: persistedConfig.autoApprove ?? false,
    agentMode: persistedConfig.agentMode || ('auto' as const),
  },
};

export const useAgentStore = create<AgentState>()(
  persist(
    (set, get) => ({
      ...initialState,
      
      setConnectionStatus: (status) => set({ connectionStatus: status }),
      setServerUrl: (url) => {
        set({ serverUrl: url, config: { ...get().config, serverUrl: url } });
      },
      setWorkdir: (workdir) => set({ workdir }),
  
  addMessage: (message) => 
    set((state) => ({ messages: [...state.messages, message] })),
    
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
  
  addToolCall: (toolCall) =>
    set((state) => ({ toolCalls: [...state.toolCalls, toolCall] })),
    
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
    set((state) => ({ diffs: [...state.diffs, diff] })),
    
  setCurrentPath: (path) => set({ currentPath: path }),
  setFileList: (files) => set({ fileList: files }),
  setSessionInfo: (info) => set({ sessionInfo: info }),
  setSessionList: (list) => set({ sessionList: list }),
  removeSessionFromList: (id) => set((state) => ({
    sessionList: state.sessionList.filter(s => s.id !== id),
  })),

  setConfig: (config) =>
    set((state) => ({ config: { ...state.config, ...config } })),
  
  addPreset: (preset) => {
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
          serverUrl: preset.serverUrl,
          model: preset.model,
          autoApprove: preset.autoApprove,
          agentMode: preset.agentMode,
        },
      });
    }
  },
  
  clearSession: () => set({
    messages: [],
    toolCalls: [],
    pendingConfirmations: [],
    diffs: [],
    isProcessing: false,
    streamingMessageId: null,
    currentMessage: '',
  }),
    
  // reset only clears session/conversation state, preserving connection settings and presets
  reset: () => set((state) => ({
    connectionStatus: 'disconnected',
    messages: [],
    toolCalls: [],
    pendingConfirmations: [],
    diffs: [],
    isProcessing: false,
    streamingMessageId: null,
    currentMessage: '',
    sessionInfo: null,
    // preserve: serverUrl, workdir, config, presets
  })),
}),
    {
      name: 'rust-agent-config',
      partialize: (state) => ({
        serverUrl: state.serverUrl,
        workdir: state.workdir,
        config: state.config,
        presets: state.presets,
      }),
    }
  )
);