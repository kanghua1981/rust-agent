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

  const connected = connectionStatus === 'connected';
  const sendDisabled = !connected || isProcessing;      // ↑ blocks while agent busy
  const dispatchDisabled = !connected;                   // ⚡ only needs a live connection
  const disabled = !connected;                           // textarea stays writable while connected
  const uploadDisabled = !connected;

  const canSend = !sendDisabled && !!currentMessage.trim();
  const canDispatch = !dispatchDisabled && !!currentMessage.trim();

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
    for (let i = 0; i < files.length; i++) onUpload?.(files[i]);
    if (fileInputRef.current) fileInputRef.current.value = '';
  }, [onUpload]);

  const placeholder = !connected
    ? '请先连接服务器…'
    : isProcessing
    ? '正在处理中… Ctrl+Enter 后台发送新任务，或等待完成后 Enter 发送'
    : '发消息给 Agent（Enter 发送，Ctrl+Enter 后台，Shift+Enter 换行）';

  return (
    <div className="input-area">
      <div className="input-row">
        <div className={`input-box${disabled ? ' disabled' : ''}`}>
          <textarea
            ref={textareaRef}
            className="input-textarea"
            value={currentMessage}
            onChange={(e) => setCurrentMessage(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); handleDispatch(); return; }
              if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); }
            }}
            placeholder={placeholder}
            disabled={disabled}
            rows={1}
          />
        </div>

        {isProcessing ? (
          <button className="input-btn cancel" onClick={onCancel} disabled={!onCancel} title="停止">■</button>
        ) : (
          <>
            {onUpload && (
              <>
                <input ref={fileInputRef} type="file" multiple onChange={handleFileSelect} style={{ display: 'none' }} />
                <button className="input-btn" onClick={() => fileInputRef.current?.click()} disabled={uploadDisabled} title="上传文件到 Agent">📎</button>
              </>
            )}
            {onDispatch && (
              <button className="input-btn dispatch" onClick={handleDispatch} disabled={!canDispatch} title="作为后台任务发送（不阻塞主对话）">⚡</button>
            )}
            <button className="input-btn send" onClick={handleSend} disabled={!canSend}>↑</button>
          </>
        )}
      </div>

      <p className="input-hint">Enter 发送 · Ctrl+Enter 后台执行 · Shift+Enter 换行</p>
    </div>
  );
};
