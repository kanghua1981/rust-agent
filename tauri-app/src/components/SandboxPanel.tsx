import React, { useEffect, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { DiffViewer } from './DiffViewer';
import { SandboxFileChange } from '../stores/agentStore';

interface Props {
  onSandboxListChanges: () => void;
  onCommit: () => void;
  onCommitFile: (filePath: string) => void;
  onRollback: () => void;
}

const kindBadge = (kind: string): { label: string; bg: string; color: string } => {
  switch (kind) {
    case 'modified':  return { label: 'M', bg: 'rgba(245,158,11,0.2)', color: '#f59e0b' };
    case 'created':   return { label: 'C', bg: 'rgba(16,185,129,0.2)', color: '#10b981' };
    case 'deleted':   return { label: 'D', bg: 'rgba(239,68,68,0.2)',  color: '#ef4444' };
    default:          return { label: 'U', bg: 'var(--bg3)',            color: 'var(--text3)' };
  }
};

const formatSize = (n: number | null): string => {
  if (n === null) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
};

export const SandboxPanel: React.FC<Props> = ({ onSandboxListChanges, onCommit, onCommitFile, onRollback }) => {
  const { sandboxBackend, pendingChanges, sandboxChangesData, connectionStatus, config, isProcessing } = useAgentStore();
  const [confirmAction, setConfirmAction] = useState<'commit' | 'rollback' | null>(null);
  const [expandedDiffs, setExpandedDiffs] = useState<Set<string>>(new Set());

  // Auto-fetch when tab becomes active (sandbox state or connection changes)
  useEffect(() => {
    if (connectionStatus === 'connected' && config.isolation === 'sandbox' && sandboxBackend !== 'disabled') {
      onSandboxListChanges();
    }
  }, [connectionStatus, config.isolation, sandboxBackend]);

  // Auto-fetch when agent finishes processing (files may have been written)
  useEffect(() => {
    if (!isProcessing && connectionStatus === 'connected' && config.isolation === 'sandbox' && sandboxBackend !== 'disabled') {
      onSandboxListChanges();
    }
  }, [isProcessing]);

  const toggleDiff = (path: string) => {
    setExpandedDiffs(prev => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });
  };

  const handleConfirm = (action: 'commit' | 'rollback') => {
    if (action === 'commit') onCommit();
    else onRollback();
    setConfirmAction(null);
  };

  const isDisabled = sandboxBackend === 'disabled' || config.isolation !== 'sandbox';

  if (connectionStatus !== 'connected') {
    return (
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text3)', padding: '40px' }}>
        <p style={{ textAlign: 'center', fontSize: '14px' }}>未连接到服务器</p>
      </div>
    );
  }

  if (isDisabled) {
    return (
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', color: 'var(--text3)', padding: '40px', gap: '12px' }}>
        <span style={{ fontSize: '40px' }}>🚧</span>
        <p style={{ textAlign: 'center', fontSize: '14px' }}>沙盒未启用</p>
        <p style={{ textAlign: 'center', fontSize: '12px', color: 'var(--text3)' }}>在设置中开启沙盒后，此面板显示所有文件变更</p>
      </div>
    );
  }

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', background: 'var(--bg)' }}>
      {/* Toolbar */}
      <div style={{
        padding: '12px 16px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        alignItems: 'center',
        gap: '10px',
        background: 'var(--bg2)',
        flexShrink: 0,
      }}>
        <div style={{ flex: 1 }}>
          <span style={{ fontSize: '14px', fontWeight: '600', color: 'var(--text)' }}>沙盒变更</span>
          {sandboxChangesData !== null && (
            <span style={{ marginLeft: '8px', fontSize: '12px', color: 'var(--text3)' }}>
              {sandboxChangesData.length} 个文件
            </span>
          )}
          <span style={{
            marginLeft: '8px',
            background: sandboxBackend === 'overlay' ? 'rgba(16,185,129,0.15)' : 'rgba(245,158,11,0.15)',
            color: sandboxBackend === 'overlay' ? '#10b981' : '#f59e0b',
            borderRadius: '6px',
            padding: '1px 7px',
            fontSize: '11px',
            fontWeight: '600',
          }}>
            {sandboxBackend === 'overlay' ? 'overlay' : '快照'}
          </span>
        </div>

        <button
          onClick={onSandboxListChanges}
          style={{
            padding: '5px 11px',
            borderRadius: '6px',
            background: 'var(--bg3)',
            color: 'var(--text2)',
            border: '1px solid var(--border)',
            fontSize: '12px',
            cursor: 'pointer',
          }}
        >
          🔄 刷新
        </button>

        <button
          onClick={() => setConfirmAction('rollback')}
          style={{
            padding: '5px 11px',
            borderRadius: '6px',
            background: 'rgba(239,68,68,0.15)',
            color: '#f87171',
            border: '1px solid rgba(239,68,68,0.3)',
            fontSize: '12px',
            cursor: pendingChanges === 0 ? 'not-allowed' : 'pointer',
            opacity: pendingChanges === 0 ? 0.5 : 1,
          }}
          disabled={pendingChanges === 0}
        >
          ↩ 回滚
        </button>

        <button
          onClick={() => setConfirmAction('commit')}
          style={{
            padding: '5px 11px',
            borderRadius: '6px',
            background: 'rgba(99,102,241,0.15)',
            color: '#818cf8',
            border: '1px solid rgba(99,102,241,0.3)',
            fontSize: '12px',
            cursor: pendingChanges === 0 ? 'not-allowed' : 'pointer',
            opacity: pendingChanges === 0 ? 0.5 : 1,
          }}
          disabled={pendingChanges === 0}
        >
          ✅ 提交
        </button>
      </div>

      {/* Confirm dialog overlay */}
      {confirmAction && (
        <div style={{
          position: 'absolute', inset: 0, zIndex: 100,
          background: 'rgba(0,0,0,0.6)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}>
          <div style={{
            background: 'var(--bg2)',
            border: '1px solid var(--border)',
            borderRadius: '12px',
            padding: '24px',
            maxWidth: '380px',
            width: '90%',
            boxShadow: '0 20px 60px rgba(0,0,0,0.5)',
          }}>
            <p style={{ fontSize: '15px', fontWeight: '600', color: 'var(--text)', marginBottom: '10px' }}>
              {confirmAction === 'commit' ? '确认提交变更？' : '确认回滚变更？'}
            </p>
            <p style={{ fontSize: '13px', color: 'var(--text2)', marginBottom: '20px' }}>
              {confirmAction === 'commit'
                ? `将把沙盒中的 ${pendingChanges} 个变更写入真实文件系统，操作不可撤销。`
                : `将丢弃沙盒中的所有 ${pendingChanges} 个变更，恢复到操作前的状态。`}
            </p>
            <div style={{ display: 'flex', gap: '10px', justifyContent: 'flex-end' }}>
              <button
                onClick={() => setConfirmAction(null)}
                style={{
                  padding: '7px 16px', borderRadius: '8px',
                  background: 'var(--bg3)', color: 'var(--text2)',
                  border: '1px solid var(--border)', fontSize: '13px', cursor: 'pointer',
                }}
              >
                取消
              </button>
              <button
                onClick={() => handleConfirm(confirmAction)}
                style={{
                  padding: '7px 16px', borderRadius: '8px',
                  background: confirmAction === 'commit' ? 'rgba(99,102,241,0.8)' : 'rgba(239,68,68,0.8)',
                  color: '#fff',
                  border: 'none', fontSize: '13px', cursor: 'pointer', fontWeight: '600',
                }}
              >
                {confirmAction === 'commit' ? '确认提交' : '确认回滚'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* File list */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '12px 16px' }}>
        {sandboxChangesData === null ? (
          <div style={{ textAlign: 'center', color: 'var(--text3)', paddingTop: '40px', fontSize: '13px' }}>
            点击"刷新"查看变更列表
          </div>
        ) : sandboxChangesData.length === 0 ? (
          <div style={{ textAlign: 'center', color: 'var(--text3)', paddingTop: '40px', fontSize: '13px' }}>
            沙盒中没有未提交的变更
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
            {sandboxChangesData.map((file: SandboxFileChange) => {
              const badge = kindBadge(file.kind);
              const hasDiff = !!file.diff;
              const isExpanded = expandedDiffs.has(file.path);

              return (
                <div key={file.path} style={{
                  background: 'var(--bg2)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  overflow: 'hidden',
                }}>
                  {/* File row */}
                  <div
                    onClick={() => hasDiff && toggleDiff(file.path)}
                    style={{
                      display: 'flex', alignItems: 'center', gap: '10px',
                      padding: '9px 12px',
                      cursor: hasDiff ? 'pointer' : 'default',
                    }}
                    onMouseOver={e => { if (hasDiff) (e.currentTarget as HTMLElement).style.background = 'var(--bg3)'; }}
                    onMouseOut={e => { (e.currentTarget as HTMLElement).style.background = ''; }}
                  >
                    {/* Kind badge */}
                    <span style={{
                      background: badge.bg, color: badge.color,
                      borderRadius: '4px', padding: '1px 6px',
                      fontSize: '11px', fontWeight: '700',
                      flexShrink: 0,
                    }}>
                      {badge.label}
                    </span>

                    {/* Path */}
                    <span style={{
                      flex: 1, fontFamily: 'monospace', fontSize: '12px',
                      color: 'var(--text)', wordBreak: 'break-all',
                    }}>
                      {file.path}
                    </span>

                    {/* Size info */}
                    {(file.original_size !== null || file.current_size !== null) && (
                      <span style={{ fontSize: '11px', color: 'var(--text3)', flexShrink: 0 }}>
                        {file.kind === 'created'
                          ? formatSize(file.current_size)
                          : file.kind === 'deleted'
                          ? formatSize(file.original_size)
                          : `${formatSize(file.original_size)} → ${formatSize(file.current_size)}`
                        }
                      </span>
                    )}

                    {/* Commit file button */}
                    {file.kind !== 'deleted' && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onCommitFile(file.path);
                        }}
                        title={`提交 ${file.path}`}
                        style={{
                          padding: '2px 8px',
                          borderRadius: '4px',
                          background: 'rgba(99,102,241,0.15)',
                          color: '#818cf8',
                          border: '1px solid rgba(99,102,241,0.3)',
                          fontSize: '10px',
                          cursor: 'pointer',
                          flexShrink: 0,
                        }}
                      >
                        ✓
                      </button>
                    )}

                    {hasDiff && (
                      <span style={{ fontSize: '12px', color: 'var(--text3)', flexShrink: 0 }}>
                        {isExpanded ? '▲' : '▼'}
                      </span>
                    )}
                  </div>

                  {/* Diff viewer */}
                  {hasDiff && isExpanded && (
                    <div style={{ borderTop: '1px solid var(--border)' }}>
                      <DiffViewer path={file.path} diff={file.diff!} />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
