import React, { useEffect, useMemo, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { DirEntry } from '../types/agent';

interface Props {
  collapsed: boolean;
  onListDir: (path: string) => void;
  onOpenFile: (path: string) => void;
}

const kindDot = (kind: string): React.CSSProperties => {
  switch (kind) {
    case 'modified': return { background: '#f59e0b', boxShadow: '0 0 4px rgba(245,158,11,0.6)' };
    case 'created':  return { background: '#10b981', boxShadow: '0 0 4px rgba(16,185,129,0.6)' };
    case 'deleted':  return { background: '#ef4444', boxShadow: '0 0 4px rgba(239,68,68,0.6)' };
    default:         return { background: 'transparent' };
  }
};

const DirTreeNode: React.FC<{
  entry: DirEntry;
  depth: number;
  expandedDirs: Set<string>;
  dirCache: Record<string, DirEntry[]>;
  changedFilesMap: Record<string, string>;
  onToggleDir: (path: string) => void;
  onOpenFile: (path: string) => void;
}> = ({ entry, depth, expandedDirs, dirCache, changedFilesMap, onToggleDir, onOpenFile }) => {
  const isExpanded = expandedDirs.has(entry.path);
  const changeKind = changedFilesMap[entry.path];

  return (
    <div>
      <div
        onClick={() => {
          if (entry.is_dir) {
            onToggleDir(entry.path);
          } else {
            onOpenFile(entry.path);
          }
        }}
        title={entry.path}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '4px',
          padding: '3px 0',
          paddingLeft: `${8 + depth * 14}px`,
          paddingRight: '8px',
          cursor: 'pointer',
          borderRadius: '4px',
          fontSize: '12px',
          fontFamily: 'monospace',
          color: 'var(--text)',
          background: 'transparent',
          transition: 'background 0.1s',
        }}
        onMouseOver={(e) => {
          e.currentTarget.style.background = 'var(--bg3)';
        }}
        onMouseOut={(e) => {
          e.currentTarget.style.background = 'transparent';
        }}
      >
        {/* Expand/collapse arrow for dirs */}
        <span style={{
          width: '12px',
          fontSize: '9px',
          color: 'var(--text3)',
          flexShrink: 0,
          textAlign: 'center',
          visibility: entry.is_dir ? 'visible' : 'hidden',
          transform: isExpanded ? 'rotate(90deg)' : 'none',
          transition: 'transform 0.15s',
        }}>
          ▶
        </span>

        {/* Icon */}
        <span style={{ fontSize: '13px', flexShrink: 0 }}>
          {entry.is_dir ? (isExpanded ? '📂' : '📁') : '📄'}
        </span>

        {/* Name */}
        <span style={{
          flex: 1,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          textDecoration: changeKind === 'deleted' ? 'line-through' : 'none',
          opacity: changeKind === 'deleted' ? 0.5 : 1,
        }}>
          {entry.name}
        </span>

        {/* Change indicator dot */}
        {changeKind && changeKind !== 'unchanged' && (
          <span style={{
            width: '6px',
            height: '6px',
            borderRadius: '50%',
            flexShrink: 0,
            ...kindDot(changeKind),
          }} />
        )}
      </div>

      {/* Children (lazy loaded) */}
      {entry.is_dir && isExpanded && (
        <div>
          {(dirCache[entry.path] || []).map((child) => (
            <DirTreeNode
              key={child.path}
              entry={child}
              depth={depth + 1}
              expandedDirs={expandedDirs}
              dirCache={dirCache}
              changedFilesMap={changedFilesMap}
              onToggleDir={onToggleDir}
              onOpenFile={onOpenFile}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export const DirectoryTree: React.FC<Props> = ({ collapsed, onListDir, onOpenFile }) => {
  const dirCache = useAgentStore(s => s.dirCache);
  const expandedDirs = useAgentStore(s => s.expandedDirs);
  const changedFilesMap = useAgentStore(s => s.changedFilesMap);
  const connectionStatus = useAgentStore(s => s.connectionStatus);

  const activeProjectId = useAgentStore(s => s.activeProjectId);

  const [filter, setFilter] = useState('');

  // Auto-load root on connect or project switch
  useEffect(() => {
    if (connectionStatus === 'connected') {
      onListDir('.');
    }
  }, [connectionStatus, activeProjectId]);

  const toggleDir = (path: string) => {
    const store = useAgentStore.getState();
    const isExpanded = store.expandedDirs.has(path);
    if (!isExpanded && !store.dirCache[path]) {
      // Lazy load
      onListDir(path);
    } else {
      store.toggleDirExpanded(path);
    }
  };

  const rootEntries = dirCache['.'] || [];

  // Filter entries
  const filteredEntries = useMemo(() => {
    if (!filter.trim()) return rootEntries;
    const lower = filter.toLowerCase();
    const match = (entries: DirEntry[]): DirEntry[] =>
      entries.filter(e => {
        if (!e || !e.name) return false;
        const nameMatch = e.name.toLowerCase().includes(lower);
        if (e.is_dir && e.children?.length) {
          const childMatches = match(e.children);
          return nameMatch || childMatches.length > 0;
        }
        return nameMatch;
      });
    return match(rootEntries);
  }, [rootEntries, filter]);

  if (collapsed) return null;

  return (
    <div style={{
      height: '100%',
      display: 'flex',
      flexDirection: 'column',
      padding: '8px 8px 0',
    }}>
      {/* Header */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        marginBottom: '6px',
        paddingLeft: '4px',
      }}>
        <span style={{
          fontSize: '10px',
          fontWeight: '600',
          color: 'var(--text3)',
          letterSpacing: '0.08em',
          textTransform: 'uppercase',
        }}>
          📂 文件
        </span>
        <button
          onClick={() => onListDir('.')}
          title="刷新根目录"
          style={{
            fontSize: '10px',
            color: 'var(--text3)',
            background: 'transparent',
            border: 'none',
            cursor: 'pointer',
            padding: '0 4px',
            borderRadius: '4px',
          }}
          onMouseOver={(e) => e.currentTarget.style.color = 'var(--text)'}
          onMouseOut={(e) => e.currentTarget.style.color = 'var(--text3)'}
        >
          🔄
        </button>
      </div>

      {/* Filter input */}
      <input
        type="text"
        placeholder="过滤文件..."
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        style={{
          width: '100%',
          padding: '4px 8px',
          marginBottom: '6px',
          background: 'var(--bg3)',
          border: '1px solid var(--border)',
          borderRadius: '6px',
          color: 'var(--text)',
          fontSize: '11px',
          fontFamily: 'monospace',
          outline: 'none',
          boxSizing: 'border-box',
        }}
        onFocus={(e) => e.currentTarget.style.borderColor = 'var(--accent)'}
        onBlur={(e) => e.currentTarget.style.borderColor = 'var(--border)'}
      />

      {/* Tree */}
      <div style={{
        flex: 1,
        minHeight: 0,
        overflowY: 'auto',
        overflowX: 'hidden',
      }}>
        {connectionStatus !== 'connected' ? (
          <div style={{ padding: '8px', color: 'var(--text3)', fontSize: '11px', textAlign: 'center' }}>
            未连接
          </div>
        ) : filteredEntries.length === 0 ? (
          <div style={{ padding: '8px', color: 'var(--text3)', fontSize: '11px', textAlign: 'center' }}>
            {filter ? '无匹配文件' : '空目录'}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <DirTreeNode
              key={entry.path}
              entry={entry}
              depth={0}
              expandedDirs={expandedDirs}
              dirCache={dirCache}
              changedFilesMap={changedFilesMap}
              onToggleDir={toggleDir}
              onOpenFile={onOpenFile}
            />
          ))
        )}
      </div>
    </div>
  );
};
