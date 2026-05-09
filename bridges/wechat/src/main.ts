#!/usr/bin/env node
// WeChat Bridge for Rust Agent
//
// 启动方式:
//   AGENT_URL=ws://localhost:9527/agent tsx src/main.ts
//
// 环境变量:
//   AGENT_URL          — Agent Server WebSocket URL（默认 ws://localhost:9527/agent）
//   CHUNK_SIZE          — 分片发送阈值（0 = 聚合模式，默认 0）
//   SESSION_TTL_MS      — 会话超时毫秒（默认 1800000 = 30分钟）
//   LOG_LEVEL           — 日志级别（默认 info）
//   STORAGE_DIR         — wechatbot 存储目录（默认 ~/.wechatbot）

import { WeChatBot } from '@wechatbot/wechatbot';
import { loadConfig, banner } from './config.js';
import { SessionPool } from './session-pool.js';
import { startGateway } from './gateway.js';

// ═══════════════════════════════════════════════════════════════════
// 日志
// ═══════════════════════════════════════════════════════════════════

const LOG_LEVELS: Record<string, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
  silent: 4,
};

function createLogger(minLevel: string) {
  const min = LOG_LEVELS[minLevel] ?? 1;
  return (level: 'info' | 'warn' | 'error', msg: string) => {
    if ((LOG_LEVELS[level] ?? 1) >= min) {
      const ts = new Date().toISOString().slice(11, 19);
      const tag = level.toUpperCase().padEnd(5);
      console.log(`[${ts}] ${tag} ${msg}`);
    }
  };
}

// ═══════════════════════════════════════════════════════════════════
// 主函数
// ═══════════════════════════════════════════════════════════════════

async function main(): Promise<void> {
  const config = loadConfig();
  const log = createLogger(config.logLevel);

  console.log(banner(config));

  // ── 1. 初始化 WeChat Bot ──────────────────────────────────────────
  const bot = new WeChatBot({
    storage: 'file',
    storageDir: config.storageDir,
    logLevel: config.logLevel === 'debug' ? 'debug' : 'info',
    loginCallbacks: {
      onQrUrl: (url: string) => {
        console.log(`\n📱 请扫描二维码登录:\n${url}\n`);
      },
      onScanned: () => {
        log('info', '二维码已扫描，等待确认...');
      },
      onExpired: () => {
        log('warn', '二维码已过期，重新生成...');
      },
    },
  });

  bot.on('login', (creds: any) => {
    log('info', `WeChat 登录成功 (account: ${creds?.accountId ?? 'unknown'})`);
  });

  bot.on('session:expired', () => {
    log('warn', 'WeChat 会话过期，需要重新扫码');
  });

  bot.on('session:restored', (creds: any) => {
    log('info', `WeChat 会话已恢复 (account: ${creds?.accountId ?? 'unknown'})`);
  });

  bot.on('error', (err: unknown) => {
    const msg = err instanceof Error ? err.message : String(err);
    log('error', `WeChat error: ${msg}`);
  });

  // ── 2. 初始化会话池 ──────────────────────────────────────────────
  const pool = new SessionPool({
    agentUrl: config.agentUrl,
    ttlMs: config.sessionTtlMs,
    cleanupIntervalMs: config.cleanupIntervalMs,
    log,
  });

  pool.startCleanup();

  // ── 3. 启动桥接 ──────────────────────────────────────────────────
  startGateway({ config, bot, pool, log });

  // ── 4. 登录并启动 ────────────────────────────────────────────────
  try {
    await bot.login();
    log('info', 'WeChat Bridge 启动中...');
    await bot.start();
  } catch (err) {
    log('error', `启动失败: ${err}`);
    process.exit(1);
  }
}

// ═══════════════════════════════════════════════════════════════════
// 信号处理
// ═══════════════════════════════════════════════════════════════════

let shuttingDown = false;

async function shutdown(signal: string) {
  if (shuttingDown) return;
  shuttingDown = true;
  console.log(`\n收到 ${signal}，正在关闭...`);

  // bot.stop() 由 @wechatbot/wechatbot 提供（如其 API 支持）
  // pool.shutdown() 关闭所有 Agent 连接

  // 简单退出：让操作系统清理所有资源
  process.exit(0);
}

process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

main();
