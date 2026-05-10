// Agent Server WebSocket 客户端
//
// 封装与 Agent Server（--mode server）的 WebSocket 通信，
// 提供简单的事件驱动 API。

import WebSocket from 'ws';
import { EventEmitter } from 'node:events';
import type {
  ClientMessage,
  ServerEvent,
  UserMessage,
  ConfirmResponse,
  AskUserResponse,
} from './protocol.js';
import { parseServerEvent } from './protocol.js';

export interface AgentClientOptions {
  /** Agent Server URL（如 ws://localhost:9527/agent） */
  url: string;
  /** 重连间隔（毫秒），默认 5000 */
  reconnectMs?: number;
  /** 最大重连次数，默认 10，-1 表示无限 */
  maxReconnects?: number;
}

export declare interface AgentClient {
  on(event: 'ready', listener: (event: ServerEvent & { type: 'ready' }) => void): this;
  on(event: 'event', listener: (event: ServerEvent) => void): this;
  on(event: 'token', listener: (token: string) => void): this;
  on(event: 'turn_start', listener: () => void): this;
  on(event: 'turn_end', listener: (text: string) => void): this;
  on(event: 'confirm_required', listener: (event: ServerEvent & { type: 'confirm_request' }) => void): this;
  on(event: 'file', listener: (event: ServerEvent & { type: 'file' }) => void): this;
  on(event: 'error', listener: (err: Error) => void): this;
  on(event: 'close', listener: (code: number, reason: string) => void): this;
}

export class AgentClient extends EventEmitter {
  private url: string;
  private ws: WebSocket | null = null;
  private reconnectMs: number;
  private maxReconnects: number;
  private reconnectCount = 0;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private closed = false;
  private buffer = '';
  private turnActive = false;

  constructor(opts: AgentClientOptions) {
    super();
    this.url = opts.url;
    this.reconnectMs = opts.reconnectMs ?? 5000;
    this.maxReconnects = opts.maxReconnects ?? 10;
  }

  // ═════════════════════════════════════════════════════════════════
  // 公共 API
  // ═════════════════════════════════════════════════════════════════

  /** 连接到 Agent Server */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        resolve();
        return;
      }

      this.ws = new WebSocket(this.url);

      this.ws.on('open', () => {
        this.reconnectCount = 0;
        // ready 事件在收到服务器的 ready 消息后才触发
      });

      this.ws.on('message', (raw) => {
        const event = parseServerEvent(raw.toString());
        if (!event) return;

        // 首次 ready
        if (event.type === 'ready') {
          this.emit('ready', event);
          resolve();
          return;
        }

        // 通用事件
        this.emit('event', event);

        // 细化事件
        switch (event.type) {
          case 'streaming_token':
            this.buffer += event.data.token;
            this.emit('token', event.data.token);
            if (!this.turnActive) {
              this.turnActive = true;
              this.emit('turn_start');
            }
            break;

          case 'stream_end':
          case 'assistant_text':
          case 'done':
            if (this.turnActive) {
              const text = this.buffer;
              this.buffer = '';
              this.turnActive = false;
              this.emit('turn_end', text);
            }
            break;

          case 'confirm_request':
            this.emit('confirm_required', event);
            break;

          case 'file':
            this.emit('file', event);
            break;

          case 'error':
            this.emit('error', new Error(event.data.message));
            break;
        }
      });

      this.ws.on('close', (code, reason) => {
        this.emit('close', code, reason.toString());
        this.scheduleReconnect();
      });

      this.ws.on('error', (err) => {
        this.emit('error', err);
        // close 事件会跟随 error 触发，重连在 close 中处理
      });
    });
  }

  /** 发送用户消息 */
  sendMessage(text: string, workdir?: string): void {
    this.buffer = '';
    this.send({ type: 'user_message', data: { text, workdir } });
  }

  /** 发送确认响应 */
  sendConfirm(approved: boolean, toolId?: string): void {
    this.send({ type: 'confirm_response', data: { approved, tool_id: toolId } });
  }

  /** 发送 AskUser 响应 */
  sendAskUserAnswer(answer: string): void {
    this.send({ type: 'ask_user_response', data: { answer } });
  }

  /** 关闭连接 */
  close(): void {
    this.closed = true;
    this.clearReconnect();
    this.ws?.close();
    this.ws = null;
  }

  /** 是否已连接 */
  isAlive(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  // ═════════════════════════════════════════════════════════════════
  // 内部
  // ═════════════════════════════════════════════════════════════════

  private send(msg: ClientMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  private scheduleReconnect(): void {
    if (this.closed) return;
    if (this.maxReconnects >= 0 && this.reconnectCount >= this.maxReconnects) {
      this.emit('error', new Error(`Max reconnects (${this.maxReconnects}) reached`));
      return;
    }
    this.reconnectCount++;
    this.reconnectTimer = setTimeout(() => {
      this.connect().catch(() => {
        // 重连失败，close 事件会再次触发 scheduleReconnect
      });
    }, this.reconnectMs);
  }

  private clearReconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}
