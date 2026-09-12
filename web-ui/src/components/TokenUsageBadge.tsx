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
      className="tok-wrap"
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <div
        className={`tok-badge${hasRoleBreakdown ? ' clickable' : ''}`}
        title={`输入: ${input_tokens.toLocaleString()} / 输出: ${output_tokens.toLocaleString()} / 总计: ${total.toLocaleString()} tokens`}
      >
        <span>🔤</span>
        <span>{formatTokens(total)}</span>
      </div>
      {/* Tooltip with per-role breakdown */}
      {showTooltip && hasRoleBreakdown && (
        <div className="tok-tip">
          <div className="tok-tip-title">按角色用量</div>
          {Object.entries(role_usage!).map(([role, [inp, out]]) => (
            <div key={role} className="tok-row">
              <span className="tok-role">{role}</span>
              <span>入 {formatTokens(inp)}</span>
              <span>出 {formatTokens(out)}</span>
            </div>
          ))}
          <div className="tok-total">
            <span style={{ fontWeight: 600 }}>总计</span>
            <span>入 {formatTokens(input_tokens)}</span>
            <span>出 {formatTokens(output_tokens)}</span>
          </div>
        </div>
      )}
    </div>
  );
};
