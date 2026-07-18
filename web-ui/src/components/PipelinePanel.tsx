import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import type { PipelineDef, PipelineInfo, StageDef } from '../types/agent';

interface PipelinePanelProps {
  isConnected: boolean;
  listPipelinesWs: () => void;
  getPipelineWs: (name: string) => void;
  savePipelineWs: (pipeline: PipelineDef) => void;
  deletePipelineWs: (name: string) => void;
}

const emptyStage = (): StageDef => ({
  id: `stage_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
  name: '',
  role: undefined,
  model: undefined,
  tools: 'all' as const,
  context: 'shared' as const,
  system_prompt: undefined,
  initial_message: undefined,
  inputs: [],
  artifact: undefined,
  on_pass: 'done',
  on_fail: 'done',
  max_retries: undefined,
});

const emptyPipeline = (): PipelineDef => ({
  name: '',
  description: '',
  stages: [],
});

export const PipelinePanel: React.FC<PipelinePanelProps> = ({
  isConnected,
  listPipelinesWs,
  getPipelineWs,
  savePipelineWs,
  deletePipelineWs,
}) => {
  const { pipelines } = useAgentStore();
  const [editing, setEditing] = useState<PipelineDef | null>(null);
  const [selectedStageIdx, setSelectedStageIdx] = useState<number | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [newName, setNewName] = useState('');

  useEffect(() => {
    if (isConnected) listPipelinesWs();
  }, [isConnected, listPipelinesWs]);

  const handleSelect = (name: string) => {
    getPipelineWs(name);
    // After receiving pipeline_loaded, the store's editingPipeline will be set
    setTimeout(() => {
      const st = useAgentStore.getState();
      if (st.editingPipeline) {
        setEditing({ ...st.editingPipeline });
        setSelectedStageIdx(null);
      }
    }, 100);
  };

  const handleNew = () => {
    setEditing(emptyPipeline());
    setShowNew(true);
    setNewName('');
    setSelectedStageIdx(null);
  };

  const handleCreate = () => {
    if (!editing || !newName.trim()) return;
    const def = { ...editing, name: newName.trim() };
    setEditing(def);
    setShowNew(false);
  };

  const handleSave = () => {
    if (!editing) return;
    savePipelineWs(editing);
  };

  const handleDelete = (name: string) => {
    if (!confirm(`Delete pipeline "${name}"?`)) return;
    deletePipelineWs(name);
    setEditing(null);
    setSelectedStageIdx(null);
  };

  const handleAddStage = () => {
    if (!editing) return;
    const stage = emptyStage();
    setEditing({ ...editing, stages: [...editing.stages, stage] });
    setSelectedStageIdx(editing.stages.length);
  };

  const handleRemoveStage = (idx: number) => {
    if (!editing) return;
    const stages = editing.stages.filter((_, i) => i !== idx);
    setEditing({ ...editing, stages });
    if (selectedStageIdx === idx) setSelectedStageIdx(null);
    else if (selectedStageIdx !== null && selectedStageIdx > idx) setSelectedStageIdx(selectedStageIdx - 1);
  };

  const handleStageChange = (idx: number, updates: Partial<StageDef>) => {
    if (!editing) return;
    const stages = editing.stages.map((s, i) => (i === idx ? { ...s, ...updates } : s));
    setEditing({ ...editing, stages });
  };

  const styles = {
    container: { display: 'flex', height: '100%', background: 'var(--bg)', color: 'var(--text)' } as React.CSSProperties,
    sidebar: { width: '220px', borderRight: '1px solid var(--border)', overflow: 'auto', padding: '12px', flexShrink: 0 } as React.CSSProperties,
    main: { flex: 1, overflow: 'auto', padding: '16px' } as React.CSSProperties,
    btn: {
      padding: '6px 12px', borderRadius: '6px', border: '1px solid var(--border)',
      background: 'var(--bg2)', color: 'var(--text)', cursor: 'pointer', fontSize: '12px',
    } as React.CSSProperties,
    btnPrimary: {
      padding: '6px 12px', borderRadius: '6px', border: 'none',
      background: 'var(--accent)', color: '#fff', cursor: 'pointer', fontSize: '12px',
    } as React.CSSProperties,
    btnDanger: {
      padding: '6px 12px', borderRadius: '6px', border: 'none',
      background: 'var(--red)', color: '#fff', cursor: 'pointer', fontSize: '12px',
    } as React.CSSProperties,
    input: {
      width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid var(--border)',
      background: 'var(--bg)', color: 'var(--text)', fontSize: '12px', boxSizing: 'border-box' as any,
    },
    textarea: {
      width: '100%', padding: '6px 8px', borderRadius: '4px', border: '1px solid var(--border)',
      background: 'var(--bg)', color: 'var(--text)', fontSize: '11px', minHeight: '60px',
      fontFamily: 'monospace', boxSizing: 'border-box' as any, resize: 'vertical' as any,
    },
    label: { fontSize: '11px', fontWeight: 600, color: 'var(--text3)', marginBottom: '4px', display: 'block' } as React.CSSProperties,
    field: { marginBottom: '10px' } as React.CSSProperties,
    listItem: {
      padding: '8px 10px', borderRadius: '6px', cursor: 'pointer', marginBottom: '4px',
      fontSize: '12px', display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    } as React.CSSProperties,
    stageCard: {
      border: '1px solid var(--border)', borderRadius: '8px', padding: '12px', marginBottom: '8px',
      background: 'var(--bg2)',
    } as React.CSSProperties,
  };

  if (!isConnected) {
    return <div style={{ padding: '24px', color: 'var(--text3)' }}>请先连接项目以管理 Pipeline。</div>;
  }

  return (
    <div style={styles.container}>
      {/* Sidebar: pipeline list */}
      <div style={styles.sidebar}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '10px' }}>
          <span style={{ fontSize: '13px', fontWeight: 600 }}>Pipelines</span>
          <button style={styles.btnPrimary} onClick={handleNew}>+ 新建</button>
        </div>

        {pipelines.map((p: PipelineInfo) => (
          <div
            key={p.name}
            style={{
              ...styles.listItem,
              background: editing?.name === p.name ? 'var(--accent-glow)' : 'transparent',
            }}
            onClick={() => handleSelect(p.name)}
          >
            <div>
              <div style={{ fontWeight: 600 }}>{p.name}</div>
              <div style={{ fontSize: '10px', color: 'var(--text3)' }}>{p.description}</div>
              <div style={{ fontSize: '10px', color: 'var(--text3)' }}>{p.stage_count} stages</div>
            </div>
          </div>
        ))}

        {pipelines.length === 0 && (
          <div style={{ fontSize: '12px', color: 'var(--text3)', textAlign: 'center', padding: '12px' }}>
            暂无 pipeline。点击"新建"或使用内置 default。
          </div>
        )}
      </div>

      {/* Main: editor */}
      <div style={styles.main}>
        {showNew && (
          <div style={{ ...styles.stageCard, marginBottom: '16px' }}>
            <div style={styles.field}>
              <label style={styles.label}>Pipeline 名称</label>
              <input
                style={styles.input}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="例如: code-review"
              />
            </div>
            <div style={{ display: 'flex', gap: '8px' }}>
              <button style={styles.btnPrimary} onClick={handleCreate}>创建</button>
              <button style={styles.btn} onClick={() => { setShowNew(false); setEditing(null); }}>取消</button>
            </div>
          </div>
        )}

        {editing && !showNew && (
          <>
            {/* Pipeline header */}
            <div style={{ marginBottom: '16px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '8px' }}>
                <h3 style={{ margin: 0, fontSize: '16px' }}>{editing.name || '(未命名)'}</h3>
                <button style={styles.btnDanger} onClick={() => handleDelete(editing.name)}>删除</button>
              </div>
              <div style={styles.field}>
                <label style={styles.label}>描述</label>
                <input
                  style={styles.input}
                  value={editing.description}
                  onChange={(e) => setEditing({ ...editing, description: e.target.value })}
                />
              </div>
            </div>

            {/* Stages */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '10px' }}>
              <span style={{ fontSize: '13px', fontWeight: 600 }}>阶段 ({editing.stages.length})</span>
              <button style={styles.btnPrimary} onClick={handleAddStage}>+ 添加阶段</button>
            </div>

            {editing.stages.map((stage, idx) => (
              <div key={stage.id} style={{
                ...styles.stageCard,
                borderColor: selectedStageIdx === idx ? 'var(--accent)' : 'var(--border)',
              }} onClick={() => setSelectedStageIdx(idx)}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
                  <span style={{ fontWeight: 600, fontSize: '13px' }}>
                    #{idx + 1} {stage.name || stage.id || '(新阶段)'}
                  </span>
                  <div style={{ display: 'flex', gap: '6px' }}>
                    <button style={{ ...styles.btn, fontSize: '11px', padding: '2px 6px' }}
                      onClick={(e) => { e.stopPropagation(); handleRemoveStage(idx); }}>✕</button>
                  </div>
                </div>

                {selectedStageIdx === idx && (
                  <div>
                    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
                      <div style={styles.field}>
                        <label style={styles.label}>ID (唯一标识)</label>
                        <input style={styles.input} value={stage.id}
                          onChange={(e) => handleStageChange(idx, { id: e.target.value })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>显示名称</label>
                        <input style={styles.input} value={stage.name}
                          onChange={(e) => handleStageChange(idx, { name: e.target.value })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>角色 (role)</label>
                        <input style={styles.input} value={stage.role || ''}
                          placeholder="planner / executor / checker"
                          onChange={(e) => handleStageChange(idx, { role: e.target.value || undefined })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>模型 (model)</label>
                        <input style={styles.input} value={stage.model || ''}
                          placeholder="留空则使用 role 默认"
                          onChange={(e) => handleStageChange(idx, { model: e.target.value || undefined })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>工具权限</label>
                        <select style={styles.input} value={stage.tools}
                          onChange={(e) => handleStageChange(idx, { tools: e.target.value as any })}>
                          <option value="all">全部工具</option>
                          <option value="read_only">只读</option>
                        </select>
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>上下文</label>
                        <select style={styles.input} value={stage.context}
                          onChange={(e) => handleStageChange(idx, { context: e.target.value as any })}>
                          <option value="shared">共享 (shared)</option>
                          <option value="isolated">隔离 (isolated)</option>
                        </select>
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>成功跳转到</label>
                        <input style={styles.input} value={stage.on_pass}
                          placeholder="stage_id 或 done"
                          onChange={(e) => handleStageChange(idx, { on_pass: e.target.value })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>失败跳转到</label>
                        <input style={styles.input} value={stage.on_fail}
                          placeholder="stage_id 或 done"
                          onChange={(e) => handleStageChange(idx, { on_fail: e.target.value })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>最大重试</label>
                        <input style={styles.input} type="number" min="0"
                          value={stage.max_retries ?? ''}
                          onChange={(e) => handleStageChange(idx, { max_retries: e.target.value ? Number(e.target.value) : undefined })} />
                      </div>
                      <div style={styles.field}>
                        <label style={styles.label}>输出 Artifact</label>
                        <input style={styles.input} value={stage.artifact || ''}
                          placeholder=".agent/artifacts/plan.md"
                          onChange={(e) => handleStageChange(idx, { artifact: e.target.value || undefined })} />
                      </div>
                    </div>

                    <div style={styles.field}>
                      <label style={styles.label}>输入 Artifacts (逗号分隔)</label>
                      <input style={styles.input} value={(stage.inputs || []).join(', ')}
                        placeholder="plan.md, result.md"
                        onChange={(e) => handleStageChange(idx, {
                          inputs: e.target.value.split(',').map(s => s.trim()).filter(Boolean),
                        })} />
                    </div>

                    <div style={styles.field}>
                      <label style={styles.label}>自定义 System Prompt (可选)</label>
                      <textarea style={styles.textarea} value={stage.system_prompt || ''}
                        placeholder="留空则使用 role 的默认 system prompt"
                        onChange={(e) => handleStageChange(idx, { system_prompt: e.target.value || undefined })} />
                    </div>

                    <div style={styles.field}>
                      <label style={styles.label}>
                        阶段启动消息 (支持 {'{{task}}'}, {'{{inputs.xxx}}'}, {'{{stage.previous}}'})
                      </label>
                      <textarea style={{ ...styles.textarea, minHeight: '100px' }}
                        value={stage.initial_message || ''}
                        onChange={(e) => handleStageChange(idx, { initial_message: e.target.value || undefined })} />
                    </div>
                  </div>
                )}
              </div>
            ))}

            {editing.stages.length === 0 && (
              <div style={{ fontSize: '12px', color: 'var(--text3)', textAlign: 'center', padding: '24px' }}>
                暂无阶段。点击"+ 添加阶段"开始配置。
              </div>
            )}

            {/* Save button */}
            <div style={{ marginTop: '16px', paddingTop: '12px', borderTop: '1px solid var(--border)' }}>
              <button style={styles.btnPrimary} onClick={handleSave}>💾 保存 Pipeline</button>
            </div>
          </>
        )}

        {!editing && !showNew && (
          <div style={{ fontSize: '14px', color: 'var(--text3)', textAlign: 'center', padding: '48px' }}>
            选择左侧一个 pipeline 进行编辑，或点击"新建"创建新的 pipeline。
          </div>
        )}
      </div>
    </div>
  );
};
