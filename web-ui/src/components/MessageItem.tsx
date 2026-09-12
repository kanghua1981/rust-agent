import React, { useState, useMemo, useRef, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Message, ToolCall } from '../types/agent';
import { ToolCallCard } from './ToolCallCard';
import { DiffViewer } from './DiffViewer';
import type { DiffEntry } from '../stores/agentStore';

// Module-level constant to avoid re-creating array on every render
const markdownPlugins = [remarkGfm];

interface Props {
  message: Message;
  isStreaming: boolean;
  isThinking: boolean;
  toolCalls: ToolCall[];
  diffs: DiffEntry[];
}

const UserAvatar = () => <div className="avatar user">U</div>;
const AgentAvatar = () => <div className="avatar agent">🤖</div>;

const formatTime = (ts: number) =>
  new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

/** Backend role labels carry a leading emoji: "🤖 Agent" → "Agent". */
const roleName = (label: string) => label.replace(/^[^\p{L}\p{N}]+/u, '').trim();

/** The main loop reports itself as "Agent"; other roles are worth calling out. */
const isNoteworthyRole = (label?: string) =>
  !!label && roleName(label).toLowerCase() !== 'agent';

export const MessageItem = React.memo<Props>(({ message, isStreaming, isThinking, toolCalls, diffs }) => {
  const isUser = message.role === 'user';
  const isSystem = message.role === 'system';

  const [copyStatus, setCopyStatus] = useState<'idle' | 'success' | 'error'>('idle');
  const mountedRef = useRef(true);
  useEffect(() => () => { mountedRef.current = false; }, []);

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      if (!mountedRef.current) return;
      setCopyStatus('success');
      setTimeout(() => { if (mountedRef.current) setCopyStatus('idle'); }, 2000);
      return true;
    } catch (err) {
      console.error('复制失败:', err);
      if (!mountedRef.current) return;
      setCopyStatus('error');
      setTimeout(() => { if (mountedRef.current) setCopyStatus('idle'); }, 2000);
      return false;
    }
  };

  if (isSystem) {
    return (
      <div className="fade-in sys-note-wrap">
        <span className="sys-note">{message.content}</span>
      </div>
    );
  }

  const relatedToolCalls = isUser ? [] : toolCalls;
  const relatedDiffs = isUser ? [] : diffs;

  // Thinking display: collapsible, defaults to expanded while streaming.
  const [thinkingExpanded, setThinkingExpanded] = useState(false);
  const isCurrentlyThinking = isThinking;
  const hasThinking = !!message.thinking;
  const showThinking = hasThinking || isCurrentlyThinking;

  // Cache ReactMarkdown output — only re-parse when content changes.
  const markdownContent = useMemo(
    () => message.content ? <ReactMarkdown remarkPlugins={markdownPlugins}>{message.content}</ReactMarkdown> : null,
    [message.content],
  );

  return (
    <div className={`fade-in msg${isUser ? ' user' : ''}`}>
      {isUser ? <UserAvatar /> : <AgentAvatar />}

      <div className="msg-body">
        {/* Name + time + which role/model produced this reply */}
        <div className={`msg-head${isUser ? ' user' : ''}`}>
          <span className={`msg-name${isUser ? ' user' : ''}`}>{isUser ? '你' : 'Assistant'}</span>
          {!isUser && isNoteworthyRole(message.meta?.stageLabel) && (
            <span className="msg-role">{message.meta!.stageLabel}</span>
          )}
          {!isUser && message.meta?.stageModel && (
            <span className="msg-model" title={message.meta.stageModel}>
              🧠 {message.meta.stageModel.split('/').pop()}
            </span>
          )}
          <span className="msg-time">{formatTime(message.timestamp)}</span>
        </div>

        {/* Thinking block (collapsible) */}
        {showThinking && !isUser && (
          <div style={{ marginBottom: 6 }}>
            <div
              className={`thinking-toggle${isCurrentlyThinking ? ' active' : ''}`}
              onClick={() => setThinkingExpanded(!thinkingExpanded)}
            >
              <span>{thinkingExpanded || isCurrentlyThinking ? '▼' : '▶'}</span>
              <span>💭 {isCurrentlyThinking ? 'Thinking…' : 'Thinking'}</span>
            </div>
            {(thinkingExpanded || isCurrentlyThinking) && message.thinking && (
              <div className="thinking-body">
                {message.thinking}
                {isCurrentlyThinking && <span className="cursor" />}
              </div>
            )}
          </div>
        )}

        {/* Message bubble */}
        {(message.content || isStreaming) && (
          <div className={`bubble ${isUser ? 'user' : 'agent'}`}>
            {isUser ? (
              <span className="bubble-text">{message.content}</span>
            ) : (
              <div className="md-content">
                {markdownContent}
                {isStreaming && <span className="cursor" />}
              </div>
            )}

            <button className="copy-btn" onClick={() => copyToClipboard(message.content)} title="复制消息">
              {copyStatus === 'success' ? '✓' : copyStatus === 'error' ? '✗' : '📋'}
            </button>

            {copyStatus !== 'idle' && (
              <div className={`copy-toast ${copyStatus === 'success' ? 'ok' : 'err'}`}>
                {copyStatus === 'success' ? '已复制' : '复制失败'}
              </div>
            )}
          </div>
        )}

        {/* Tool calls inline */}
        {relatedToolCalls.length > 0 && (
          <div className="msg-extra">
            {relatedToolCalls.map(tc => (
              <ToolCallCard key={tc.id} toolCall={tc} />
            ))}
          </div>
        )}

        {/* Diffs inline */}
        {relatedDiffs.length > 0 && (
          <div className="msg-extra">
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
