import React from 'react';
import { useAgentStore } from '../stores/agentStore';

interface HeaderProps {
  onOpenConnect: () => void;
  onDisconnect: () => void;
}

const statusConfig = {
  disconnected: { color: '#6b7280', label: '未连接', dot: '#374151' },
  connecting:   { color: '#f59e0b', label: '连接中…', dot: '#f59e0b' },
  connected:    { color: '#10b981', label: '已连接',  dot: '#10b981' },
  error:        { color: '#ef4444', label: '连接错误', dot: '#ef4444' },
};

export const Header: React.FC<HeaderProps> = ({ onOpenConnect, onDisconnect }) => {
  const { connectionStatus, serverUrl, workdir, isProcessing, sandboxBackend, pendingChanges, config } = useAgentStore();
  const cfg = statusConfig[connectionStatus];
  const isolation = config.isolation ?? 'container';

  return (
    <header style={{
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '0 20px',
      height: '52px',
      background: 'var(--bg2)',
      borderBottom: '1px solid var(--border)',
      flexShrink: 0,
      gap: '16px',
    }}>
      {/* Logo + title */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
        <div style={{
          width: '28px', height: '28px',
          background: 'linear-gradient(135deg, var(--accent), #8b5cf6)',
          borderRadius: '7px',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: '14px', flexShrink: 0,
        }}>🤖</div>
        <span style={{ fontWeight: '600', color: 'var(--text)', fontSize: '15px', letterSpacing: '-0.3px' }}>
          Rust Agent
        </span>
      </div>

      {/* Status pill */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: '6px',
        padding: '4px 10px',
        background: 'var(--bg3)',
        border: '1px solid var(--border)',
        borderRadius: '20px',
        flexShrink: 0,
      }}>
        <span style={{
          width: '7px', height: '7px', borderRadius: '50%',
          background: cfg.dot,
          boxShadow: connectionStatus === 'connected' ? `0 0 6px ${cfg.dot}` : 'none',
          display: 'inline-block',
          flexShrink: 0,
        }} />
        <span style={{ fontSize: '12px', color: cfg.color, fontWeight: '500' }}>{cfg.label}</span>
        {isProcessing && <span className="spin" style={{ fontSize: '11px', color: 'var(--accent)' }}>⟳</span>}
      </div>

      {/* Center: server info */}
      {connectionStatus === 'connected' && (
        <div style={{
          flex: 1, display: 'flex', alignItems: 'center', gap: '12px',
          overflow: 'hidden', minWidth: 0,
        }}>
          <span className="truncate" style={{ fontSize: '12px', color: 'var(--text3)', fontFamily: 'monospace' }}>
            {serverUrl}
          </span>
          {workdir && (
            <span className="truncate" style={{ fontSize: '12px', color: 'var(--text2)', fontFamily: 'monospace' }}>
              📂 {workdir}
            </span>
          )}
          {/* Isolation mode badge */}
          {isolation === 'sandbox' ? (
            <span style={{
              display: 'inline-flex', alignItems: 'center', gap: '4px',
              padding: '2px 8px',
              background: pendingChanges > 0 ? 'rgba(245,158,11,0.15)' : 'rgba(16,185,129,0.12)',
              border: `1px solid ${pendingChanges > 0 ? 'rgba(245,158,11,0.4)' : 'rgba(16,185,129,0.3)'}`,
              borderRadius: '10px',
              fontSize: '11px', fontWeight: '500',
              color: pendingChanges > 0 ? '#f59e0b' : '#10b981',
              flexShrink: 0,
            }}>
              🔒 沙筱{pendingChanges > 0 ? ` · ${pendingChanges} 待提交` : ''}
            </span>
          ) : isolation === 'container' ? (
            <span style={{
              display: 'inline-flex', alignItems: 'center', gap: '4px',
              padding: '2px 8px',
              background: 'rgba(59,130,246,0.12)',
              border: '1px solid rgba(59,130,246,0.3)',
              borderRadius: '10px',
              fontSize: '11px', fontWeight: '500',
              color: '#3b82f6',
              flexShrink: 0,
            }}>
              🔲 容器
            </span>
          ) : (
            <span style={{
              display: 'inline-flex', alignItems: 'center', gap: '4px',
              padding: '2px 8px',
              background: 'rgba(107,114,128,0.12)',
              border: '1px solid rgba(107,114,128,0.3)',
              borderRadius: '10px',
              fontSize: '11px', fontWeight: '500',
              color: '#6b7280',
              flexShrink: 0,
            }}>
              🕑3 无容器
            </span>
          )}
        </div>
      )}

      {/* Actions */}
      <div style={{ display: 'flex', gap: '8px', flexShrink: 0, marginLeft: 'auto' }}>
        {connectionStatus !== 'connected' ? (
          <button
            onClick={onOpenConnect}
            style={{
              padding: '5px 14px',
              background: 'var(--accent)',
              color: '#fff',
              borderRadius: '7px',
              fontWeight: '500',
              fontSize: '13px',
              transition: 'opacity 0.15s',
            }}
            onMouseOver={(e) => (e.currentTarget.style.opacity = '0.85')}
            onMouseOut={(e) => (e.currentTarget.style.opacity = '1')}
          >
            连接服务器
          </button>
        ) : (
          <button
            onClick={onDisconnect}
            style={{
              padding: '5px 14px',
              background: 'var(--red-dim)',
              color: 'var(--red)',
              borderRadius: '7px',
              fontWeight: '500',
              fontSize: '13px',
              border: '1px solid rgba(239,68,68,0.3)',
            }}
          >
            断开
          </button>
        )}
      </div>
    </header>
  );
};
