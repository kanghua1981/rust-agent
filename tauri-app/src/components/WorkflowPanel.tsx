import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import type { WorkflowDef, WorkflowStage } from '../types/agent';

interface WorkflowPanelProps {
  isConnected: boolean;
  listWorkflowsWs: () => void;
  saveWorkflowWs: (wf: any) => void;
  deleteWorkflowWs: (id: string) => void;
  runWorkflowWs: (workflowId: string, task: string) => void;
}

const emptyStage = (order: number): WorkflowStage => ({
  id: `stage_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
  workflowId: '',
  stageOrder: order,
  stageGroup: 'default',
  inputTemplate: '{{task}}',
  outputKey: undefined,
  condition: 'always',
  timeoutSecs: 300,
  retryCount: 0,
  autoApprove: false,
  serverUrl: '',
  workdir: undefined,
  model: undefined,
  agentMode: 'auto',
});

const emptyWorkflow = (): WorkflowDef => ({
  id: `wf_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
  name: '',
  description: '',
  enabled: true,
  defaultTimeout: 600,
  stages: [],
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
});

export const WorkflowPanel: React.FC<WorkflowPanelProps> = ({
  isConnected,
  listWorkflowsWs,
  saveWorkflowWs,
  deleteWorkflowWs,
  runWorkflowWs,
}) => {
  const { workflows, projects, activeRun } = useAgentStore();
  const [editing, setEditing] = useState<WorkflowDef | null>(null);
  const [showEditor, setShowEditor] = useState(false);
  const [runTask, setRunTask] = useState('');
  const [showRunInput, setShowRunInput] = useState<string | null>(null); // wf id or null

  // Fetch workflows on mount / connect
  useEffect(() => {
    if (isConnected) listWorkflowsWs();
  }, [isConnected, listWorkflowsWs]);

  const handleNew = () => {
    setEditing(emptyWorkflow());
    setShowEditor(true);
  };

  const handleEdit = (wf: WorkflowDef) => {
    setEditing({ ...wf, stages: wf.stages.map(s => ({ ...s })) });
    setShowEditor(true);
  };

  const handleDelete = (id: string) => {
    if (confirm('确定删除此工作流？')) {
      deleteWorkflowWs(id);
    }
  };

  const handleSave = () => {
    if (!editing) return;
    if (!editing.name.trim()) return;
    saveWorkflowWs({
      ...editing,
      updatedAt: new Date().toISOString(),
    });
    setShowEditor(false);
    setEditing(null);
  };

  const handleCancel = () => {
    setShowEditor(false);
    setEditing(null);
  };

  const addStage = () => {
    if (!editing) return;
    const nextOrder = editing.stages.length;
    setEditing({
      ...editing,
      stages: [...editing.stages, emptyStage(nextOrder)],
    });
  };

  const removeStage = (stageId: string) => {
    if (!editing) return;
    setEditing({
      ...editing,
      stages: editing.stages
        .filter(s => s.id !== stageId)
        .map((s, i) => ({ ...s, stageOrder: i })),
    });
  };

  const updateStage = (stageId: string, patch: Partial<WorkflowStage>) => {
    if (!editing) return;
    setEditing({
      ...editing,
      stages: editing.stages.map(s =>
        s.id === stageId ? { ...s, ...patch } : s
      ),
    });
  };

  const theme = {
    bg: 'var(--bg, #1a1a2e)',
    cardBg: 'var(--card-bg, #16213e)',
    border: 'var(--border, #2a2a4a)',
    text: 'var(--text, #e0e0e0)',
    dim: 'var(--dim, #888)',
    accent: 'var(--accent, #7c3aed)',
    green: 'var(--green, #22c55e)',
    red: 'var(--red, #ef4444)',
    inputBg: 'var(--bg3, #1a1d2a)',
  };

  const inputStyle: React.CSSProperties = {
    padding: '6px 10px',
    background: theme.inputBg,
    border: `1px solid ${theme.border}`,
    borderRadius: '4px',
    color: theme.text,
    fontSize: '13px',
    width: '100%',
    boxSizing: 'border-box',
  };

  const btnStyle: React.CSSProperties = {
    padding: '6px 14px',
    background: theme.accent,
    color: '#fff',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '12px',
  };

  const dimBtnStyle: React.CSSProperties = {
    ...btnStyle,
    background: 'transparent',
    border: `1px solid ${theme.border}`,
    color: theme.dim,
  };

  const dangerBtnStyle: React.CSSProperties = {
    ...btnStyle,
    background: 'transparent',
    border: `1px solid ${theme.red}`,
    color: theme.red,
  };

  // ── Render ─────────────────────────────────────────────────────────
  return (
    <div style={{ padding: '16px', height: '100%', overflowY: 'auto', color: theme.text }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <h3 style={{ margin: 0 }}>⚙️ 工作流</h3>
        <button style={btnStyle} onClick={handleNew} disabled={!isConnected}>
          + 新建工作流
        </button>
      </div>

      {!showEditor && (
        <div>
          {workflows.length === 0 && (
            <p style={{ color: theme.dim, fontSize: 13 }}>暂无工作流。点击「新建工作流」创建。</p>
          )}
          {workflows.map(wf => (
            <div key={wf.id} style={{
              background: theme.cardBg, border: `1px solid ${theme.border}`,
              borderRadius: 6, padding: '10px 14px', marginBottom: 8,
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>{wf.name}</div>
                <div style={{ fontSize: 12, color: theme.dim }}>
                  {wf.description || '无描述'} · {wf.stages.length} 阶段 · {wf.enabled ? '✅ 启用' : '⏸ 禁用'}
                </div>
              </div>
              <div style={{ display: 'flex', gap: 6 }}>
                <button style={dimBtnStyle} onClick={() => setShowRunInput(showRunInput === wf.id ? null : wf.id)}>▶ 运行</button>
                <button style={dimBtnStyle} onClick={() => handleEdit(wf)}>编辑</button>
                <button style={dangerBtnStyle} onClick={() => handleDelete(wf.id)}>删除</button>
              </div>
            </div>
          ))}

          {/* Run input */}
          {showRunInput && (
            <div style={{
              background: theme.cardBg, border: `1px solid ${theme.accent}`,
              borderRadius: 6, padding: '10px 14px', marginBottom: 8,
            }}>
              <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>▶ 运行工作流</div>
              <textarea
                style={{ ...inputStyle, minHeight: 50, marginBottom: 8 } as any}
                value={runTask}
                onChange={e => setRunTask(e.target.value)}
                placeholder="输入任务描述，例如：重构 auth 模块"
                rows={2}
              />
              <div style={{ display: 'flex', gap: 6 }}>
                <button style={btnStyle}
                  onClick={() => {
                    if (runTask.trim()) {
                      runWorkflowWs(showRunInput, runTask.trim());
                      setRunTask('');
                      setShowRunInput(null);
                    }
                  }}
                  disabled={!runTask.trim()}
                >🚀 启动</button>
                <button style={dimBtnStyle} onClick={() => { setShowRunInput(null); setRunTask(''); }}>取消</button>
              </div>
            </div>
          )}

          {/* Run progress */}
          {activeRun && (
            <div style={{
              background: theme.cardBg, border: `1px solid ${activeRun.status === 'success' ? theme.green : activeRun.status === 'error' || activeRun.status === 'failed' ? theme.red : theme.accent}`,
              borderRadius: 6, padding: '10px 14px', marginBottom: 8,
            }}>
              <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 4 }}>
                {(activeRun.status === 'running' || activeRun.status === 'success') ? '🔄' : activeRun.status === 'error' || activeRun.status === 'failed' ? '❌' : '📋'} {activeRun.workflowName} — {activeRun.status}
              </div>
              {activeRun.errorMessage && (
                <div style={{ color: theme.red, fontSize: 12, marginBottom: 4 }}>{activeRun.errorMessage}</div>
              )}
              {activeRun.stageResults && activeRun.stageResults.length > 0 && (
                <div>
                  {activeRun.stageResults.map((sr: any, i: number) => (
                    <div key={i} style={{
                      fontSize: 12, padding: '4px 0', borderTop: `1px solid ${theme.border}`,
                      display: 'flex', justifyContent: 'space-between',
                    }}>
                      <span>#{i + 1} {sr.presetName || '?'} — {sr.status}</span>
                      <span style={{ color: theme.dim }}>
                        {sr.outputSummary ? sr.outputSummary.slice(0, 60) + '…' : ''}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {showEditor && editing && (
        <div style={{ background: theme.cardBg, border: `1px solid ${theme.border}`, borderRadius: 8, padding: 16 }}>
          <h4 style={{ margin: '0 0 12px' }}>{editing.id.includes('wf_') ? '新建工作流' : '编辑工作流'}</h4>

          {/* Name */}
          <div style={{ marginBottom: 10 }}>
            <label style={{ fontSize: 12, color: theme.dim, display: 'block', marginBottom: 4 }}>名称 *</label>
            <input
              style={inputStyle}
              value={editing.name}
              onChange={e => setEditing({ ...editing, name: e.target.value })}
              placeholder="工作流名称"
            />
          </div>

          {/* Description */}
          <div style={{ marginBottom: 10 }}>
            <label style={{ fontSize: 12, color: theme.dim, display: 'block', marginBottom: 4 }}>描述</label>
            <input
              style={inputStyle}
              value={editing.description}
              onChange={e => setEditing({ ...editing, description: e.target.value })}
              placeholder="可选描述"
            />
          </div>

          <div style={{ display: 'flex', gap: 12, marginBottom: 14 }}>
            <label style={{ fontSize: 12, display: 'flex', alignItems: 'center', gap: 4 }}>
              <input type="checkbox" checked={editing.enabled}
                onChange={e => setEditing({ ...editing, enabled: e.target.checked })} />
              启用
            </label>
            <div>
              <label style={{ fontSize: 12, color: theme.dim, marginRight: 6 }}>超时(秒)</label>
              <input
                type="number"
                style={{ ...inputStyle, width: 70 }}
                value={editing.defaultTimeout}
                onChange={e => setEditing({ ...editing, defaultTimeout: parseInt(e.target.value) || 600 })}
              />
            </div>
          </div>

          {/* Stages */}
          <div style={{ marginBottom: 14 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
              <span style={{ fontSize: 13, fontWeight: 600 }}>阶段 ({editing.stages.length})</span>
              <button style={dimBtnStyle} onClick={addStage}>+ 添加阶段</button>
            </div>

            {editing.stages.length === 0 && (
              <p style={{ fontSize: 12, color: theme.dim }}>尚未添加阶段。每个阶段配置目标服务器连接和输入模板。</p>
            )}

            {editing.stages.map((stage, idx) => (
              <div key={stage.id} style={{
                background: theme.bg, border: `1px solid ${theme.border}`,
                borderRadius: 4, padding: '8px 10px', marginBottom: 6,
              }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: theme.accent }}>#{idx + 1}</span>
                  <button style={{ ...dangerBtnStyle, padding: '2px 8px', fontSize: 11 }}
                    onClick={() => removeStage(stage.id)}>✕</button>
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
                  {/* Server URL */}
                  <div style={{ gridColumn: '1 / -1' }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 2 }}>
                      <label style={{ fontSize: 11, color: theme.dim }}>目标服务器 URL *</label>
                      <select
                        style={{ fontSize: 11, padding: '1px 4px', background: theme.inputBg, border: `1px solid ${theme.border}`, borderRadius: 3, color: theme.dim }}
                        value=""
                        onChange={e => {
                          const pid = e.target.value;
                          if (!pid) return;
                          const p = Object.values(projects).find(x => x.id === pid);
                          if (p) {
                            updateStage(stage.id, {
                              serverUrl: p.serverUrl,
                              workdir: p.workdir,
                              agentMode: p.agentMode || 'auto',
                            });
                          }
                        }}
                      >
                        <option value="">从项目快速填充…</option>
                        {Object.values(projects).map(p => (
                          <option key={p.id} value={p.id}>{p.label} ({p.serverUrl})</option>
                        ))}
                      </select>
                    </div>
                    <input style={inputStyle} value={stage.serverUrl || ''}
                      onChange={e => updateStage(stage.id, { serverUrl: e.target.value })}
                      placeholder="ws://host:9527" />
                  </div>

                  {/* Workdir */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>工作目录</label>
                    <input style={inputStyle} value={stage.workdir || ''}
                      onChange={e => updateStage(stage.id, { workdir: e.target.value || undefined })}
                      placeholder="/path/to/project" />
                  </div>

                  {/* Model */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>模型</label>
                    <input style={inputStyle} value={stage.model || ''}
                      onChange={e => updateStage(stage.id, { model: e.target.value || undefined })}
                      placeholder="gpt-4 / claude-3" />
                  </div>

                  {/* Agent mode */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>执行模式</label>
                    <select
                      style={{ ...inputStyle, fontSize: 12 }}
                      value={stage.agentMode || 'auto'}
                      onChange={e => updateStage(stage.id, { agentMode: e.target.value })}
                    >
                      <option value="auto">auto</option>
                      <option value="simple">simple</option>
                      <option value="plan">plan</option>
                      <option value="pipeline">pipeline</option>
                    </select>
                  </div>

                  {/* Stage group */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>分组</label>
                    <input style={inputStyle} value={stage.stageGroup}
                      onChange={e => updateStage(stage.id, { stageGroup: e.target.value })} />
                  </div>

                  {/* Input template */}
                  <div style={{ gridColumn: '1 / -1' }}>
                    <label style={{ fontSize: 11, color: theme.dim }}>输入模板 (支持 {'{{task}}'} {'{{stage.s1.output}}'})</label>
                    <textarea
                      style={{ ...inputStyle, minHeight: 40, resize: 'vertical' } as any}
                      value={stage.inputTemplate}
                      onChange={e => updateStage(stage.id, { inputTemplate: e.target.value })}
                      rows={2}
                    />
                  </div>

                  {/* Output key */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>输出键 (供后续阶段引用)</label>
                    <input style={inputStyle} value={stage.outputKey || ''}
                      onChange={e => updateStage(stage.id, { outputKey: e.target.value || undefined })} />
                  </div>

                  {/* Condition */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>执行条件</label>
                    <select
                      style={{ ...inputStyle, fontSize: 12 }}
                      value={stage.condition}
                      onChange={e => updateStage(stage.id, { condition: e.target.value })}
                    >
                      <option value="always">always</option>
                      <option value="on_success">on_success</option>
                      <option value="on_failure">on_failure</option>
                    </select>
                  </div>

                  {/* Timeout + retries */}
                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>超时(秒)</label>
                    <input type="number" style={inputStyle} value={stage.timeoutSecs}
                      onChange={e => updateStage(stage.id, { timeoutSecs: parseInt(e.target.value) || 300 })} />
                  </div>

                  <div>
                    <label style={{ fontSize: 11, color: theme.dim }}>重试次数</label>
                    <input type="number" style={inputStyle} value={stage.retryCount}
                      onChange={e => updateStage(stage.id, { retryCount: parseInt(e.target.value) || 0 })} />
                  </div>

                  <div>
                    <label style={{ fontSize: 12, display: 'flex', alignItems: 'center', gap: 4, marginTop: 18 }}>
                      <input type="checkbox" checked={stage.autoApprove}
                        onChange={e => updateStage(stage.id, { autoApprove: e.target.checked })} />
                      自动批准
                    </label>
                  </div>
                </div>
              </div>
            ))}
          </div>

          {/* Actions */}
          <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
            <button style={dimBtnStyle} onClick={handleCancel}>取消</button>
            <button style={btnStyle} onClick={handleSave} disabled={!editing.name.trim()}>
              💾 保存
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
