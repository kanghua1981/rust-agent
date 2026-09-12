import React, { useState } from 'react';
import { useAgentStore, SandboxFileChange } from '../stores/agentStore';
import { DiffViewer } from './DiffViewer';

interface Props {
  onSandboxListChanges: () => void;
  onCommit: () => void;
  onCommitFile: (filePath: string) => void;
  onRollback: () => void;
}

const kindBadge = (kind: string): { label: string; cls: string } => {
  switch (kind) {
    case 'modified':  return { label: 'M', cls: 'modified' };
    case 'created':   return { label: 'C', cls: 'created' };
    case 'deleted':   return { label: 'D', cls: 'deleted' };
    default:          return { label: 'U', cls: 'untracked' };
  }
};

const formatSize = (n: number | null): string => {
  if (n === null) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
};

export const ChangesList: React.FC<Props> = ({ onSandboxListChanges, onCommit, onCommitFile, onRollback }) => {
  const { sandboxBackend, pendingChanges, sandboxChangesData } = useAgentStore();
  const [confirmAction, setConfirmAction] = useState<'commit' | 'rollback' | null>(null);
  const [expandedDiffs, setExpandedDiffs] = useState<Set<string>>(new Set());

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

  return (
    <div className="panel">
      {/* Toolbar */}
      <div className="changes-bar">
        <div className="fill" style={{ minWidth: 120 }}>
          <span className="changes-title">沙盒变更</span>
          {sandboxChangesData !== null && (
            <span className="changes-count">{sandboxChangesData.length} 个文件</span>
          )}
          <span className={`badge-backend ${sandboxBackend === 'overlay' ? 'overlay' : 'snapshot'}`}>
            {sandboxBackend === 'overlay' ? 'overlay' : '快照'}
          </span>
        </div>

        <button className="btn-mini" onClick={onSandboxListChanges}>🔄 刷新</button>
        <button
          className="btn-mini danger"
          onClick={() => setConfirmAction('rollback')}
          disabled={pendingChanges === 0}
        >↩ 回滚</button>
        <button
          className="btn-mini accent"
          onClick={() => setConfirmAction('commit')}
          disabled={pendingChanges === 0}
        >✅ 提交</button>
      </div>

      {/* Confirm dialog overlay */}
      {confirmAction && (
        <div className="dlg-overlay">
          <div className="dlg">
            <p className="dlg-title">
              {confirmAction === 'commit' ? '确认提交变更？' : '确认回滚变更？'}
            </p>
            <p className="dlg-text">
              {confirmAction === 'commit'
                ? `将把沙盒中的 ${pendingChanges} 个变更写入真实文件系统，操作不可撤销。`
                : `将丢弃沙盒中的所有 ${pendingChanges} 个变更，恢复到操作前的状态。`}
            </p>
            <div className="dlg-actions">
              <button className="btn-lg ghost" onClick={() => setConfirmAction(null)}>取消</button>
              <button
                className={`btn-lg confirm${confirmAction === 'rollback' ? ' danger' : ''}`}
                onClick={() => handleConfirm(confirmAction)}
              >
                {confirmAction === 'commit' ? '确认提交' : '确认回滚'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* File list */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '8px 10px' }}>
        {sandboxChangesData === null ? (
          <div className="empty-line">点击"刷新"查看变更列表</div>
        ) : sandboxChangesData.length === 0 ? (
          <div className="empty-line">沙盒中没有未提交的变更</div>
        ) : (
          <div className="col" style={{ gap: 4 }}>
            {sandboxChangesData.map((file: SandboxFileChange) => {
              const badge = kindBadge(file.kind);
              const hasDiff = !!file.diff;
              const isExpanded = expandedDiffs.has(file.path);

              return (
                <div key={file.path} className="file-card">
                  {/* File row */}
                  <div
                    className={`file-row${hasDiff ? ' clickable' : ''}`}
                    onClick={() => hasDiff && toggleDiff(file.path)}
                  >
                    <span className={`kind-badge ${badge.cls}`}>{badge.label}</span>

                    <span className="file-path">{file.path}</span>

                    {(file.original_size !== null || file.current_size !== null) && (
                      <span className="file-size">
                        {file.kind === 'created'
                          ? formatSize(file.current_size)
                          : file.kind === 'deleted'
                          ? formatSize(file.original_size)
                          : `${formatSize(file.original_size)} → ${formatSize(file.current_size)}`
                        }
                      </span>
                    )}

                    {file.kind !== 'deleted' && (
                      <button
                        className="btn-xs primary"
                        onClick={(e) => {
                          e.stopPropagation();
                          onCommitFile(file.path);
                        }}
                        title={`提交 ${file.path}`}
                      >✓</button>
                    )}

                    {hasDiff && <span className="file-size">{isExpanded ? '▲' : '▼'}</span>}
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
