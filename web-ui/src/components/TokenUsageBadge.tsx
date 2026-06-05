import React, { useState } from 'react';
import type { TokenUsage } from '../types/agent';

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

export const TokenUsageBadge: React.FC<{ tokenUsage: TokenUsage | null }> = ({ tokenUsage }) => {
  const [showTooltip, setShowTooltip] = useState(false);

  if (!tokenUsage || typeof tokenUsage.input_tokens !== 'number' || typeof tokenUsage.output_tokens !== 'number') {
    return null;
  }

  const { input_tokens, output_tokens, role_usage } = tokenUsage;
  const total = input_tokens + output_tokens;
  const hasRoleBreakdown = role_usage && Object.keys(role_usage).length > 0;

  return (
    <div
      style={{ position: 'relative', display: 'inline-flex' }}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <div
        style={{
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
          cursor: hasRoleBreakdown ? 'pointer' : 'default',
        }}
        title={`输入: ${input_tokens.toLocaleString()} / 输出: ${output_tokens.toLocaleString()} / 总计: ${total.toLocaleString()} tokens`}
      >
        <span>🔤</span>
        <span>{formatTokens(total)}</span>
      </div>
      {/* Tooltip with per-role breakdown */}
      {showTooltip && hasRoleBreakdown && (
        <div
          style={{
            position: 'absolute',
            top: '100%',
            right: 0,
            marginTop: '4px',
            padding: '8px 10px',
            background: 'var(--bg2)',
            border: '1px solid var(--border)',
            borderRadius: '8px',
            fontSize: '11px',
            color: 'var(--text2)',
            whiteSpace: 'nowrap',
            zIndex: 100,
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
          }}
        >
          <div style={{ fontWeight: '600', marginBottom: '4px', color: 'var(--text)' }}>
            按角色用量
          </div>
          {Object.entries(role_usage!).map(([role, [inp, out]]) => (
            <div key={role} style={{ display: 'flex', gap: '8px', justifyContent: 'space-between' }}>
              <span style={{ color: 'var(--accent)' }}>{role}</span>
              <span>入 {formatTokens(inp)}</span>
              <span>出 {formatTokens(out)}</span>
            </div>
          ))}
          <div style={{ borderTop: '1px solid var(--border)', marginTop: '4px', paddingTop: '4px', display: 'flex', gap: '8px', justifyContent: 'space-between' }}>
            <span style={{ fontWeight: '600' }}>总计</span>
            <span>入 {formatTokens(input_tokens)}</span>
            <span>出 {formatTokens(output_tokens)}</span>
          </div>
        </div>
      )}
    </div>
  );
};
