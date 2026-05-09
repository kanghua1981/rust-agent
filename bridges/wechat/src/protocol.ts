// Agent WebSocket 协议类型定义
//
// 精简自 web-ui/src/types/agent.ts，仅包含 Bridge 需要的消息类型。
// 如果上游协议变更，以 web-ui/src/types/agent.ts 为准同步更新。

// ═══════════════════════════════════════════════════════════════════
// 基础结构
// ═══════════════════════════════════════════════════════════════════

export interface BaseMessage {
  type: string;
  data?: any;
  id?: string;
}

// ═══════════════════════════════════════════════════════════════════
// 客户端 → 服务器 消息
// ═══════════════════════════════════════════════════════════════════

export interface UserMessage {
  type: 'user_message';
  data: {
    text: string;
    workdir?: string;
    model?: string;
  };
}

export interface ConfirmResponse {
  type: 'confirm_response';
  data: {
    approved: boolean;
    tool_id?: string;
  };
}

export interface AskUserResponse {
  type: 'ask_user_response';
  data: {
    answer: string;
  };
}

export type ClientMessage =
  | UserMessage
  | ConfirmResponse
  | AskUserResponse;

// ═══════════════════════════════════════════════════════════════════
// 服务器 → 客户端 事件
// ═══════════════════════════════════════════════════════════════════

export interface ReadyEvent {
  type: 'ready';
  data: {
    version: string;
    workdir?: string;
    isolation?: 'normal' | 'container' | 'sandbox';
    sandbox?: boolean;
    sandbox_backend?: 'overlay' | 'snapshot' | 'disabled';
  };
}

export interface ThinkingStartEvent {
  type: 'thinking_start';
  data: {};
}

export interface ThinkingTokenEvent {
  type: 'thinking_token';
  data: {
    token: string;
  };
}

export interface ThinkingEndEvent {
  type: 'thinking_end';
  data: {};
}

export interface StreamStartEvent {
  type: 'stream_start';
  data: {};
}

export interface StreamingTokenEvent {
  type: 'streaming_token';
  data: {
    token: string;
  };
}

export interface StreamEndEvent {
  type: 'stream_end';
  data: {};
}

export interface AssistantTextEvent {
  type: 'assistant_text';
  data: {
    text: string;
  };
}

export interface ToolUseEvent {
  type: 'tool_use';
  data: {
    tool: string;
    input: any;
    id?: string;
  };
}

export interface ToolResultEvent {
  type: 'tool_result';
  data: {
    tool: string;
    output: string;
    is_error: boolean;
  };
}

export interface DiffEvent {
  type: 'diff';
  data: {
    path: string;
    diff: string;
  };
}

export interface ConfirmRequestEvent {
  type: 'confirm_request';
  data: {
    action: string;
    details?: string;
    tool_id?: string;
  };
}

export interface AskUserEvent {
  type: 'ask_user';
  data: {
    question: string;
  };
}

export interface WarningEvent {
  type: 'warning';
  data: {
    message: string;
  };
}

export interface ErrorEvent {
  type: 'error';
  data: {
    message: string;
  };
}

export interface DoneEvent {
  type: 'done';
  data: {
    text: string;
    id?: string;
    pending_changes?: number;
  };
}

export interface PongEvent {
  type: 'pong';
  data: {};
}

export interface CancelledEvent {
  type: 'cancelled';
  data: {
    message: string;
  };
}

// ── 流水线 / 多角色 ──

export interface RoleHeaderEvent {
  type: 'role_header';
  data: {
    label: string;
    model: string;
  };
}

export interface StageEndEvent {
  type: 'stage_end';
  data: {
    label: string;
  };
}

export type ServerEvent =
  | ReadyEvent
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
  | WarningEvent
  | ErrorEvent
  | DoneEvent
  | PongEvent
  | CancelledEvent
  | RoleHeaderEvent
  | StageEndEvent;

// ═══════════════════════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════════════════════

/** 将 JSON 字符串解析为 ServerEvent，失败返回 null */
export function parseServerEvent(raw: string): ServerEvent | null {
  try {
    const obj = JSON.parse(raw);
    if (typeof obj.type === 'string') {
      return obj as ServerEvent;
    }
    return null;
  } catch {
    return null;
  }
}

/** 判断事件是否表示一轮响应结束 */
export function isTurnEnd(event: ServerEvent): boolean {
  return event.type === 'done' || event.type === 'error';
}
