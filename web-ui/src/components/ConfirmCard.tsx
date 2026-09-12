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

  const submitAnswer = () => {
    if (!answer.trim()) return;
    onAnswer?.(confirmation.id, answer);
    setAnswer('');
  };

  if (confirmation.type === 'ask_user') {
    return (
      <div className="fade-in confirm-card info">
        <div className="confirm-head">
          <span className="confirm-icon">❓</span>
          <div className="confirm-body">
            <p className="confirm-title">Agent 提问</p>
            <p className="confirm-text">{confirmation.action}</p>
          </div>
        </div>
        <div className="row" style={{ gap: 8 }}>
          <input
            className="field"
            style={{ flex: 1 }}
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') submitAnswer(); }}
            placeholder="输入回答…"
          />
          <button className="confirm-btn go" onClick={submitAnswer} disabled={!answer.trim()}>
            发送
          </button>
        </div>
      </div>
    );
  }

  if (confirmation.type === 'review_plan') {
    return (
      <div className="fade-in confirm-card warn">
        <div className="confirm-head" style={{ alignItems: 'center' }}>
          <span className="confirm-icon">📋</span>
          <p className="confirm-title" style={{ marginBottom: 0 }}>执行计划审阅</p>
        </div>
        {confirmation.details && (
          <pre className="confirm-details">{confirmation.details}</pre>
        )}
        <textarea
          className="field"
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          placeholder="可选：对计划的反馈意见…"
          rows={2}
          style={{ resize: 'vertical', marginBottom: 10 }}
        />
        <div className="confirm-actions">
          <button className="confirm-btn ok" onClick={() => onReviewPlan?.(confirmation.id, true, feedback || undefined)}>
            ✓ 批准计划
          </button>
          <button className="confirm-btn no" onClick={() => onReviewPlan?.(confirmation.id, false, feedback || undefined)}>
            ✗ 拒绝计划
          </button>
        </div>
      </div>
    );
  }

  // Default: confirm tool call
  return (
    <div className="fade-in confirm-card warn">
      <div className="confirm-head">
        <span className="confirm-icon">⚡</span>
        <div className="confirm-body">
          <p className="confirm-title">需要确认</p>
          <p className="confirm-text">{confirmation.action}</p>
          {confirmation.details && (
            <pre className="confirm-details tight">{confirmation.details}</pre>
          )}
        </div>
      </div>
      <div className="confirm-actions">
        <button className="confirm-btn ok" onClick={() => onConfirm(confirmation.id, true)}>✓ 允许</button>
        <button className="confirm-btn no" onClick={() => onConfirm(confirmation.id, false)}>✗ 拒绝</button>
      </div>
    </div>
  );
};
