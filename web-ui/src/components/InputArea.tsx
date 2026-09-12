import React, { useRef, useEffect, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';

/** Must match .input-textarea max-height in index.css. */
const MAX_INPUT_HEIGHT = 320;

interface Props {
  onSend: (text: string) => void;
  onCancel?: () => void;
  onDispatch?: (text: string) => void;
  onUpload?: (file: File) => void;
  /** Switch the model used for the next turn (WebSocket action). */
  onSetModelRemote?: (alias: string) => void;
  /** Start a fresh session (WebSocket action). */
  onNewSession?: () => void;
}

export const InputArea: React.FC<Props> = ({ onSend, onCancel, onDispatch, onUpload, onSetModelRemote, onNewSession }) => {
  // Selective subscriptions — this component renders on every keystroke, so it
  // must not subscribe to messages/tool calls that change on each streamed token.
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const isProcessing = useAgentStore(s => s.isProcessing);
  const currentMessage = useAgentStore(s => s.currentMessage);
  const setCurrentMessage = useAgentStore(s => s.setCurrentMessage);
  const setAgentMode = useAgentStore(s => s.setAgentMode);
  const availableModels = useAgentStore(s => s.availableModels);
  const activeModel = useAgentStore(s => s.activeModel);
  const agentMode = useAgentStore(s => s.agentMode);

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
    el.style.height = Math.min(el.scrollHeight, MAX_INPUT_HEIGHT) + 'px';
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

      <div className="input-meta">
        <div className="input-meta-left">
          {availableModels.length > 0 && (
            <select
              className="select-sm model-pick"
              value={activeModel ?? ''}
              onChange={(e) => { const alias = e.target.value; if (alias) onSetModelRemote?.(alias); }}
              disabled={!connected}
              title="本轮使用的模型"
            >
              {activeModel == null && <option value="">默认模型</option>}
              {availableModels.map(m => (
                <option key={m.alias} value={m.alias}>🧠 {m.alias}</option>
              ))}
            </select>
          )}
          <select
            className="select-sm"
            value={agentMode || 'auto'}
            onChange={(e) => setAgentMode(e.target.value as 'auto' | 'simple' | 'plan')}
            title="本轮使用的运行模式"
          >
            <option value="auto">🤖 自动</option>
            <option value="simple">⚡ 单层</option>
            <option value="plan">📋 计划</option>
          </select>
          {connected && onNewSession && (
            <button className="btn-ghost" onClick={onNewSession} title="新建会话 (Ctrl+Shift+N)">
              <span>➕</span><span>新会话</span>
            </button>
          )}
        </div>
        <span className="input-hint">Enter 发送 · Ctrl+Enter 后台 · Shift+Enter 换行</span>
      </div>
    </div>
  );
};
