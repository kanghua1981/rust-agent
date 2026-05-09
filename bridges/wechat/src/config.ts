// 配置管理 — 全部通过环境变量 + 默认值

export interface BridgeConfig {
  /** Agent Server WebSocket URL（如 ws://localhost:9527/agent） */
  agentUrl: string;

  /** 分片发送的字符数阈值（聚合模式下此值为 0） */
  chunkSize: number;

  /** 会话 TTL（毫秒），超时未活跃的 WeChat 用户连接被关闭 */
  sessionTtlMs: number;

  /** 会话清理定时器间隔（毫秒） */
  cleanupIntervalMs: number;

  /** 日志级别 */
  logLevel: 'debug' | 'info' | 'warn' | 'error' | 'silent';

  /** wechatbot 存储目录（登录凭证持久化） */
  storageDir: string;
}

const defaults: BridgeConfig = {
  agentUrl: 'ws://localhost:9527/agent',
  chunkSize: 0,             // 0 = 聚合模式（等 done 后一次性发送）
  sessionTtlMs: 30 * 60_000, // 30 分钟
  cleanupIntervalMs: 30_000, // 30 秒
  logLevel: 'info',
  storageDir: '~/.wechatbot',
};

export function loadConfig(overrides?: Partial<BridgeConfig>): BridgeConfig {
  return {
    agentUrl: overrides?.agentUrl
      ?? process.env.AGENT_URL
      ?? defaults.agentUrl,
    chunkSize: overrides?.chunkSize
      ?? (process.env.CHUNK_SIZE ? parseInt(process.env.CHUNK_SIZE, 10) : defaults.chunkSize),
    sessionTtlMs: overrides?.sessionTtlMs
      ?? (process.env.SESSION_TTL_MS ? parseInt(process.env.SESSION_TTL_MS, 10) : defaults.sessionTtlMs),
    cleanupIntervalMs: overrides?.cleanupIntervalMs
      ?? (process.env.CLEANUP_INTERVAL_MS ? parseInt(process.env.CLEANUP_INTERVAL_MS, 10) : defaults.cleanupIntervalMs),
    logLevel: overrides?.logLevel
      ?? (process.env.LOG_LEVEL as BridgeConfig['logLevel'])
      ?? defaults.logLevel,
    storageDir: overrides?.storageDir
      ?? process.env.STORAGE_DIR
      ?? defaults.storageDir,
  };
}

/** 生成启动横幅 */
export function banner(cfg: BridgeConfig): string {
  const mode = cfg.chunkSize > 0
    ? `分片模式（每 ${cfg.chunkSize} 字符）`
    : '聚合模式（done 后一次性回复）';
  return [
    `🤖 WeChat Bridge for Rust Agent`,
    `   Agent: ${cfg.agentUrl}`,
    `   发送:  ${mode}`,
    `   TTL:   ${cfg.sessionTtlMs / 1000}s`,
    ``,
  ].join('\n');
}
