import React from 'react';
import { useAgentStore } from '../stores/agentStore';

interface HeaderProps {
  onOpenConnect: () => void;
  onDisconnect: () => void;
  onNewSession?: () => void;
}

const statusConfig = {
  disconnected: { color: '#6b7280', label: '未连接', dot: '#374151' },
  connecting:   { color: '#f59e0b', label: '连接中…', dot: '#f59e0b' },
  connected:    { color: '#10b981', label: '已连接',  dot: '#10b981' },
  error:        { color: '#ef4444', label: '连接错误', dot: '#ef4444' },
};

export const Header: React.FC<HeaderProps> = ({ onOpenConnect, onDisconnect, onNewSession }) => {
  const { connectionStatus, serverUrl, workdir, isProcessing, sandboxBackend, pendingChanges, config, messages, toolCalls, pendingConfirmations } = useAgentStore();
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

      {/* Status pill - clickable to open connection modal */}
      <button
        onClick={onOpenConnect}
        style={{
          display: 'flex', alignItems: 'center', gap: '6px',
          padding: '4px 10px',
          background: 'var(--bg3)',
          border: '1px solid var(--border)',
          borderRadius: '20px',
          flexShrink: 0,
          cursor: 'pointer',
          transition: 'all 0.15s',
        }}
        onMouseOver={(e) => e.currentTarget.style.background = 'var(--bg4)'}
        onMouseOut={(e) => e.currentTarget.style.background = 'var(--bg3)'}
      >
        <span style={{
          width: '7px', height: '7px', borderRadius: '50%',
          background: cfg.dot,
          boxShadow: connectionStatus === 'connected' ? `0 0 6px ${cfg.dot}` : 'none',
          display: 'inline-block',
          flexShrink: 0,
        }} />
        <span style={{ fontSize: '12px', color: cfg.color, fontWeight: '500' }}>{cfg.label}</span>
        {isProcessing && <span className="spin" style={{ fontSize: '11px', color: 'var(--accent)' }}>⟳</span>}
      </button>

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
              🔒 沙盒{pendingChanges > 0 ? ` · ${pendingChanges} 待提交` : ''}
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

      {/* Stats badges - 只在连接状态下显示 */}
      {connectionStatus === 'connected' && (
        <div style={{ display: 'flex', gap: '6px', flexShrink: 0, marginLeft: 'auto' }}>
          {/* 消息数量徽章 */}
          <div style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: '4px',
            padding: '2px 8px',
            background: 'var(--bg3)',
            border: '1px solid var(--border)',
            borderRadius: '10px',
            fontSize: '11px',
            fontWeight: '500',
            color: 'var(--text2)',
            flexShrink: 0,
          }}>
            <span>💬</span>
            <span>{messages.length}</span>
          </div>
          
          {/* 工具调用徽章 */}
          <div style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: '4px',
            padding: '2px 8px',
            background: 'var(--bg3)',
            border: '1px solid var(--border)',
            borderRadius: '10px',
            fontSize: '11px',
            fontWeight: '500',
            color: 'var(--text2)',
            flexShrink: 0,
          }}>
            <span>🔨</span>
            <span>{toolCalls.length}</span>
          </div>
          
          {/* 待确认徽章（只在有确认时显示） */}
          {pendingConfirmations.length > 0 && (
            <div style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: '4px',
              padding: '2px 8px',
              background: 'rgba(245,158,11,0.15)',
              border: '1px solid rgba(245,158,11,0.4)',
              borderRadius: '10px',
              fontSize: '11px',
              fontWeight: '500',
              color: '#f59e0b',
              flexShrink: 0,
            }}>
              <span>⏳</span>
              <span>{pendingConfirmations.length}</span>
            </div>
          )}
        </div>
      )}

      {/* 快捷操作工具栏 - 只在连接状态下显示 */}
      {connectionStatus === 'connected' && (
        <div style={{ 
          display: 'flex', 
          alignItems: 'center', 
          gap: '6px', 
          marginLeft: '12px',
          flexShrink: 0,
        }}>
          
          {/* 运行模式快捷切换 */}
          <div style={{ position: 'relative' }}>
            <select
              value={config.agentMode || 'auto'}
              onChange={(e) => {
                const newMode = e.target.value as 'auto' | 'simple' | 'plan' | 'pipeline';
                useAgentStore.getState().setConfig({ agentMode: newMode });
              }}
              style={{
                padding: '4px 8px 4px 28px',
                background: 'var(--bg3)',
                border: '1px solid var(--border)',
                borderRadius: '6px',
                fontSize: '11px',
                color: 'var(--text)',
                cursor: 'pointer',
                appearance: 'none',
                minWidth: '100px',
              }}
              title="切换运行模式"
            >
              <option value="auto">🤖 自动</option>
              <option value="simple">⚡ 单层</option>
              <option value="plan">📋 计划</option>
              <option value="pipeline">🔀 流水线</option>
            </select>
            <span style={{
              position: 'absolute',
              left: '8px',
              top: '50%',
              transform: 'translateY(-50%)',
              fontSize: '12px',
              pointerEvents: 'none',
            }}>
              {config.agentMode === 'auto' ? '🤖' : 
               config.agentMode === 'simple' ? '⚡' : 
               config.agentMode === 'plan' ? '📋' : '🔀'}
            </span>
          </div>

          {/* 清空会话按钮 */}
          <button
            onClick={() => {
              if (window.confirm('确定要清空当前会话的所有消息吗？此操作不可撤销。')) {
                useAgentStore.getState().clearSession();
              }
            }}
            style={{
              padding: '4px 10px',
              background: 'var(--bg3)',
              border: '1px solid var(--border)',
              borderRadius: '6px',
              fontSize: '11px',
              color: 'var(--text2)',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              transition: 'all 0.15s',
            }}
            onMouseOver={(e) => e.currentTarget.style.background = 'var(--bg4)'}
            onMouseOut={(e) => e.currentTarget.style.background = 'var(--bg3)'}
            title="清空会话 (Ctrl+Shift+C)"
          >
            <span>🗑️</span>
            <span>清空</span>
          </button>

          {/* 新建会话按钮 */}
          <button
            onClick={() => {
              onNewSession?.();
            }}
            style={{
              padding: '4px 10px',
              background: 'var(--bg3)',
              border: '1px solid var(--border)',
              borderRadius: '6px',
              fontSize: '11px',
              color: 'var(--text2)',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              transition: 'all 0.15s',
            }}
            onMouseOver={(e) => e.currentTarget.style.background = 'var(--bg4)'}
            onMouseOut={(e) => e.currentTarget.style.background = 'var(--bg3)'}
            title="新建会话 (Ctrl+Shift+N)"
          >
            <span>➕</span>
            <span>新建</span>
          </button>

        </div>
      )}

      {/* Disconnect button - only shown when connected */}
      {connectionStatus === 'connected' && (
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
            transition: 'opacity 0.15s',
          }}
          onMouseOver={(e) => (e.currentTarget.style.opacity = '0.85')}
          onMouseOut={(e) => (e.currentTarget.style.opacity = '1')}
        >
          断开
        </button>
      )}
    </header>
  );
};
