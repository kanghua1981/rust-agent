// 核心桥接逻辑
//
// 胶水代码：微信消息 ↔ Agent Server 事件

import type { WeChatBot } from '@wechatbot/wechatbot';
import type { AgentClient } from './agent-client.js';
import type { BridgeConfig } from './config.js';
import { SessionPool } from './session-pool.js';

export interface GatewayDeps {
  config: BridgeConfig;
  bot: WeChatBot;
  pool: SessionPool;
  log: (level: 'info' | 'warn' | 'error', msg: string) => void;
}

/**
 * 启动桥接：注册微信消息处理器，开始转发。
 */
export function startGateway(deps: GatewayDeps): void {
  const { config, bot, pool, log } = deps;
  const isChunked = config.chunkSize > 0;

  bot.onMessage(async (msg) => {
    // 忽略非文本消息（图片/语音/文件等暂不处理）
    if (!msg.text) return;

    log('info', `[wx] ${msg.userId.slice(0, 8)}... "${msg.text.slice(0, 50)}${msg.text.length > 50 ? '...' : ''}"`);

    let client: AgentClient;
    try {
      client = await pool.get(msg.userId);
    } catch (err) {
      log('error', `[wx] failed to get agent session for ${msg.userId.slice(0, 8)}...: ${err}`);
      await bot.reply(msg, '⚠️ 暂时无法连接到 AI 服务，请稍后再试。').catch(() => {});
      return;
    }

    // 绑定本轮响应处理（先清理旧的监听器）
    setupTurnHandlers({ client, bot, msg, config, log, isChunked });

    // 发送用户消息到 Agent
    client.sendMessage(msg.text);
  });

  log('info', '[gateway] message handler registered');
}

// ═══════════════════════════════════════════════════════════════════
// 单轮响应处理
// ═══════════════════════════════════════════════════════════════════

interface TurnContext {
  client: AgentClient;
  bot: WeChatBot;
  msg: any; // WeChat message object
  config: BridgeConfig;
  log: (level: 'info' | 'warn' | 'error', msg: string) => void;
  isChunked: boolean;
}

function setupTurnHandlers(ctx: TurnContext): void {
  const { client, bot, msg, config, log, isChunked } = ctx;

  // 移除旧的监听器（简单的"一次性"模式：每次设置前先清理）
  client.removeAllListeners('token');
  client.removeAllListeners('turn_start');
  client.removeAllListeners('turn_end');
  client.removeAllListeners('confirm_required');

  // ── 聚合模式状态 ──
  let buffer = '';

  // ── 分片模式状态 ──
  let chunkBuffer = '';
  let typingSent = false;

  // token 事件
  client.on('token', (token: string) => {
    if (isChunked) {
      chunkBuffer += token;
      // 显示"正在输入"
      if (!typingSent) {
        bot.sendTyping(msg.userId).catch(() => {});
        typingSent = true;
      }
      // 达到阈值时分片发送
      if (chunkBuffer.length >= config.chunkSize) {
        bot.send(msg.userId, chunkBuffer).catch(() => {});
        chunkBuffer = '';
      }
    } else {
      buffer += token;
    }
  });

  // 一轮开始
  client.on('turn_start', () => {
    buffer = '';
    chunkBuffer = '';
  });

  // 一轮结束
  client.on('turn_end', (fullText: string) => {
    const finalText = isChunked
      ? chunkBuffer  // 分片模式：发送剩余片段
      : fullText;    // 聚合模式：发送完整回复

    if (finalText && finalText.trim()) {
      if (isChunked) {
        bot.send(msg.userId, finalText).catch(() => {});
        // 停止 typing 指示
        bot.stopTyping(msg.userId).catch(() => {});
      } else {
        // reply() 自动清除 typing 指示
        bot.reply(msg, finalText).catch(() => {});
      }
      log('info', `[wx] replied to ${msg.userId.slice(0, 8)}... (${finalText.length} chars)`);
    } else {
      bot.reply(msg, '🤔（Agent 没有返回内容）').catch(() => {});
    }

    // 更新会话活跃时间
    // pool.touch() 在外部通过闭包较麻烦，这里通过 client 的事件处理即可
  });

  // 确认请求 — 微信场景下默认批准（可通过配置改为向用户询问）
  client.on('confirm_required', (event: any) => {
    const action = event.data?.action ?? '未知操作';
    log('warn', `[wx] confirm requested: "${action}" — auto-approving`);
    // 自动批准所有工具调用（微信场景下用户无法实时确认）
    // 如需更安全的策略，可改为拒绝或向用户发送询问消息
    client.sendConfirm(true, event.data?.tool_id);
  });
}
