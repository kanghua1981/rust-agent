import React, { useEffect, useRef, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { MessageItem } from './MessageItem';
import { ConfirmCard } from './ConfirmCard';

interface Props {
  onConfirm: (id: string, approved: boolean) => void;
  onAnswer: (id: string, answer: string) => void;
  onReviewPlan: (id: string, approved: boolean, feedback?: string) => void;
}

export const ChatArea: React.FC<Props> = ({ onConfirm, onAnswer, onReviewPlan }) => {
  const { messages, pendingConfirmations, streamingMessageId, connectionStatus, isProcessing } = useAgentStore();
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  // Track whether the user is near the bottom. Start true so initial messages auto-scroll.
  const isNearBottomRef = useRef(true);

  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    // "Near bottom" = within 200px of the bottom edge
    isNearBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 200;
  }, []);

  // Auto-scroll only when user is already near the bottom.
  // This way reading history is never interrupted by new messages.
  const msgCount = messages.length;
  const confirmCount = pendingConfirmations.length;
  useEffect(() => {
    if (isNearBottomRef.current) {
      bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [msgCount, confirmCount]);

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
    <div
      ref={scrollContainerRef}
      onScroll={handleScroll}
      style={{
        flex: 1, overflowY: 'auto', padding: '20px 24px',
        display: 'flex', flexDirection: 'column',
      }}
    >
      {messages.length === 0 ? (
        <div style={{
          flex: 1, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center',
          color: 'var(--text3)', gap: '10px',
        }}>
          <span style={{ fontSize: '36px' }}>💬</span>
          <p style={{ fontSize: '14px', color: 'var(--text2)' }}>发送消息开始对话</p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '2px', maxWidth: '800px', width: '100%', margin: '0 auto' }}>
          {messages.map(msg => (
            <MessageItem
              key={msg.id}
              message={msg}
              isStreaming={streamingMessageId === msg.id}
            />
          ))}

          {/* Inline confirmations */}
          {pendingConfirmations.length > 0 && (
            <div style={{ maxWidth: '680px', margin: '8px 40px 0' }}>
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
            <div style={{ display: 'flex', gap: '10px', alignItems: 'center', padding: '6px 0', marginLeft: '40px' }}>
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
        </div>
      )}
      <div ref={bottomRef} />
    </div>
  );
};
