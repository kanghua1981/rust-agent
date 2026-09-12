import React, { useState } from 'react';

interface Props {
  path: string;
  diff: string;
}

export const DiffViewer: React.FC<Props> = ({ path, diff }) => {
  const [collapsed, setCollapsed] = useState(false);

  const lines = (diff ?? '').split('\n');

  const lineClass = (line: string): string => {
    if (line.startsWith('+') && !line.startsWith('+++')) return 'add';
    if (line.startsWith('-') && !line.startsWith('---')) return 'del';
    if (line.startsWith('@@')) return 'hunk';
    return '';
  };

  return (
    <div className="fade-in diff-card">
      <button className="diff-head" onClick={() => setCollapsed(!collapsed)}>
        <span style={{ fontSize: 14 }}>📝</span>
        <span className="diff-path">{path}</span>
        <span className="diff-badge">diff</span>
        <span className="diff-caret">{collapsed ? '▲' : '▼'}</span>
      </button>

      {!collapsed && (
        <div className="diff-scroll">
          <pre className="diff-pre">
            {lines.map((line, i) => (
              <div key={i} className={`diff-line ${lineClass(line)}`}>{line || ' '}</div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
};
