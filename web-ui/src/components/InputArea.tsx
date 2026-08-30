import React, { useRef, useEffect, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';

interface Props {
  onSend: (text: string) => void;
  onCancel?: () => void;
  onDispatch?: (text: string) => void;
  onUpload?: (file: File) => void;
}

export const InputArea: React.FC<Props> = ({ onSend, onCancel, onDispatch, onUpload }) => {
  const { connectionStatus, isProcessing, currentMessage, setCurrentMessage } = useAgentStore();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // sendDisabled: blocks regular ↑ send when agent is busy
  const sendDisabled = connectionStatus !== 'connected' || isProcessing;
  // dispatchDisabled: ⚡ only needs a live connection, not idle agent
  const dispatchDisabled = connectionStatus !== 'connected';
  // textarea is always writable when connected so user can prepare next message
  const disabled = connectionStatus !== 'connected';
  // upload is allowed whenever connected
  const uploadDisabled = connectionStatus !== 'connected';

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 200) + 'px';
  }, [currentMessage]);

  const handleSend = () => {
    const text = currentMessage.trim();
    if (!text || sendDisabled) return;
    onSend(text);
    setCurrentMessage('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  };

  const handleDispatch = () => {
    const text = currentMessage.trim();
    if (!text || dispatchDisabled) return;
    onDispatch?.(text);
    setCurrentMessage('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  };

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    for (let i = 0; i < files.length; i++) {
      onUpload?.(files[i]);
    }
    // Reset so the same file can be selected again
    if (fileInputRef.current) fileInputRef.current.value = '';
  }, [onUpload]);

  const handleAttachClick = () => {
    fileInputRef.current?.click();
  };

  const placeholder = !['connected'].includes(connectionStatus)
    ? '请先连接服务器…'
    : isProcessing
    ? '正在处理中… Ctrl+Enter 后台发送新任务，或等待完成后 Enter 发送'
    : '发消息给 Agent（Enter 发送，Ctrl+Enter 后台，Shift+Enter 换行）';

  return (
    <div style={{
      borderTop: '1px solid var(--border)',
      background: 'var(--bg2)',
      padding: '8px 20px 10px',
      flexShrink: 0,
    }}>
      <div style={{
        maxWidth: '800px',
        margin: '0 auto',
        display: 'flex',
        gap: '8px',
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
              // Ctrl+Enter / Cmd+Enter → background dispatch
              if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
                e.preventDefault();
                handleDispatch();
                return;
              }
              // Enter (without Shift) → regular send
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
              padding: '8px 12px',
              background: 'transparent',
              color: 'var(--text)',
              border: 'none',
              outline: 'none',
              resize: 'none',
              minHeight: '36px',
              maxHeight: '200px',
              lineHeight: '1.5',
              fontSize: '13px',
            }}
          />
        </div>

        {isProcessing ? (
          <button
            onClick={onCancel}
            disabled={!onCancel}
            title="停止"
            style={{
              width: '36px', height: '36px',
              background: 'var(--error, #e53e3e)',
              color: '#fff',
              borderRadius: '9px',
              fontSize: '16px',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0,
              transition: 'background 0.15s',
              lineHeight: 1,
            }}
          >
            ■
          </button>
        ) : (
          <>
            {onUpload && (
              <>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  onChange={handleFileSelect}
                  style={{ display: 'none' }}
                />
                <button
                  onClick={handleAttachClick}
                  disabled={uploadDisabled}
                  title="上传文件到 Agent"
                  style={{
                    width: '36px', height: '36px',
                    background: uploadDisabled ? 'var(--bg3)' : 'var(--bg3)',
                    color: uploadDisabled ? 'var(--text3)' : 'var(--text2)',
                    border: `1px solid ${uploadDisabled ? 'var(--border)' : 'var(--border2)'}`,
                    borderRadius: '9px',
                    fontSize: '16px',
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    flexShrink: 0,
                    cursor: uploadDisabled ? 'not-allowed' : 'pointer',
                    transition: 'background 0.15s, color 0.15s',
                    lineHeight: 1,
                  }}
                >
                  📎
                </button>
              </>
            )}
            {onDispatch && (
              <button
                onClick={handleDispatch}
                disabled={dispatchDisabled || !currentMessage.trim()}
                title="作为后台任务发送（不阻塞主对话）"
                style={{
                  width: '36px', height: '36px',
                  background: dispatchDisabled || !currentMessage.trim() ? 'var(--bg3)' : 'rgba(139,92,246,0.15)',
                  color: dispatchDisabled || !currentMessage.trim() ? 'var(--text3)' : '#8b5cf6',
                  border: `1px solid ${dispatchDisabled || !currentMessage.trim() ? 'var(--border)' : 'rgba(139,92,246,0.4)'}`,
                  borderRadius: '9px',
                  fontSize: '14px',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  flexShrink: 0,
                  cursor: dispatchDisabled || !currentMessage.trim() ? 'not-allowed' : 'pointer',
                  transition: 'background 0.15s, color 0.15s',
                  lineHeight: 1,
                }}
              >
                ⚡
              </button>
            )}
            <button
              onClick={handleSend}
              disabled={sendDisabled || !currentMessage.trim()}
              style={{
                width: '36px', height: '36px',
                background: sendDisabled || !currentMessage.trim() ? 'var(--bg3)' : 'var(--accent)',
                color: sendDisabled || !currentMessage.trim() ? 'var(--text3)' : '#fff',
                borderRadius: '9px',
                fontSize: '16px',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                flexShrink: 0,
                transition: 'background 0.15s, color 0.15s',
                lineHeight: 1,
              }}
            >
              ↑
            </button>
          </>
        )}
      </div>

      <p style={{
        textAlign: 'center', fontSize: '11px', color: 'var(--text3)',
        maxWidth: '800px', margin: '4px auto 0',
      }}>
        Enter 发送 · Ctrl+Enter 后台执行 · Shift+Enter 换行
      </p>
    </div>
  );
};
