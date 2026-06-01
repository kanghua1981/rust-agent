import React, { useCallback, useEffect, useRef } from 'react';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';
import { useAgentStore } from '../stores/agentStore';
import { MessageItem } from './MessageItem';
import { ErrorBoundary } from './ErrorBoundary';
import { ConfirmCard } from './ConfirmCard';
import type { ToolCall } from '../types/agent';
import type { DiffEntry as StoreDiffEntry } from '../stores/agentStore';

interface Props {
  messages: ReturnType<typeof useAgentStore.getState>['messages'];
  messageDataMap: Map<string, { toolCalls: ToolCall[]; diffs: StoreDiffEntry[] }>;
  streamingMessageId: string | null;
  thinkingMessageId: string | null;
  pendingConfirmations: ReturnType<typeof useAgentStore.getState>['pendingConfirmations'];
  isProcessing: boolean;
  onConfirm: (id: string, approved: boolean) => void;
  onAnswer: (id: string, answer: string) => void;
  onReviewPlan: (id: string, approved: boolean, feedback?: string) => void;
}

export const VirtualMessageList: React.FC<Props> = ({
  messages,
  messageDataMap,
  streamingMessageId,
  thinkingMessageId,
  pendingConfirmations,
  isProcessing,
  onConfirm,
  onAnswer,
  onReviewPlan,
}) => {
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const userScrolledUpRef = useRef(false);
  const prevMsgCountRef = useRef(messages.length);
  const prevProcessingRef = useRef(isProcessing);

  // Reset scroll-lock when user manually scrolls to the very bottom
  const handleAtBottomStateChange = useCallback((atBottom: boolean) => {
    if (atBottom) {
      userScrolledUpRef.current = false;
    }
  }, []);

  // Force-scroll to bottom when the user sends a new message
  // or when a new message is appended (user msg / assistant msg)
  // or when processing finishes (done/error/cancelled)
  useEffect(() => {
    const msgCount = messages.length;
    const processingJustStarted = isProcessing && !prevProcessingRef.current;
    const processingJustEnded = prevProcessingRef.current && !isProcessing;
    const newMsgAdded = msgCount > prevMsgCountRef.current;

    if (processingJustStarted) {
      // User sent a new message — always scroll to bottom and reset scroll lock
      userScrolledUpRef.current = false;
      virtuosoRef.current?.scrollToIndex({
        index: msgCount - 1,
        behavior: 'smooth',
        align: 'end',
      });
    } else if (processingJustEnded) {
      // Conversation finished — scroll to bottom so user sees final messages
      userScrolledUpRef.current = false;
      if (msgCount > 0) {
        virtuosoRef.current?.scrollToIndex({
          index: msgCount - 1,
          behavior: 'auto',
          align: 'end',
        });
      }
    } else if (newMsgAdded && !userScrolledUpRef.current) {
      // New message appended while user hasn't scrolled away — ensure visible
      virtuosoRef.current?.scrollToIndex({
        index: msgCount - 1,
        behavior: 'smooth',
        align: 'end',
      });
    }

    prevMsgCountRef.current = msgCount;
    prevProcessingRef.current = isProcessing;
  }, [messages.length, isProcessing]);

  const renderItem = useCallback(
    (_index: number, msg: (typeof messages)[number]) => {
      const data = messageDataMap.get(msg.id);
      return (
        <div style={{ maxWidth: '800px', margin: '0 auto', width: '100%' }}>
          <ErrorBoundary>
            <MessageItem
              message={msg}
              isStreaming={streamingMessageId === msg.id}
              isThinking={thinkingMessageId === msg.id}
              toolCalls={data?.toolCalls ?? []}
              diffs={data?.diffs ?? []}
            />
          </ErrorBoundary>
        </div>
      );
    },
    [messageDataMap, streamingMessageId, thinkingMessageId],
  );

  const Footer = useCallback(() => {
    return (
      <>
        {/* Inline confirmations */}
        {pendingConfirmations.length > 0 && (
          <div style={{ maxWidth: '680px', margin: '8px auto 0', padding: '0 24px' }}>
            {pendingConfirmations.map(c => (
              <ConfirmCard
                key={c.id}
                confirmation={c}
                onConfirm={onConfirm}
                onAnswer={(id, answer) => { onAnswer(id, answer); }}
                onReviewPlan={onReviewPlan}
              />
            ))}
          </div>
        )}

        {/* Processing indicator */}
        {isProcessing && pendingConfirmations.length === 0 && !streamingMessageId && (
          <div style={{ display: 'flex', gap: '10px', alignItems: 'center', padding: '6px 24px', justifyContent: 'center' }}>
            <div style={{
              width: '30px', height: '30px', borderRadius: '50%',
              background: 'linear-gradient(135deg, #0ea5e9, #6366f1)',
              display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '14px',
            }}>🤖</div>
            <div style={{ display: 'flex', gap: '4px', alignItems: 'center' }}>
              {[0, 1, 2].map(i => (
                <div key={i} style={{
                  width: '6px', height: '6px', borderRadius: '50%',
                  background: 'var(--accent)',
                  animation: `blink 1.2s ${i * 0.2}s infinite`,
                }} />
              ))}
            </div>
          </div>
        )}
      </>
    );
  }, [pendingConfirmations, isProcessing, streamingMessageId, onConfirm, onAnswer, onReviewPlan]);

  const EmptyPlaceholder = useCallback(() => {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center',
        color: 'var(--text3)', gap: '10px', padding: '40px',
      }}>
        <span style={{ fontSize: '36px' }}>💬</span>
        <p style={{ fontSize: '14px', color: 'var(--text2)' }}>发送消息开始对话</p>
      </div>
    );
  }, []);

  return (
    <Virtuoso
      ref={virtuosoRef}
      style={{ flex: 1 }}
      data={messages}
      itemContent={renderItem}
      followOutput={'smooth'}
      atBottomThreshold={200}
      atBottomStateChange={handleAtBottomStateChange}
      initialTopMostItemIndex={messages.length > 0 ? messages.length - 1 : undefined}
      components={{
        Footer,
        EmptyPlaceholder,
      }}
    />
  );
};
