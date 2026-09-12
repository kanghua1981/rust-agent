import React, { useEffect, useMemo, useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { DirEntry } from '../types/agent';

interface Props {
  collapsed: boolean;
  onListDir: (path: string) => void;
  onOpenFile: (path: string) => void;
}

const kindClass = (kind: string): string => {
  switch (kind) {
    case 'modified': return 'modified';
    case 'created':  return 'created';
    case 'deleted':  return 'deleted';
    default:         return '';
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
        className="tree-node"
        onClick={() => {
          if (entry.is_dir) {
            onToggleDir(entry.path);
          } else {
            onOpenFile(entry.path);
          }
        }}
        title={entry.path}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
      >
        {/* Expand/collapse arrow for dirs */}
        <span className={`tree-arrow${isExpanded ? ' open' : ''}${entry.is_dir ? '' : ' hidden'}`}>▶</span>

        {/* Icon */}
        <span className="tree-node-icon">
          {entry.is_dir ? (isExpanded ? '📂' : '📁') : '📄'}
        </span>

        {/* Name */}
        <span className={`tree-node-name${changeKind === 'deleted' ? ' deleted' : ''}`}>
          {entry.name}
        </span>

        {/* Change indicator dot */}
        {changeKind && changeKind !== 'unchanged' && (
          <span className={`tree-change-dot ${kindClass(changeKind)}`} />
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
    <div className="filetree">
      {/* Header */}
      <div className="filetree-head">
        <span className="filetree-title">📂 文件</span>
        <button className="filetree-refresh" onClick={() => onListDir('.')} title="刷新根目录">🔄</button>
      </div>

      {/* Filter input */}
      <input
        className="filetree-filter"
        type="text"
        placeholder="过滤文件..."
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />

      {/* Tree */}
      <div className="filetree-body">
        {connectionStatus !== 'connected' ? (
          <div className="filetree-empty">未连接</div>
        ) : filteredEntries.length === 0 ? (
          <div className="filetree-empty">{filter ? '无匹配文件' : '空目录'}</div>
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
