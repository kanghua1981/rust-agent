import React, { useRef, useMemo } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useShallow } from 'zustand/react/shallow';
import { VirtualMessageList } from './VirtualMessageList';
import type { ToolCall } from '../types/agent';
import type { DiffEntry as StoreDiffEntry } from '../stores/agentStore';

interface Props {
  slotId: string;
  onConfirm: (id: string, approved: boolean) => void;
  onAnswer: (id: string, answer: string) => void;
  onReviewPlan: (id: string, approved: boolean, feedback?: string) => void;
  onRestoreSession: () => void;
  onDismissRestore: () => void;
}

/** Get a slot by id, returning a safe fallback for missing slots.
 *  Only picks fields ChatArea actually renders — excludes currentMessage
 *  (which changes on every streaming token and is only used by InputArea). */
const emptySlot = { messages: [] as any[], toolCalls: [] as any[], diffs: [] as any[], pendingConfirmations: [] as any[], connectionStatus: 'disconnected' as const, isProcessing: false, streamingMessageId: null as string | null, thinkingMessageId: null as string | null, sessionRestoreAvailable: null as any };

const useSlot = (id: string) => useAgentStore(
  useShallow((s) => {
    const slot = s.projectSlots[id];
    if (!slot) return emptySlot;
    return {
      messages: slot.messages,
      toolCalls: slot.toolCalls,
      diffs: slot.diffs,
      pendingConfirmations: slot.pendingConfirmations,
      connectionStatus: slot.connectionStatus,
      isProcessing: slot.isProcessing,
      streamingMessageId: slot.streamingMessageId,
      thinkingMessageId: slot.thinkingMessageId,
      sessionRestoreAvailable: slot.sessionRestoreAvailable,
    };
  })
);

export const ChatArea: React.FC<Props> = ({ slotId, onConfirm, onAnswer, onReviewPlan, onRestoreSession, onDismissRestore }) => {
  const slot = useSlot(slotId);
  const messages: any[] = slot.messages ?? [];
  const toolCalls: any[] = slot.toolCalls ?? [];
  const diffs: any[] = slot.diffs ?? [];
  const connectionStatus: string = slot.connectionStatus;
  const isProcessing: boolean = slot.isProcessing;
  const streamingMessageId: string | null = slot.streamingMessageId;
  const thinkingMessageId: string | null = slot.thinkingMessageId;
  const pendingConfirmations: any[] = slot.pendingConfirmations ?? [];
  const sessionRestoreAvailable = slot.sessionRestoreAvailable;

  // ── Stable-reference cache: reuse arrays whose contents haven't changed ──
  // Without this, useMemo creates new arrays every time → React.memo on
  // MessageItem is completely defeated → all 300 messages re-render.
  const stableRef = useRef<Map<string, { toolCalls: ToolCall[]; diffs: StoreDiffEntry[] }>>(new Map());
  const stableKeys = useRef(new Map<string, string>()); // msgId → fingerprint

  // Pre-compute toolCalls and diffs per message. O(n+m) via indexing instead
  // of the original O(n×m) nested-filter that ran ~150K comparisons per update.
  const messageDataMap = useMemo(() => {
    // ── Session cleared: reset stable-reference caches ──
    // Without this, stale cache entries from a previous session survive
    // across session clears, potentially leaking data or causing mismatches.
    if (messages.length === 0) {
      stableRef.current.clear();
      stableKeys.current.clear();
      const empty = new Map<string, { toolCalls: ToolCall[]; diffs: StoreDiffEntry[] }>();
      return empty;
    }

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
      <div className="chat-empty">
        <div className="chat-empty-logo">🤖</div>
        <p className="chat-empty-title">Rust Agent Web UI</p>
        <p className="chat-empty-text">
          {connectionStatus === 'error'
            ? '连接失败，请检查服务器地址并重试。'
            : '点击右上角「连接服务器」开始使用 AI 助手。'}
        </p>
        <div className="chat-hints">
          {['编写代码', '分析项目', '执行任务', '查找文件'].map(hint => (
            <span key={hint} className="chat-hint">{hint}</span>
          ))}
        </div>
      </div>
    );
  }

  if (connectionStatus === 'connecting') {
    return (
      <div className="chat-connecting">
        <span className="spin" style={{ marginRight: 8 }}>⟳</span> 正在连接…
      </div>
    );
  }

  return (
    <>
      {sessionRestoreAvailable && messages.length === 0 && (
        <div className="restore-banner">
          <div className="info">
            <span className="icon">📋</span>
            <span className="text">
              检测到上次会话（<strong>{sessionRestoreAvailable.message_count}</strong> 条消息）
            </span>
          </div>
          <div className="actions">
            <button className="btn-secondary" onClick={onDismissRestore}>忽略</button>
            <button className="btn-primary" onClick={onRestoreSession}>恢复会话</button>
          </div>
        </div>
      )}
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
    </>
  );
};
