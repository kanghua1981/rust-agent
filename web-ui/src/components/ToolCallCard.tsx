import React, { useState } from 'react';
import { ToolCall } from '../types/agent';

const toolIcons: Record<string, string> = {
  read_file: '📖', write_file: '✏️', edit_file: '✏️', multi_edit_file: '✏️',
  run_command: '🔨', list_dir: '📂', search: '🔍', think: '🤔',
  batch_read: '📚', browser: '🌐', read_pdf: '📄',
};

const statusLabels: Record<string, string> = {
  pending: '待确认', executing: '执行中', completed: '完成', error: '错误',
};

interface Props {
  toolCall: ToolCall;
}

export const ToolCallCard: React.FC<Props> = ({ toolCall }) => {
  const [expanded, setExpanded] = useState(false);
  const status = statusLabels[toolCall.status] ? toolCall.status : 'executing';
  const icon = toolIcons[toolCall.tool] || '🔧';

  const inputStr = (() => {
    if (toolCall.input == null) return '(空)';
    if (typeof toolCall.input === 'string') return toolCall.input;
    try {
      return JSON.stringify(toolCall.input, null, 2);
    } catch {
      return String(toolCall.input);
    }
  })();

  const output = toolCall.output;
  const isError = toolCall.status === 'error';

  return (
    <div className={`fade-in tool-card ${status}`}>
      <button className="tool-head" onClick={() => setExpanded(!expanded)}>
        <span className="tool-icon">{icon}</span>
        <span className="tool-name">{toolCall.tool}</span>
        {toolCall.status === 'executing' && (
          <span className="spin" style={{ color: 'var(--blue)', fontSize: 13 }}>⟳</span>
        )}
        <span className="tool-status">{statusLabels[status]}</span>
        <span className="tool-chevron">{expanded ? '▲' : '▼'}</span>
      </button>

      {expanded && (
        <div className="tool-body">
          <p className="tool-label">输入</p>
          <pre className="tool-pre">{inputStr}</pre>

          {output && (
            <>
              <p className="tool-label" style={{ marginTop: 10 }}>{isError ? '错误输出' : '输出'}</p>
              <pre className={`tool-pre${isError ? ' error' : ''}`}>
                {typeof output === 'string' ? output : String(output ?? '')}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
};
