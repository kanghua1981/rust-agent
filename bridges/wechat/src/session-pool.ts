// 会话连接池
//
// 维护 WeChat 用户 ID → AgentClient 的映射。
// 定期清理超时的连接，释放 Agent Server 侧的 worker 进程。

import { AgentClient } from './agent-client.js';

export interface SessionPoolOptions {
  /** Agent Server URL */
  agentUrl: string;
  /** 会话 TTL（毫秒） */
  ttlMs: number;
  /** 清理间隔（毫秒） */
  cleanupIntervalMs: number;
  /** 日志函数 */
  log?: (level: 'info' | 'warn' | 'error', msg: string) => void;
}

interface SessionEntry {
  client: AgentClient;
  lastActive: number;
}

export class SessionPool {
  private sessions = new Map<string, SessionEntry>();
  private agentUrl: string;
  private ttlMs: number;
  private cleanupTimer: NodeJS.Timeout | null = null;
  private log: (level: 'info' | 'warn' | 'error', msg: string) => void;

  constructor(opts: SessionPoolOptions) {
    this.agentUrl = opts.agentUrl;
    this.ttlMs = opts.ttlMs;
    this.log = opts.log ?? (() => {});
  }

  // ═════════════════════════════════════════════════════════════════
  // 公共 API
  // ═════════════════════════════════════════════════════════════════

  /**
   * 获取或创建指定 WeChat 用户的 Agent 连接。
   * 如果已有存活连接则复用，否则新建。
   */
  async get(userId: string): Promise<AgentClient> {
    const existing = this.sessions.get(userId);
    if (existing?.client.isAlive()) {
      existing.lastActive = Date.now();
      return existing.client;
    }

    // 清理死连接
    if (existing) {
      this.sessions.delete(userId);
    }

    // 新建连接
    const client = new AgentClient({ url: this.agentUrl });
    await client.connect();

    this.sessions.set(userId, {
      client,
      lastActive: Date.now(),
    });

    this.log('info', `[pool] new session for user ${userId.slice(0, 8)}... (total: ${this.sessions.size})`);
    return client;
  }

  /** 更新指定用户的最后活跃时间 */
  touch(userId: string): void {
    const entry = this.sessions.get(userId);
    if (entry) {
      entry.lastActive = Date.now();
    }
  }

  /** 启动定期清理 */
  startCleanup(): void {
    this.cleanupTimer = setInterval(() => this.cleanup(), this.ttlMs / 2);
  }

  /** 停止清理定时器并关闭所有连接 */
  async shutdown(): Promise<void> {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
    const promises: Promise<void>[] = [];
    for (const [userId, entry] of this.sessions) {
      this.log('info', `[pool] closing session for user ${userId.slice(0, 8)}...`);
      promises.push(new Promise((resolve) => {
        entry.client.on('close', () => resolve());
        entry.client.close();
        // 兜底
        setTimeout(resolve, 2000);
      }));
    }
    this.sessions.clear();
    await Promise.all(promises);
  }

  /** 获取当前活跃会话数 */
  get size(): number {
    return this.sessions.size;
  }

  // ═════════════════════════════════════════════════════════════════
  // 内部
  // ═════════════════════════════════════════════════════════════════

  private cleanup(): void {
    const now = Date.now();
    const expired: string[] = [];

    for (const [userId, entry] of this.sessions) {
      if (now - entry.lastActive > this.ttlMs) {
        expired.push(userId);
      }
    }

    for (const userId of expired) {
      const entry = this.sessions.get(userId);
      if (entry) {
        this.log('info', `[pool] expiring session for user ${userId.slice(0, 8)}... (idle ${Math.round((now - entry.lastActive) / 1000)}s)`);
        entry.client.close();
        this.sessions.delete(userId);
      }
    }

    if (expired.length > 0) {
      this.log('info', `[pool] cleaned ${expired.length} expired sessions (remaining: ${this.sessions.size})`);
    }
  }
}
