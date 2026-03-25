/**
 * TaskPanelList — 所有后台任务面板的容器
 *
 * 排布在主聊天区域的右侧（或底部，视宽度而定），
 * 以可折叠卡片列表形式展示所有 TaskSession。
 */

import React from 'react';
import { useTaskStore } from '../stores/taskStore';
import { TaskPanel } from './TaskPanel';

export const TaskPanelList: React.FC = () => {
  const tasks = useTaskStore((s) => s.tasks);
  const removeTask = useTaskStore((s) => s.removeTask);

  if (tasks.length === 0) return null;

  const running = tasks.filter((t) => t.status === 'running' || t.status === 'connecting');
  const done = tasks.filter((t) => t.status === 'done' || t.status === 'error');

  return (
    <div style={{
      width: '360px',
      flexShrink: 0,
      display: 'flex',
      flexDirection: 'column',
      borderLeft: '1px solid var(--border)',
      background: 'var(--bg)',
      overflow: 'hidden',
    }}>
      {/* Strip header */}
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '10px 14px 8px',
        borderBottom: '1px solid var(--border)',
        flexShrink: 0,
      }}>
        <span style={{ fontSize: '12px', fontWeight: '600', color: 'var(--text2)', letterSpacing: '0.04em' }}>
          后台任务
        </span>
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          {running.length > 0 && (
            <span style={{
              fontSize: '10px', fontWeight: '600',
              background: 'rgba(16,185,129,0.15)',
              color: '#10b981',
              border: '1px solid rgba(16,185,129,0.3)',
              borderRadius: '10px', padding: '1px 7px',
            }}>
              {running.length} 运行中
            </span>
          )}
          {done.length > 0 && (
            <button
              onClick={() => done.forEach((t) => removeTask(t.id))}
              title="清除已完成任务"
              style={{
                fontSize: '11px', color: 'var(--text3)',
                background: 'transparent', border: '1px solid var(--border)',
                borderRadius: '6px', padding: '1px 6px', cursor: 'pointer',
              }}
            >
              清除完成
            </button>
          )}
        </div>
      </div>

      {/* Panel list */}
      <div style={{
        flex: 1,
        overflowY: 'auto',
        padding: '10px 10px',
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
      }}>
        {/* Running tasks first */}
        {running.map((t) => (
          <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />
        ))}

        {/* Divider when both sections present */}
        {running.length > 0 && done.length > 0 && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: '8px', padding: '4px 0',
          }}>
            <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
            <span style={{ fontSize: '10px', color: 'var(--text3)' }}>已完成</span>
            <div style={{ flex: 1, height: '1px', background: 'var(--border)' }} />
          </div>
        )}

        {/* Done/error tasks */}
        {done.map((t) => (
          <TaskPanel key={t.id} taskId={t.id} onClose={removeTask} />
        ))}
      </div>
    </div>
  );
};
