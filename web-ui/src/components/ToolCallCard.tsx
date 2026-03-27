import React, { useState } from 'react';
import { ToolCall } from '../types/agent';

const toolIcons: Record<string, string> = {
  read_file: '📖', write_file: '✏️', edit_file: '✏️', multi_edit_file: '✏️',
  run_command: '🔨', list_dir: '📂', search: '🔍', think: '🤔',
  batch_read: '📚', browser: '🌐', read_pdf: '📄',
};

const statusStyles: Record<string, { bg: string; color: string; border: string; label: string }> = {
  pending:   { bg: 'var(--yellow-dim)',  color: 'var(--yellow)',  border: 'rgba(245,158,11,0.3)',  label: '待确认' },
  executing: { bg: 'var(--blue-dim)',    color: 'var(--blue)',    border: 'rgba(59,130,246,0.3)',   label: '执行中' },
  completed: { bg: 'var(--green-dim)',   color: 'var(--green)',   border: 'rgba(16,185,129,0.3)',   label: '完成' },
  error:     { bg: 'var(--red-dim)',     color: 'var(--red)',     border: 'rgba(239,68,68,0.3)',    label: '错误' },
};

interface Props {
  toolCall: ToolCall;
}

export const ToolCallCard: React.FC<Props> = ({ toolCall }) => {
  const [expanded, setExpanded] = useState(false);
  const st = statusStyles[toolCall.status] || statusStyles.executing;
  const icon = toolIcons[toolCall.tool] || '🔧';

  const inputStr = typeof toolCall.input === 'string'
    ? toolCall.input
    : JSON.stringify(toolCall.input, null, 2);

  return (
    <div
      className="fade-in"
      style={{
        background: 'var(--surface)',
        border: `1px solid ${st.border}`,
        borderRadius: 'var(--radius)',
        overflow: 'hidden',
        marginBottom: '6px',
      }}
    >
      {/* Header row */}
      <button
        onClick={() => setExpanded(!expanded)}
        style={{
          display: 'flex', alignItems: 'center', gap: '8px',
          padding: '8px 12px', width: '100%', textAlign: 'left',
          background: st.bg,
        }}
      >
        <span style={{ fontSize: '15px' }}>{icon}</span>
        <span style={{ fontFamily: 'monospace', fontWeight: '600', color: 'var(--text)', fontSize: '13px', flex: 1 }}>
          {toolCall.tool}
        </span>
        {toolCall.status === 'executing' && (
          <span className="spin" style={{ color: 'var(--blue)', fontSize: '13px' }}>⟳</span>
        )}
        <span style={{
          fontSize: '11px', fontWeight: '600', color: st.color,
          background: `rgba(0,0,0,0.2)`, padding: '2px 7px', borderRadius: '10px',
        }}>
          {st.label}
        </span>
        <span style={{ color: 'var(--text3)', fontSize: '11px' }}>{expanded ? '▲' : '▼'}</span>
      </button>

      {/* Expanded: input + output */}
      {expanded && (
        <div style={{ padding: '10px 12px', borderTop: `1px solid ${st.border}` }}>
          <p style={{ fontSize: '11px', color: 'var(--text3)', fontWeight: '600', marginBottom: '4px', textTransform: 'uppercase', letterSpacing: '0.06em' }}>输入</p>
          <pre style={{
            background: 'var(--bg)', border: '1px solid var(--border)',
            borderRadius: '6px', padding: '8px', fontSize: '12px',
            overflowX: 'auto', maxHeight: '200px', overflowY: 'auto',
            color: 'var(--text)', margin: 0,
          }}>
            {inputStr}
          </pre>

          {toolCall.output && (
            <>
              <p style={{ fontSize: '11px', color: 'var(--text3)', fontWeight: '600', marginTop: '10px', marginBottom: '4px', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
                {toolCall.status === 'error' ? '错误输出' : '输出'}
              </p>
              <pre style={{
                background: 'var(--bg)', border: `1px solid ${toolCall.status === 'error' ? 'rgba(239,68,68,0.3)' : 'var(--border)'}`,
                borderRadius: '6px', padding: '8px', fontSize: '12px',
                overflowX: 'auto', maxHeight: '300px', overflowY: 'auto',
                color: toolCall.status === 'error' ? 'var(--red)' : 'var(--text)', margin: 0,
              }}>
                {toolCall.output}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
};
