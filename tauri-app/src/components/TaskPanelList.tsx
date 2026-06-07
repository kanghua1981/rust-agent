/**
 * TaskPanelList — 所有后台任务面板的容器
 *
 * 排布在主聊天区域的右侧，支持折叠/展开双态：
 * - 折叠态：极窄竖条（36px），显示竖排文字 + 运行中任务数
 * - 展开态：完整 360px 面板（与原有功能一致）
 * - 无任务时完全隐藏
 */

import React, { useState } from 'react';
import { useTaskStore } from '../stores/taskStore';
import { TaskPanel } from './TaskPanel';

export const TaskPanelList: React.FC = () => {
  const tasks = useTaskStore((s) => s.tasks);
  const removeTask = useTaskStore((s) => s.removeTask);

  // 折叠/展开状态，持久化到 localStorage
  const [collapsed, setCollapsed] = useState(() => {
    try { return localStorage.getItem('taskpanel-collapsed') !== 'false'; } catch { return true; }
  });

  const toggleCollapsed = () => {
    setCollapsed(prev => {
      const next = !prev;
      try { localStorage.setItem('taskpanel-collapsed', String(next)); } catch {}
      return next;
    });
  };

  if (tasks.length === 0) return null;

  const running = tasks.filter((t) => t.status === 'running' || t.status === 'connecting');
  const done = tasks.filter((t) => t.status === 'done' || t.status === 'error');

  // ── 折叠态：极窄竖条 ──────────────────────────────────────────
  if (collapsed) {
    return (
      <div
        className="taskpanel-collapsed"
        onClick={toggleCollapsed}
        title="展开后台任务"
        style={{
          width: '36px',
          flexShrink: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'flex-start',
          borderLeft: '1px solid var(--border)',
          background: 'var(--bg)',
          cursor: 'pointer',
          padding: '12px 0',
          gap: '10px',
          transition: 'background 0.15s',
          userSelect: 'none',
        }}
        onMouseOver={(e) => e.currentTarget.style.background = 'var(--bg2)'}
        onMouseOut={(e) => e.currentTarget.style.background = 'var(--bg)'}
      >
        {/* Vertical label */}
        <span style={{
          writingMode: 'vertical-rl',
          letterSpacing: '0.15em',
          fontSize: '11px',
          fontWeight: '600',
          color: 'var(--text2)',
        }}>
          后台
        </span>

        {/* Running count badge */}
        {running.length > 0 && (
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            gap: '4px',
          }}>
            <span style={{
              width: '7px', height: '7px',
              borderRadius: '50%',
              background: '#10b981',
              boxShadow: '0 0 6px rgba(16,185,129,0.6)',
              display: 'inline-block',
              animation: 'pulse-green 1.5s ease-in-out infinite',
            }} />
            <span style={{
              fontSize: '12px', fontWeight: '700',
              color: '#10b981',
              lineHeight: 1,
            }}>
              {running.length}
            </span>
          </div>
        )}

        {/* Indicator dot when done tasks exist but nothing running */}
        {running.length === 0 && done.length > 0 && (
          <span style={{
            width: '5px', height: '5px',
            borderRadius: '50%',
            background: 'var(--text3)',
            display: 'inline-block',
          }} />
        )}

        {/* Expand arrow */}
        <span style={{
          fontSize: '10px',
          color: 'var(--text3)',
          marginTop: 'auto',
          transition: 'color 0.15s',
        }}>
          ▶
        </span>
      </div>
    );
  }

  // ── 展开态：完整面板 ──────────────────────────────────────────
  return (
    <div
      className="taskpanel-expanded"
      style={{
        width: '360px',
        flexShrink: 0,
        display: 'flex',
        flexDirection: 'column',
        borderLeft: '1px solid var(--border)',
        background: 'var(--bg)',
        overflow: 'hidden',
        transition: 'width 0.2s ease',
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
          {/* Collapse button */}
          <button
            onClick={toggleCollapsed}
            title="折叠后台任务面板"
            style={{
              fontSize: '12px', color: 'var(--text3)',
              background: 'transparent', border: 'none',
              borderRadius: '4px', padding: '2px 4px', cursor: 'pointer',
              transition: 'color 0.15s',
            }}
            onMouseOver={(e) => e.currentTarget.style.color = 'var(--text)'}
            onMouseOut={(e) => e.currentTarget.style.color = 'var(--text3)'}
          >
            ◀
          </button>
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
