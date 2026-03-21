import React, { useState } from 'react';
import { PendingConfirmation } from '../stores/agentStore';

interface Props {
  confirmation: PendingConfirmation;
  onConfirm: (id: string, approved: boolean) => void;
  onAnswer?: (id: string, answer: string) => void;
  onReviewPlan?: (id: string, approved: boolean, feedback?: string) => void;
}

export const ConfirmCard: React.FC<Props> = ({ confirmation, onConfirm, onAnswer, onReviewPlan }) => {
  const [answer, setAnswer] = useState('');
  const [feedback, setFeedback] = useState('');

  if (confirmation.type === 'ask_user') {
    return (
      <div className="fade-in" style={{
        background: 'var(--surface)',
        border: '1px solid rgba(99,102,241,0.4)',
        borderRadius: 'var(--radius)',
        padding: '14px',
        marginBottom: '8px',
      }}>
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: '8px', marginBottom: '10px' }}>
          <span style={{ fontSize: '16px' }}>❓</span>
          <div>
            <p style={{ fontSize: '13px', fontWeight: '600', color: 'var(--accent)', marginBottom: '2px' }}>Agent 提问</p>
            <p style={{ fontSize: '13px', color: 'var(--text)', lineHeight: '1.5' }}>{confirmation.action}</p>
          </div>
        </div>
        <div style={{ display: 'flex', gap: '8px' }}>
          <input
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && answer.trim()) { onAnswer?.(confirmation.id, answer); setAnswer(''); } }}
            placeholder="输入回答…"
            style={{
              flex: 1, padding: '7px 10px', background: 'var(--bg)',
              border: '1px solid var(--border)', borderRadius: '7px',
              color: 'var(--text)', outline: 'none',
            }}
          />
          <button
            onClick={() => { if (answer.trim()) { onAnswer?.(confirmation.id, answer); setAnswer(''); } }}
            disabled={!answer.trim()}
            style={{
              padding: '7px 14px', background: 'var(--accent)', color: '#fff',
              borderRadius: '7px', fontWeight: '500', fontSize: '13px',
              opacity: answer.trim() ? 1 : 0.4,
            }}
          >
            发送
          </button>
        </div>
      </div>
    );
  }

  if (confirmation.type === 'review_plan') {
    return (
      <div className="fade-in" style={{
        background: 'var(--surface)',
        border: '1px solid rgba(245,158,11,0.4)',
        borderRadius: 'var(--radius)',
        padding: '14px',
        marginBottom: '8px',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '10px' }}>
          <span style={{ fontSize: '16px' }}>📋</span>
          <p style={{ fontSize: '13px', fontWeight: '600', color: 'var(--yellow)' }}>执行计划审阅</p>
        </div>
        {confirmation.details && (
          <pre style={{
            background: 'var(--bg)', border: '1px solid var(--border)',
            borderRadius: '6px', padding: '10px', fontSize: '12px',
            overflowX: 'auto', maxHeight: '200px', overflowY: 'auto',
            color: 'var(--text)', margin: '0 0 10px', whiteSpace: 'pre-wrap',
          }}>
            {confirmation.details}
          </pre>
        )}
        <textarea
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          placeholder="可选：对计划的反馈意见…"
          rows={2}
          style={{
            width: '100%', padding: '7px 10px', background: 'var(--bg)',
            border: '1px solid var(--border)', borderRadius: '7px',
            color: 'var(--text)', resize: 'vertical', outline: 'none', marginBottom: '10px',
          }}
        />
        <div style={{ display: 'flex', gap: '8px' }}>
          <button
            onClick={() => onReviewPlan?.(confirmation.id, true, feedback || undefined)}
            style={{
              flex: 1, padding: '7px', background: 'var(--green-dim)', color: 'var(--green)',
              borderRadius: '7px', fontWeight: '500', border: '1px solid rgba(16,185,129,0.3)',
            }}
          >✓ 批准计划</button>
          <button
            onClick={() => onReviewPlan?.(confirmation.id, false, feedback || undefined)}
            style={{
              flex: 1, padding: '7px', background: 'var(--red-dim)', color: 'var(--red)',
              borderRadius: '7px', fontWeight: '500', border: '1px solid rgba(239,68,68,0.3)',
            }}
          >✗ 拒绝计划</button>
        </div>
      </div>
    );
  }

  // Default: confirm tool call
  return (
    <div className="fade-in" style={{
      background: 'var(--surface)',
      border: '1px solid rgba(245,158,11,0.4)',
      borderRadius: 'var(--radius)',
      padding: '14px',
      marginBottom: '8px',
    }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: '8px', marginBottom: '10px' }}>
        <span style={{ fontSize: '16px' }}>⚡</span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <p style={{ fontSize: '13px', fontWeight: '600', color: 'var(--yellow)', marginBottom: '4px' }}>需要确认</p>
          <p style={{ fontSize: '13px', color: 'var(--text)' }}>{confirmation.action}</p>
          {confirmation.details && (
            <pre style={{
              marginTop: '8px', background: 'var(--bg)',
              border: '1px solid var(--border)', borderRadius: '6px',
              padding: '8px', fontSize: '12px',
              overflowX: 'auto', maxHeight: '150px', overflowY: 'auto',
              color: 'var(--text2)', whiteSpace: 'pre-wrap',
            }}>
              {confirmation.details}
            </pre>
          )}
        </div>
      </div>
      <div style={{ display: 'flex', gap: '8px' }}>
        <button
          onClick={() => onConfirm(confirmation.id, true)}
          style={{
            flex: 1, padding: '7px', background: 'var(--green-dim)', color: 'var(--green)',
            borderRadius: '7px', fontWeight: '500', border: '1px solid rgba(16,185,129,0.3)',
          }}
        >✓ 允许</button>
        <button
          onClick={() => onConfirm(confirmation.id, false)}
          style={{
            flex: 1, padding: '7px', background: 'var(--red-dim)', color: 'var(--red)',
            borderRadius: '7px', fontWeight: '500', border: '1px solid rgba(239,68,68,0.3)',
          }}
        >✗ 拒绝</button>
      </div>
    </div>
  );
};
