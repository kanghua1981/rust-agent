import React from 'react';
import { useAgentStore } from '../stores/agentStore';
import { ToolCallCard } from './ToolCallCard';

export const ToolsPanel: React.FC = () => {
  const { toolCalls } = useAgentStore();

  if (toolCalls.length === 0) {
    return (
      <div style={{
        flex: 1, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center',
        color: 'var(--text3)', gap: '10px',
      }}>
        <span style={{ fontSize: '32px' }}>🔧</span>
        <p style={{ fontSize: '14px', color: 'var(--text2)' }}>暂无工具调用记录</p>
      </div>
    );
  }

  const executing = toolCalls.filter(t => t.status === 'executing');
  const completed = toolCalls.filter(t => t.status === 'completed');
  const errored   = toolCalls.filter(t => t.status === 'error');
  const pending   = toolCalls.filter(t => t.status === 'pending');

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
      {/* Summary */}
      <div style={{
        display: 'flex', gap: '8px', marginBottom: '20px', flexWrap: 'wrap',
      }}>
        {[
          { label: '全部', count: toolCalls.length, color: 'var(--text2)' },
          { label: '执行中', count: executing.length, color: 'var(--blue)' },
          { label: '完成', count: completed.length, color: 'var(--green)' },
          { label: '错误', count: errored.length, color: 'var(--red)' },
          { label: '待确认', count: pending.length, color: 'var(--yellow)' },
        ].map(s => (
          <div key={s.label} style={{
            display: 'flex', alignItems: 'center', gap: '4px',
            padding: '4px 10px',
            background: 'var(--bg3)',
            border: '1px solid var(--border)',
            borderRadius: '16px',
            fontSize: '12px',
          }}>
            <span style={{ color: s.color, fontWeight: '600', fontFamily: 'monospace' }}>{s.count}</span>
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
