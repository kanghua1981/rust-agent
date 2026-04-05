import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { Message } from '../types/agent';
import { useAgentStore } from '../stores/agentStore';
import { ToolCallCard } from './ToolCallCard';
import { DiffViewer } from './DiffViewer';

interface Props {
  message: Message;
  isStreaming: boolean;
}

const UserAvatar = () => (
  <div style={{
    width: '30px', height: '30px', borderRadius: '50%',
    background: 'linear-gradient(135deg, #6366f1, #8b5cf6)',
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    fontSize: '13px', flexShrink: 0, color: '#fff', fontWeight: '700',
  }}>U</div>
);

const AgentAvatar = () => (
  <div style={{
    width: '30px', height: '30px', borderRadius: '50%',
    background: 'linear-gradient(135deg, #0ea5e9, #6366f1)',
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    fontSize: '14px', flexShrink: 0,
  }}>🤖</div>
);

const formatTime = (ts: number) =>
  new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

export const MessageItem = React.memo<Props>(({ message, isStreaming }) => {
  // Use precise selectors so this component only re-renders when toolCalls/diffs
  // actually change, NOT on every streaming token.
  const toolCalls = useAgentStore(state => state.toolCalls);
  const diffs = useAgentStore(state => state.diffs);
  const isUser = message.role === 'user';
  const isSystem = message.role === 'system';
  
  // 复制功能状态
  const [copyStatus, setCopyStatus] = useState<'idle' | 'success' | 'error'>('idle');

  // 复制消息内容到剪贴板
  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopyStatus('success');
      setTimeout(() => setCopyStatus('idle'), 2000); // 2秒后重置状态
      return true;
    } catch (err) {
      console.error('复制失败:', err);
      setCopyStatus('error');
      setTimeout(() => setCopyStatus('idle'), 2000);
      return false;
    }
  };

  if (isSystem) {
    // Pipeline stage header
    if (message.meta?.stageLabel) {
      const stageIcons: Record<string, string> = {
        Planner: '🎯', Executor: '⚡', Checker: '✅', Router: '🔀',
      };
      const icon = stageIcons[message.meta.stageLabel] ?? '🔵';
      return (
        <div className="fade-in" style={{
          display: 'flex', alignItems: 'center', gap: '8px',
          padding: '10px 0 4px',
        }}>
          <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
          <span style={{
            fontSize: '11px', fontWeight: '600',
            color: 'var(--accent)', letterSpacing: '0.06em',
            background: 'rgba(99,102,241,0.1)',
            border: '1px solid rgba(99,102,241,0.3)',
            borderRadius: '12px', padding: '2px 10px',
            display: 'flex', alignItems: 'center', gap: '5px',
          }}>
            <span>{icon}</span>
            <span>{message.meta.stageLabel}</span>
            {message.meta.stageModel && (
              <span style={{ color: 'var(--text3)', fontWeight: '400', fontSize: '10px' }}>
                • {message.meta.stageModel.split('/').pop()}
              </span>
            )}
          </span>
          <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
        </div>
      );
    }

    return (
      <div className="fade-in" style={{
        display: 'flex', justifyContent: 'center', padding: '4px 0',
      }}>
        <span style={{
          fontSize: '12px', color: 'var(--text3)',
          background: 'var(--bg3)', border: '1px solid var(--border)',
          borderRadius: '20px', padding: '3px 12px',
        }}>
          {message.content}
        </span>
      </div>
    );
  }

  // For assistant messages, find related tool calls owned by this message
  const relatedToolCalls = isUser ? [] : toolCalls.filter(
    tc => tc.messageId ? tc.messageId === message.id : Math.abs(tc.timestamp - message.timestamp) < 5000
  );
  // Find diffs in same time window
  const relatedDiffs = isUser ? [] : diffs.filter(
    d => Math.abs(d.timestamp - message.timestamp) < 60000
  );

  return (
    <div
      className="fade-in"
      style={{
        display: 'flex',
        gap: '10px',
        padding: '6px 0',
        flexDirection: isUser ? 'row-reverse' : 'row',
        alignItems: 'flex-start',
      }}
    >
      {isUser ? <UserAvatar /> : <AgentAvatar />}

      <div style={{
        flex: 1,
        maxWidth: isUser ? '75%' : '100%',
        minWidth: 0,
      }}>
        {/* Name + time */}
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: '6px',
          marginBottom: '4px',
          flexDirection: isUser ? 'row-reverse' : 'row',
        }}>
          <span style={{ fontSize: '12px', fontWeight: '600', color: isUser ? 'var(--accent)' : 'var(--text2)' }}>
            {isUser ? '你' : 'Assistant'}
          </span>
          <span style={{ fontSize: '11px', color: 'var(--text3)' }}>{formatTime(message.timestamp)}</span>
        </div>

        {/* Message bubble */}
        {(message.content || isStreaming) && (
          <div style={{
            background: isUser ? 'linear-gradient(135deg, var(--accent), #8b5cf6)' : 'var(--surface)',
            border: isUser ? 'none' : '1px solid var(--border)',
            borderRadius: isUser ? '14px 14px 4px 14px' : '4px 14px 14px 14px',
            padding: '10px 14px',
            color: isUser ? '#fff' : 'var(--text)',
            wordBreak: 'break-word',
            display: 'inline-block',
            maxWidth: '100%',
            position: 'relative',
          }}>
            {isUser ? (
              <span style={{ whiteSpace: 'pre-wrap', fontSize: '14px' }}>{message.content}</span>
            ) : (
              <div className="md-content" style={{ fontSize: '14px' }}>
                {isStreaming ? (
                  // Plain text during streaming — avoids re-parsing full Markdown
                  // on every token which would saturate the JS thread.
                  <span style={{ whiteSpace: 'pre-wrap' }}>{message.content}</span>
                ) : (
                  message.content ? <ReactMarkdown>{message.content}</ReactMarkdown> : null
                )}
                {isStreaming && <span className="cursor" />}
              </div>
            )}
            
            {/* 复制按钮 - 右下角 */}
            <button
              onClick={() => copyToClipboard(message.content)}
              style={{
                position: 'absolute',
                bottom: '6px',
                right: '6px',
                background: isUser ? 'rgba(255,255,255,0.2)' : 'rgba(0,0,0,0.05)',
                border: 'none',
                borderRadius: '4px',
                width: '24px',
                height: '24px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: '12px',
                color: isUser ? 'rgba(255,255,255,0.8)' : 'var(--text3)',
                cursor: 'pointer',
                opacity: 0.6,
                transition: 'opacity 0.2s',
              }}
              onMouseOver={(e) => e.currentTarget.style.opacity = '1'}
              onMouseOut={(e) => e.currentTarget.style.opacity = '0.6'}
              title="复制消息"
            >
              {copyStatus === 'success' ? '✓' : copyStatus === 'error' ? '✗' : '📋'}
            </button>
            
            {/* 复制状态提示 */}
            {copyStatus !== 'idle' && (
              <div style={{
                position: 'absolute',
                bottom: '-24px',
                right: '0',
                fontSize: '11px',
                padding: '2px 6px',
                background: copyStatus === 'success' ? 'rgba(16,185,129,0.9)' : 'rgba(239,68,68,0.9)',
                color: '#fff',
                borderRadius: '4px',
                whiteSpace: 'nowrap',
              }}>
                {copyStatus === 'success' ? '已复制' : '复制失败'}
              </div>
            )}
          </div>
        )}

        {/* Tool calls inline */}
        {relatedToolCalls.length > 0 && (
          <div style={{ marginTop: '8px' }}>
            {relatedToolCalls.map(tc => (
              <ToolCallCard key={tc.id} toolCall={tc} />
            ))}
          </div>
        )}

        {/* Diffs inline */}
        {relatedDiffs.length > 0 && (
          <div style={{ marginTop: '8px' }}>
            {relatedDiffs.map(d => (
              <DiffViewer key={d.id} path={d.path} diff={d.diff} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
});

MessageItem.displayName = 'MessageItem';
