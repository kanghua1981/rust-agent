// Agent WebSocket 协议类型定义

// 客户端发送给服务器的消息
export type ClientMessage = 
  | UserMessage
  | ConfirmResponse
  | AskUserResponse
  | ReviewPlanResponse
  | SetWorkdirMessage
  | SetModelMessage
  | SetModeMessage
  | SetSandboxMessage
  | SandboxListChangesMessage
  | SandboxCommitMessage
  | SandboxCommitFileMessage
  | SandboxRollbackMessage
  | LoadSessionMessage
  | NewSessionMessage
  | ListSessionsMessage
  | DeleteSessionMessage
  | LoadSessionByIdMessage
  | CancelMessage
  | UploadFileMessage
  | ListPluginsMessage
  | EnablePluginMessage
  | DisablePluginMessage
  | FetchModelsMessage
  | AddModelMessage
  | DeleteModelMessage
  | ListEndpointsMessage
  | AddEndpointMessage
  | DeleteEndpointMessage
  | ListPresetsMessage
  | SavePresetMessage
  | DeletePresetMessage
  | ListWorkflowsMessage
  | GetWorkflowMessage
  | SaveWorkflowMessage
  | DeleteWorkflowMessage
  | RunWorkflowMessage;

export interface CancelMessage extends BaseMessage {
  type: 'cancel';
  data: {};
}

export interface UploadFileMessage extends BaseMessage {
  type: 'upload_file';
  data: {
    /** File name (basename only, no path separators) */
    name: string;
    /** Base64-encoded file content */
    content: string;
    /** Optional MIME type hint */
    mime_type?: string;
    /** Optional target subdirectory under uploads/ */
    target_dir?: string;
  };
}

export interface LoadSessionMessage extends BaseMessage {
  type: 'load_session';
  data: {};
}

export interface NewSessionMessage extends BaseMessage {
  type: 'new_session';
  data: {};
}

export interface ListSessionsMessage extends BaseMessage {
  type: 'list_sessions';
  data: {};
}

export interface DeleteSessionMessage extends BaseMessage {
  type: 'delete_session';
  data: { id: string };
}

export interface LoadSessionByIdMessage extends BaseMessage {
  type: 'load_session_by_id';
  data: { id: string };
}

// 服务器发送给客户端的事件
export type ServerEvent =
  | ThinkingEvent
  | ThinkingStartEvent
  | ThinkingTokenEvent
  | ThinkingEndEvent
  | StreamStartEvent
  | StreamingTokenEvent
  | StreamEndEvent
  | AssistantTextEvent
  | ToolUseEvent
  | ToolResultEvent
  | DiffEvent
  | ConfirmRequestEvent
  | AskUserEvent
  | ReviewPlanEvent
  | WarningEvent
  | ErrorEvent
  | ContextWarningEvent
  | DoneEvent
  | ReadyEvent
  | PongEvent
  | RoleHeaderEvent
  | StageEndEvent
  | SessionInfoEvent
  | SessionRestoredEvent
  | SessionAvailableEvent
  | SessionClearedEvent
  | SessionsListEvent
  | SessionDeletedEvent
  | SandboxStatusEvent
  | SandboxChangesResultEvent
  | SandboxCommitResultEvent
  | SandboxCommitFileResultEvent
  | SandboxRollbackResultEvent
  | CancelledEvent
  | UploadFileResultEvent
  | AgentEvent
  | PluginsListEvent
  | ModelChangedEvent
  | ModelStateEvent
  | EndpointsListEvent
  | ModelsFetchedEvent
  | ModelAddedEvent
  | ModelDeletedEvent
  | EndpointAddedEvent
  | EndpointDeletedEvent
  | PresetsListEvent
  | PresetSavedEvent
  | PresetDeletedEvent
  | WorkflowsListEvent
  | WorkflowLoadedEvent
  | WorkflowSavedEvent
  | WorkflowDeletedEvent
  | WorkflowStartedEvent
  | WorkflowCompleteEvent
  | WorkflowErrorEvent;

export interface SessionMeta {
  id: string;
  summary: string;
  updated_at: string;
  message_count: number;
  working_dir: string;
}

export interface SessionInfo {
  exists: boolean;
  message_count?: number;
  updated_at?: string;
  summary?: string;
  working_dir?: string;
}

export interface SessionsListEvent extends BaseMessage {
  type: 'sessions_list';
  data: { sessions: SessionMeta[] };
}

export interface SessionDeletedEvent extends BaseMessage {
  type: 'session_deleted';
  data: { id: string };
}

export interface SandboxStatusEvent extends BaseMessage {
  type: 'sandbox_status';
  data: {
    enabled: boolean;
    backend: 'overlay' | 'snapshot' | 'disabled';
    pending_changes?: number;
  };
}

export interface SandboxChangesResultEvent extends BaseMessage {
  type: 'sandbox_changes_result';
  data: {
    files: Array<{
      path: string;
      kind: 'modified' | 'created' | 'deleted' | 'unchanged';
      diff: string | null;
      original_size: number | null;
      current_size: number | null;
    }>;
    backend: string;
    pending_changes: number;
  };
}

export interface SandboxCommitResultEvent extends BaseMessage {
  type: 'sandbox_commit_result';
  data: { modified: number; created: number };
}

export interface SandboxCommitFileResultEvent extends BaseMessage {
  type: 'sandbox_commit_file_result';
  data: { file_path: string; modified: number; created: number };
}

export interface SandboxRollbackResultEvent extends BaseMessage {
  type: 'sandbox_rollback_result';
  data: { restored: number; deleted: number; errors: string[] };
}

export interface SandboxListChangesMessage extends BaseMessage {
  type: 'sandbox_list_changes';
  data: {};
}

export interface SandboxCommitMessage extends BaseMessage {
  type: 'sandbox_commit';
  data: {};
}

export interface SandboxCommitFileMessage extends BaseMessage {
  type: 'sandbox_commit_file';
  data: {
    file_path: string;
  };
}

export interface SandboxRollbackMessage extends BaseMessage {
  type: 'sandbox_rollback';
  data: {};
}

export interface SessionInfoEvent extends BaseMessage {
  type: 'session_info';
  data: SessionInfo;
}

export interface SessionRestoredMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export interface SessionRestoredEvent extends BaseMessage {
  type: 'session_restored';
  data: {
    message_count: number;
    messages: SessionRestoredMessage[];
  };
}

// Auto-restore notification (metadata only — no messages).
// Sent by the worker on connect when a local session exists.
// The UI shows a "restore available" banner; clicking it sends
// a load_session command which re-emits session_restored with full messages.
export interface SessionAvailableEvent extends BaseMessage {
  type: 'session_available';
  data: {
    message_count: number;
    session_id: string;
  };
}

export interface SessionClearedEvent extends BaseMessage {
  type: 'session_cleared';
  data: {
    message: string;
  };
}

// 基础消息结构
interface BaseMessage {
  type: string;
  data?: any;
  id?: string;
  /** Monotonically increasing event sequence number (for reconnection recovery). */
  seq?: number;
}

// 客户端消息类型
export interface UserMessage extends BaseMessage {
  type: 'user_message';
  data: {
    text: string;
    workdir?: string;  // 可选的工作目录
    model?: string;    // 可选的模型
  };
}

export interface ConfirmResponse extends BaseMessage {
  type: 'confirm_response';
  data: {
    approved: boolean;
    tool_id?: string;
  };
}

export interface AskUserResponse extends BaseMessage {
  type: 'ask_user_response';
  data: {
    answer: string;
  };
}

export interface ReviewPlanResponse extends BaseMessage {
  type: 'review_plan_response';
  data: {
    approved: boolean;
    feedback?: string;
  };
}

export interface SetWorkdirMessage extends BaseMessage {
  type: 'set_workdir';
  data: {
    workdir: string;
  };
}

export interface SetModelMessage extends BaseMessage {
  type: 'set_model';
  data: {
    model: string;
  };
}

export interface SetSandboxMessage extends BaseMessage {
  type: 'set_sandbox';
  data: {
    enabled: boolean;
  };
}

export interface SetModeMessage extends BaseMessage {
  type: 'set_mode';
  data: {
    mode: 'auto' | 'simple' | 'plan' | 'pipeline';
  };
}

// 服务器事件类型
export interface ThinkingEvent extends BaseMessage {
  type: 'thinking';
  data: {};
}

export interface ThinkingStartEvent extends BaseMessage {
  type: 'thinking_start';
  data: {};
}

export interface ThinkingTokenEvent extends BaseMessage {
  type: 'thinking_token';
  data: {
    token: string;
  };
}

export interface ThinkingEndEvent extends BaseMessage {
  type: 'thinking_end';
  data: {};
}

export interface RoleHeaderEvent extends BaseMessage {
  type: 'role_header';
  data: { label: string; model: string };
}

export interface StageEndEvent extends BaseMessage {
  type: 'stage_end';
  data: { label: string };
}

export interface StreamStartEvent extends BaseMessage {
  type: 'stream_start';
  data: {};
}

export interface StreamingTokenEvent extends BaseMessage {
  type: 'streaming_token';
  data: {
    token: string;
  };
}

export interface StreamEndEvent extends BaseMessage {
  type: 'stream_end';
  data: {};
}

export interface AssistantTextEvent extends BaseMessage {
  type: 'assistant_text';
  data: {
    text: string;
  };
}

export interface ToolUseEvent extends BaseMessage {
  type: 'tool_use';
  data: {
    tool: string;
    input: any;
  };
}

export interface ToolResultEvent extends BaseMessage {
  type: 'tool_result';
  data: {
    tool: string;
    output: string;
    is_error: boolean;
  };
}

export interface DiffEvent extends BaseMessage {
  type: 'diff';
  data: {
    path: string;
    diff: string;
  };
}

export interface ConfirmRequestEvent extends BaseMessage {
  type: 'confirm_request';
  data: {
    action: string;
    details?: string;
    tool_id?: string;
  };
}

export interface AskUserEvent extends BaseMessage {
  type: 'ask_user';
  data: {
    question: string;
  };
}

export interface ReviewPlanEvent extends BaseMessage {
  type: 'review_plan';
  data: {
    plan: string;
  };
}

export interface WarningEvent extends BaseMessage {
  type: 'warning';
  data: {
    message: string;
  };
}

export interface ErrorEvent extends BaseMessage {
  type: 'error';
  data: {
    message: string;
  };
}

export interface ContextWarningEvent extends BaseMessage {
  type: 'context_warning';
  data: {
    message: string;
  };
}

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  role_usage?: Record<string, [number, number]>; // role -> [input, output]
}

export interface DoneEvent extends BaseMessage {
  type: 'done';
  data: {
    text: string;
    pending_changes?: number;
    input_tokens?: number;
    output_tokens?: number;
    role_usage?: Record<string, [number, number]>;
  };
}

export interface ReadyEvent extends BaseMessage {
  type: 'ready';
  data: {
    version: string;
    workdir?: string;
    isolation?: 'normal' | 'container' | 'sandbox';
    sandbox?: boolean;  // legacy, kept for backward compat
    sandbox_backend?: 'overlay' | 'snapshot' | 'disabled';
    caps?: NodeCapabilities;
    virtual_nodes?: VirtualNodeInfo[];
    available_models?: ModelInfo[];
    active_model?: string | null;
  };
}

export interface ModelInfo {
  alias: string;
  provider: string;
  model: string;
  base_url?: string | null;
  endpoint?: string | null;
  thinking_enabled?: boolean | null;
  reasoning_effort?: string | null;
}

export interface EndpointInfo {
  name: string;
  provider: string;
  base_url: string;
  has_api_key: boolean;
}

export interface NodeCapabilities {
  arch: string;
  os: string;
  cpu_cores: number;
  ram_gb: number;
  gpus: Array<{ name: string }>;
  bins: string[];
}

export interface VirtualNodeInfo {
  name: string;
  workdir: string;
  description: string;
  isolation: 'normal' | 'container' | 'sandbox';
  sandbox: boolean;  // legacy, kept for backward compat
  tags: string[];
}

export interface PongEvent extends BaseMessage {
  type: 'pong';
  data: {};
}

export interface CancelledEvent extends BaseMessage {
  type: 'cancelled';
  data: { message: string };
}

// ── Sub-agent / nested agent events ──
/** Replaces the 7 sub_* event types with a single agent_event type that wraps standard events. */
export interface AgentEvent extends BaseMessage {
  type: 'agent_event';
  data: {
    agent_id: string;
    parent_id?: string;  // reserved for nested sub-sub-agents
    event: {
      type: string;
      data: Record<string, unknown>;
    };
  };
}

export interface UploadFileResultEvent extends BaseMessage {
  type: 'upload_file_result';
  data: {
    /** Whether the upload succeeded */
    success: boolean;
    /** Original file name */
    name?: string;
    /** Resulting path relative to project directory (on success) */
    path?: string;
    /** File size in bytes (on success) */
    size?: number;
    /** Error message (on failure) */
    error?: string;
  };
}

// ── 插件系统 ──
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  enabled: boolean;
  tools: string[]; // tool names provided by this plugin
  author?: string;
  homepage?: string;
}

export interface ListPluginsMessage extends BaseMessage {
  type: 'list_plugins';
  data: {};
}

export interface EnablePluginMessage extends BaseMessage {
  type: 'enable_plugin';
  data: { id: string };
}

export interface DisablePluginMessage extends BaseMessage {
  type: 'disable_plugin';
  data: { id: string };
}

export interface FetchModelsMessage extends BaseMessage {
  type: 'fetch_models';
  data: { url: string; api_key?: string };
}

export interface AddModelMessage extends BaseMessage {
  type: 'add_model';
  data: { alias: string; model: string; endpoint: string };
}

export interface DeleteModelMessage extends BaseMessage {
  type: 'delete_model';
  data: { alias: string };
}

export interface ListEndpointsMessage extends BaseMessage {
  type: 'list_endpoints';
  data: {};
}

export interface AddEndpointMessage extends BaseMessage {
  type: 'add_endpoint';
  data: { name: string; provider: string; base_url: string; api_key?: string };
}

export interface DeleteEndpointMessage extends BaseMessage {
  type: 'delete_endpoint';
  data: { name: string };
}

// ── Preset CRUD messages ───────────────────────────────────────────
export interface ListPresetsMessage extends BaseMessage {
  type: 'list_presets';
  data: {};
}

export interface SavePresetMessage extends BaseMessage {
  type: 'save_preset';
  data: Record<string, any>;  // Preset fields (camelCase)
}

export interface DeletePresetMessage extends BaseMessage {
  type: 'delete_preset';
  data: { id: string };
}

// ── Workflow CRUD messages ─────────────────────────────────────────
export interface ListWorkflowsMessage extends BaseMessage {
  type: 'list_workflows';
  data: {};
}

export interface GetWorkflowMessage extends BaseMessage {
  type: 'get_workflow';
  data: { id: string };
}

export interface SaveWorkflowMessage extends BaseMessage {
  type: 'save_workflow';
  data: Record<string, any>;  // Workflow fields
}

export interface DeleteWorkflowMessage extends BaseMessage {
  type: 'delete_workflow';
  data: { id: string };
}

export interface RunWorkflowMessage extends BaseMessage {
  type: 'run_workflow';
  data: { workflowId: string; task: string };
}

export interface PluginsListEvent extends BaseMessage {
  type: 'plugins_list';
  data: { plugins: PluginInfo[] };
}

export interface ModelChangedEvent extends BaseMessage {
  type: 'model_changed';
  data: {
    alias: string;
    model: string;
    provider: string;
    endpoint?: string | null;
  };
}

export interface ModelStateEvent extends BaseMessage {
  type: 'model_state';
  data: {
    models: ModelInfo[];
    endpoints: EndpointInfo[];
    default: string | null;
  };
}

export interface EndpointsListEvent extends BaseMessage {
  type: 'endpoints_list';
  data: { endpoints: EndpointInfo[] };
}

export interface ModelsFetchedEvent extends BaseMessage {
  type: 'models_fetched';
  data: {
    models: string[];
    source: string;
    url: string;
  };
}

export interface ModelAddedEvent extends BaseMessage {
  type: 'model_added';
  data: { alias: string; model: string; endpoint: string };
}

export interface ModelDeletedEvent extends BaseMessage {
  type: 'model_deleted';
  data: { alias: string };
}

export interface EndpointAddedEvent extends BaseMessage {
  type: 'endpoint_added';
  data: { name: string; base_url: string };
}

export interface EndpointDeletedEvent extends BaseMessage {
  type: 'endpoint_deleted';
  data: { name: string };
}

// ── Preset events ───────────────────────────────────────────────────
export interface PresetsListEvent extends BaseMessage {
  type: 'presets_list';
  data: { presets: any[] };
}

export interface PresetSavedEvent extends BaseMessage {
  type: 'preset_saved';
  data: { preset: any };
}

export interface PresetDeletedEvent extends BaseMessage {
  type: 'preset_deleted';
  data: { id: string };
}

// ── Workflow events ─────────────────────────────────────────────────
export interface WorkflowsListEvent extends BaseMessage {
  type: 'workflows_list';
  data: { workflows: any[] };
}

export interface WorkflowLoadedEvent extends BaseMessage {
  type: 'workflow_loaded';
  data: { workflow: any };
}

export interface WorkflowSavedEvent extends BaseMessage {
  type: 'workflow_saved';
  data: { workflow: any };
}

export interface WorkflowDeletedEvent extends BaseMessage {
  type: 'workflow_deleted';
  data: { id: string };
}

export interface WorkflowStartedEvent extends BaseMessage {
  type: 'workflow_started';
  data: { workflowId: string; task: string };
}

export interface WorkflowCompleteEvent extends BaseMessage {
  type: 'workflow_complete';
  data: { run: WorkflowRunResult };
}

export interface WorkflowErrorEvent extends BaseMessage {
  type: 'workflow_error';
  data: { message: string };
}

// ── Workflow data types ─────────────────────────────────────────────
export interface WorkflowStage {
  id: string;
  workflowId: string;
  presetId?: string;
  stageOrder: number;
  stageGroup: string;
  inputTemplate: string;
  outputKey?: string;
  condition: string;
  timeoutSecs: number;
  retryCount: number;
  autoApprove: boolean;
}

export interface WorkflowDef {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  defaultTimeout: number;
  stages: WorkflowStage[];
  createdAt: string;
  updatedAt: string;
}

// 工具类型定义
export interface ToolCall {
  id: string;
  tool: string;
  input: any;
  output?: string;
  status: 'pending' | 'executing' | 'completed' | 'error';
  timestamp: number;
  messageId?: string;  // ID of the assistant message that owns this tool call
}

// 消息类型
export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  toolCalls?: ToolCall[];
  // Optional thinking content (streamed from thinking_start/thinking_token/thinking_end)
  thinking?: string;
  // Optional metadata for special system messages (e.g. pipeline stage headers)
  meta?: { stageLabel?: string; stageModel?: string; stageEnd?: boolean };
}

// 连接状态
export type ConnectionStatus = 
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'error';

// Agent 配置
export interface AgentConfig {
  serverUrl: string;
  workdir?: string;
  model?: string;
  autoApprove?: boolean;
  agentMode?: 'auto' | 'simple' | 'plan' | 'pipeline';
  isolation?: 'normal' | 'container' | 'sandbox';
  newSessionOnConnect?: boolean;
}

// 文件信息
export interface FileInfo {
  name: string;
  path: string;
  type: 'file' | 'directory';
  size?: number;
  modified?: number;
}

// 配置预设
export interface ConfigPreset {
  id: string;
  name: string;
  serverUrl: string;
  workdir?: string;
  model?: string;
  autoApprove: boolean;
  agentMode: 'auto' | 'simple' | 'plan' | 'pipeline';
  isolation?: 'normal' | 'container' | 'sandbox';
  newSessionOnConnect?: boolean;
  newSession?: boolean;  // from Rust backend (camelCase of new_session)
  icon?: string;
  color?: string;
  tags?: string[];
  sortOrder?: number;
  createdAt: number;
  updatedAt?: string;
}

// 连接历史记录
export interface ConnectionHistory {
  id: string;
  serverUrl: string;
  workdir?: string;
  connectedAt: number;
  lastConnectedAt: number;
  connectionCount: number;
}

// ── 多工程 Tab 系统：每个连接槽位的独立状态 ──
export interface ConnectionSlot {
  id: string;                          // 唯一连接 ID
  label: string;                       // Tab 标签（如 "project-name :8080"）
  connectionStatus: ConnectionStatus;
  serverUrl: string;
  workdir?: string;
  connectedWorkdir: string | null;
  messages: Message[];
  toolCalls: ToolCall[];
  pendingConfirmations: PendingConfirmation[];  // stored here only for type; kept in store level
  diffs: DiffEntry[];                 // stored here only for type
  isProcessing: boolean;
  streamingMessageId: string | null;
  thinkingMessageId: string | null;
  currentMessage: string;
  sessionInfo: SessionInfo | null;
  sessionList: SessionMeta[];
  sandboxBackend: string;
  pendingChanges: number;
  sandboxChangesData: SandboxFileChange[] | null;
  tokenUsage: TokenUsage | null;
  nodeList: VirtualNodeInfo[];
  plugins: PluginInfo[];
  sessionRestoreAvailable: { message_count: number } | null;
  availableModels: ModelInfo[];
  activeModel: string | null;
  endpoints: EndpointInfo[];
}

export interface PendingConfirmation {
  id: string;
  action: string;
  details?: string;
  type: 'confirm' | 'ask_user' | 'review_plan';
}

export interface DiffEntry {
  id: string;
  path: string;
  diff: string;
  timestamp: number;
}

export interface SandboxFileChange {
  path: string;
  kind: 'modified' | 'created' | 'deleted' | 'unchanged';
  original_size: number | null;
  current_size: number | null;
  diff: string | null;
}

// ═══════════════════════════════════════════════════════════════
//  Workflow Run Types
// ═══════════════════════════════════════════════════════════════

export interface StageRunResult {
  id: string;
  runId: string;
  stageId: string;
  stageOrder: number;
  presetName?: string;
  status: 'pending' | 'running' | 'success' | 'failed' | 'skipped';
  inputPrompt?: string;
  outputText?: string;
  outputSummary?: string;
  tokensUsed: number;
  toolCalls: string[];
  startedAt?: string;
  finishedAt?: string;
  errorMessage?: string;
  retryAttempt: number;
}

export interface WorkflowRunResult {
  id: string;
  workflowId: string;
  workflowName: string;
  status: string;
  task: string;
  startedAt?: string;
  finishedAt?: string;
  totalTokens: number;
  errorMessage?: string;
  stageResults: StageRunResult[];
}