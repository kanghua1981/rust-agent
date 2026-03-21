import React, { useState } from 'react';

interface Props {
  path: string;
  diff: string;
}

export const DiffViewer: React.FC<Props> = ({ path, diff }) => {
  const [collapsed, setCollapsed] = useState(false);

  const lines = diff.split('\n');

  const lineColor = (line: string): React.CSSProperties => {
    if (line.startsWith('+') && !line.startsWith('+++')) return { background: 'rgba(16,185,129,0.12)', color: '#6ee7b7' };
    if (line.startsWith('-') && !line.startsWith('---')) return { background: 'rgba(239,68,68,0.12)', color: '#fca5a5' };
    if (line.startsWith('@@')) return { background: 'rgba(99,102,241,0.1)', color: '#a5b4fc' };
    return { color: 'var(--text2)' };
  };

  return (
    <div className="fade-in" style={{
      background: 'var(--surface)',
      border: '1px solid rgba(99,102,241,0.3)',
      borderRadius: 'var(--radius)',
      overflow: 'hidden',
      marginBottom: '6px',
    }}>
      <button
        onClick={() => setCollapsed(!collapsed)}
        style={{
          display: 'flex', alignItems: 'center', gap: '8px',
          padding: '8px 12px', width: '100%', textAlign: 'left',
          background: 'rgba(99,102,241,0.08)',
        }}
      >
        <span style={{ fontSize: '14px' }}>📝</span>
        <span style={{ fontFamily: 'monospace', color: 'var(--accent)', fontSize: '13px', flex: 1 }}>{path}</span>
        <span style={{
          fontSize: '11px', color: 'var(--accent)', background: 'var(--accent-glow)',
          padding: '2px 7px', borderRadius: '10px', fontWeight: '600',
        }}>diff</span>
        <span style={{ color: 'var(--text3)', fontSize: '11px' }}>{collapsed ? '▲' : '▼'}</span>
      </button>

      {!collapsed && (
        <div style={{ overflowX: 'auto' }}>
          <pre style={{
            margin: 0, padding: '10px 0',
            fontSize: '12px', lineHeight: '1.55',
            fontFamily: 'monospace',
            background: 'var(--bg)',
            maxHeight: '350px',
            overflowY: 'auto',
          }}>
            {lines.map((line, i) => (
              <div key={i} style={{ padding: '0 14px', ...lineColor(line) }}>
                {line || ' '}
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
};
