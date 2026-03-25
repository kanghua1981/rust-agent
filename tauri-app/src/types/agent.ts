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
  | CancelMessage;

export interface CancelMessage extends BaseMessage {
  type: 'cancel';
  data: {};
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
  | SessionClearedEvent
  | SessionsListEvent
  | SessionDeletedEvent
  | SandboxStatusEvent
  | SandboxChangesResultEvent
  | SandboxCommitResultEvent
  | SandboxCommitFileResultEvent
  | SandboxRollbackResultEvent
  | CancelledEvent;

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
    id?: string;
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

export interface DoneEvent extends BaseMessage {
  type: 'done';
  data: {
    text: string;
    id?: string;
    pending_changes?: number;
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
  };
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
  createdAt: number;
}