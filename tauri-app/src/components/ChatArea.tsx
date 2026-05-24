import React, { useRef, useMemo } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useShallow } from 'zustand/react/shallow';
import { VirtualMessageList } from './VirtualMessageList';
import type { ToolCall } from '../types/agent';
import type { DiffEntry as StoreDiffEntry } from '../stores/agentStore';

interface Props {
  onConfirm: (id: string, approved: boolean) => void;
  onAnswer: (id: string, answer: string) => void;
  onReviewPlan: (id: string, approved: boolean, feedback?: string) => void;
}

export const ChatArea: React.FC<Props> = ({ onConfirm, onAnswer, onReviewPlan }) => {
  // 高频变化 — 独立订阅（驱 messageDataMap 的 useMemo 重算）
  const messages = useAgentStore(s => s.messages);
  const toolCalls = useAgentStore(s => s.toolCalls);
  const diffs = useAgentStore(s => s.diffs);

  // 低频变化 — 合并为一次 shallow 比较订阅
  const { streamingMessageId, connectionStatus, isProcessing, thinkingMessageId, pendingConfirmations } = useAgentStore(
    useShallow(s => ({
      streamingMessageId: s.streamingMessageId,
      connectionStatus: s.connectionStatus,
      isProcessing: s.isProcessing,
      thinkingMessageId: s.thinkingMessageId,
      pendingConfirmations: s.pendingConfirmations,
    })),
  );

  // ── Stable-reference cache: reuse arrays whose contents haven't changed ──
  // Without this, useMemo creates new arrays every time → React.memo on
  // MessageItem is completely defeated → all 300 messages re-render.
  const stableRef = useRef<Map<string, { toolCalls: ToolCall[]; diffs: StoreDiffEntry[] }>>(new Map());
  const stableKeys = useRef(new Map<string, string>()); // msgId → fingerprint

  // Pre-compute toolCalls and diffs per message. O(n+m) via indexing instead
  // of the original O(n×m) nested-filter that ran ~150K comparisons per update.
  const messageDataMap = useMemo(() => {
    // ── Phase 1: index toolCalls by messageId (O(toolCalls.length)) ──
    const tcByMsgId = new Map<string, ToolCall[]>();
    const unmatchedTCs: ToolCall[] = [];
    for (const tc of toolCalls) {
      if (tc.messageId) {
        const arr = tcByMsgId.get(tc.messageId);
        if (arr) arr.push(tc);
        else tcByMsgId.set(tc.messageId, [tc]);
      } else {
        unmatchedTCs.push(tc);
      }
    }

    // ── Phase 1b: index diffs by minute-bucket (O(diffs.length)) ──
    const diffsByMinute = new Map<number, StoreDiffEntry[]>();
    for (const d of diffs) {
      const bucket = Math.floor(d.timestamp / 60000);
      const arr = diffsByMinute.get(bucket);
      if (arr) arr.push(d);
      else diffsByMinute.set(bucket, [d]);
    }

    const prev = stableRef.current;
    const prevKeys = stableKeys.current;
    const next = new Map<string, { toolCalls: ToolCall[]; diffs: StoreDiffEntry[] }>();
    const nextKeys = new Map<string, string>();

    // ── Phase 2: assign to each message (O(messages.length)) ──
    for (const msg of messages) {
      if (msg.role === 'user') continue;

      // Direct-by-messageId hit (vast majority of cases)
      let relatedTCs = tcByMsgId.get(msg.id);

      // Fallback: unmatched (no messageId) toolCalls within 5s window
      if (unmatchedTCs.length > 0) {
        const nearby = unmatchedTCs.filter(
          tc => Math.abs(tc.timestamp - msg.timestamp) < 5000,
        );
        if (nearby.length > 0) {
          relatedTCs = relatedTCs
            ? [...relatedTCs, ...nearby]
            : nearby;
        }
      }
      if (!relatedTCs) relatedTCs = [];

      // Diffs: check current + adjacent minute-buckets
      const msgMinute = Math.floor(msg.timestamp / 60000);
      let relatedDiffs: StoreDiffEntry[] = [];
      for (let b = msgMinute - 1; b <= msgMinute + 1; b++) {
        const bucket = diffsByMinute.get(b);
        if (bucket) {
          for (const d of bucket) {
            if (Math.abs(d.timestamp - msg.timestamp) < 60000) {
              relatedDiffs.push(d);
            }
          }
        }
      }

      // ── Stable-reference optimisation ──────────────────────────────
      // If the data for this message hasn't changed, reuse the previous
      // array reference so React.memo on MessageItem actually works.
      const fp = `${relatedTCs.map(tc => tc.id).join(',')}|${relatedDiffs.map(d => d.id).join(',')}`;
      nextKeys.set(msg.id, fp);

      if (prev.has(msg.id) && prevKeys.get(msg.id) === fp) {
        next.set(msg.id, prev.get(msg.id)!);
      } else {
        next.set(msg.id, { toolCalls: relatedTCs, diffs: relatedDiffs });
      }
    }

    stableRef.current = next;
    stableKeys.current = nextKeys;
    return next;
  }, [messages, toolCalls, diffs]);

  if (connectionStatus === 'disconnected' || connectionStatus === 'error') {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center',
        color: 'var(--text3)', gap: '12px', padding: '40px',
      }}>
        <div style={{
          width: '60px', height: '60px', borderRadius: '50%',
          background: 'var(--bg3)', display: 'flex', alignItems: 'center',
          justifyContent: 'center', fontSize: '28px',
        }}>
          🤖
        </div>
        <p style={{ fontSize: '16px', fontWeight: '500', color: 'var(--text2)' }}>Rust Agent Web UI</p>
        <p style={{ fontSize: '13px', textAlign: 'center', maxWidth: '320px', lineHeight: '1.6' }}>
          {connectionStatus === 'error'
            ? '连接失败，请检查服务器地址并重试。'
            : '点击右上角「连接服务器」开始使用 AI 助手。'}
        </p>
        <div style={{ marginTop: '8px', display: 'flex', gap: '8px', flexWrap: 'wrap', justifyContent: 'center' }}>
          {['编写代码', '分析项目', '执行任务', '查找文件'].map(hint => (
            <span key={hint} style={{
              padding: '4px 12px', background: 'var(--bg3)',
              border: '1px solid var(--border)', borderRadius: '16px',
              fontSize: '12px', color: 'var(--text2)',
            }}>{hint}</span>
          ))}
        </div>
      </div>
    );
  }

  if (connectionStatus === 'connecting') {
    return (
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text2)' }}>
        <span className="spin" style={{ marginRight: '8px' }}>⟳</span> 正在连接…
      </div>
    );
  }

  return (
    <VirtualMessageList
      messages={messages}
      messageDataMap={messageDataMap}
      streamingMessageId={streamingMessageId}
      thinkingMessageId={thinkingMessageId}
      pendingConfirmations={pendingConfirmations}
      isProcessing={isProcessing}
      onConfirm={onConfirm}
      onAnswer={onAnswer}
      onReviewPlan={onReviewPlan}
    />
  );
};
