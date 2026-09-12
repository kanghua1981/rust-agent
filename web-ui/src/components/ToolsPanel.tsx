import React from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ToolCallCard } from './ToolCallCard';

export const ToolsPanel: React.FC = () => {
  const toolCalls = useAgentStore(s => s.toolCalls ?? []);

  if (toolCalls.length === 0) {
    return (
      <div className="tool-empty">
        <span style={{ fontSize: 32 }}>🔧</span>
        <p style={{ fontSize: 14, color: 'var(--text2)' }}>暂无工具调用记录</p>
      </div>
    );
  }

  const executing = toolCalls.filter(t => t.status === 'executing');
  const completed = toolCalls.filter(t => t.status === 'completed');
  const errored   = toolCalls.filter(t => t.status === 'error');
  const pending   = toolCalls.filter(t => t.status === 'pending');

  return (
    <div className="tool-panel-wrap">
      {/* Summary */}
      <div className="tool-panel-summary">
        {[
          { label: '全部', count: toolCalls.length, color: 'var(--text2)' },
          { label: '执行中', count: executing.length, color: 'var(--blue)' },
          { label: '完成', count: completed.length, color: 'var(--green)' },
          { label: '错误', count: errored.length, color: 'var(--red)' },
          { label: '待确认', count: pending.length, color: 'var(--yellow)' },
        ].map(s => (
          <div key={s.label} className="stat-pill">
            <span className="stat-count" style={{ color: s.color }}>{s.count}</span>
            <span style={{ color: 'var(--text2)' }}>{s.label}</span>
          </div>
        ))}
      </div>

      {/* Tool cards */}
      {[...executing, ...pending, ...completed, ...errored].map(tc => (
        <ToolCallCard key={tc.id} toolCall={tc} />
      ))}
    </div>
  );
};
