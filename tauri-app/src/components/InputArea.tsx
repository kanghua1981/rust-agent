import React, { useRef, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';

interface Props {
  onSend: (text: string) => void;
}

export const InputArea: React.FC<Props> = ({ onSend }) => {
  const { connectionStatus, isProcessing, currentMessage, setCurrentMessage } = useAgentStore();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const disabled = connectionStatus !== 'connected' || isProcessing;

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 200) + 'px';
  }, [currentMessage]);

  const handleSend = () => {
    const text = currentMessage.trim();
    if (!text || disabled) return;
    onSend(text);
    setCurrentMessage('');
    // Reset height
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  };

  const placeholder = !['connected'].includes(connectionStatus)
    ? '请先连接服务器…'
    : isProcessing
    ? 'Agent 正在处理中…'
    : '发消息给 Agent（Enter 发送，Shift+Enter 换行）';

  return (
    <div style={{
      borderTop: '1px solid var(--border)',
      background: 'var(--bg2)',
      padding: '12px 20px 14px',
      flexShrink: 0,
    }}>
      <div style={{
        maxWidth: '800px',
        margin: '0 auto',
        display: 'flex',
        gap: '10px',
        alignItems: 'flex-end',
      }}>
        <div style={{
          flex: 1,
          display: 'flex',
          background: disabled ? 'var(--bg3)' : 'var(--surface)',
          border: `1px solid ${disabled ? 'var(--border)' : 'var(--border2)'}`,
          borderRadius: '12px',
          transition: 'border-color 0.15s, box-shadow 0.15s',
          overflow: 'hidden',
        }}
          onFocus={() => {}}
        >
          <textarea
            ref={textareaRef}
            value={currentMessage}
            onChange={(e) => setCurrentMessage(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
            placeholder={placeholder}
            disabled={disabled}
            rows={1}
            style={{
              flex: 1,
              padding: '11px 14px',
              background: 'transparent',
              color: 'var(--text)',
              border: 'none',
              outline: 'none',
              resize: 'none',
              minHeight: '44px',
              maxHeight: '200px',
              lineHeight: '1.5',
              fontSize: '14px',
            }}
          />
        </div>

        <button
          onClick={handleSend}
          disabled={disabled || !currentMessage.trim()}
          style={{
            width: '44px', height: '44px',
            background: disabled || !currentMessage.trim() ? 'var(--bg3)' : 'var(--accent)',
            color: disabled || !currentMessage.trim() ? 'var(--text3)' : '#fff',
            borderRadius: '11px',
            fontSize: '18px',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            flexShrink: 0,
            transition: 'background 0.15s, color 0.15s',
            lineHeight: 1,
          }}
        >
          {isProcessing ? <span className="spin" style={{ fontSize: '16px' }}>⟳</span> : '↑'}
        </button>
      </div>

      <p style={{
        textAlign: 'center', fontSize: '11px', color: 'var(--text3)',
        marginTop: '6px', maxWidth: '800px', margin: '6px auto 0',
      }}>
        Enter 发送 · Shift+Enter 换行
      </p>
    </div>
  );
};
