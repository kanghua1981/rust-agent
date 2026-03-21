import React, { useState } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { useWebSocket } from '../hooks/useWebSocket';
import { isDesktopApp, getEnvironmentInfo } from '../utils/environment';
import type { ConfigPreset } from '../types/agent';

export const SettingsPanel: React.FC = () => {
  const { 
    serverUrl, setServerUrl, workdir, setWorkdir, config, setConfig, reset, connectionStatus,
    presets, addPreset, updatePreset, deletePreset, applyPreset 
  } = useAgentStore();

  const { setSandbox, isConnected } = useWebSocket();
  
  const [activeTab, setActiveTab] = useState<'current' | 'presets'>('current');
  const [showNewPreset, setShowNewPreset] = useState(false);
  const [editingPreset, setEditingPreset] = useState<string | null>(null);
  const [sandboxEnabled, setSandboxEnabled] = useState(false);

  // Current config form state
  const [urlDraft, setUrlDraft] = useState(serverUrl);
  const [dirDraft, setDirDraft] = useState(workdir ?? '');
  const [modelDraft, setModelDraft] = useState(config.model ?? '');
  const [saved, setSaved] = useState(false);

  // New preset form state
  const [presetForm, setPresetForm] = useState<{
    name: string;
    serverUrl: string;
    workdir: string;
    model: string;
    autoApprove: boolean;
    agentMode: 'auto' | 'simple' | 'plan' | 'pipeline';
  }>({
    name: '',
    serverUrl: 'ws://localhost:9527',
    workdir: '',
    model: '',
    autoApprove: false,
    agentMode: 'auto',
  });

  const handleSandboxToggle = (enabled: boolean) => {
    setSandboxEnabled(enabled);
    if (isConnected) {
      setSandbox(enabled);
    }
  };

  const saveCurrentConfig = () => {
    setServerUrl(urlDraft.trim() || 'ws://localhost:9527');
    if (dirDraft.trim()) setWorkdir(dirDraft.trim());
    if (modelDraft.trim()) setConfig({ model: modelDraft.trim() });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const savePreset = () => {
    if (!presetForm.name.trim()) return;
    
    if (editingPreset) {
      updatePreset(editingPreset, presetForm);
      setEditingPreset(null);
    } else {
      addPreset(presetForm);
    }
    
    resetPresetForm();
    setShowNewPreset(false);
  };

  const resetPresetForm = () => {
    setPresetForm({
      name: '',
      serverUrl: 'ws://localhost:9527',
      workdir: '',
      model: '',
      autoApprove: false,
      agentMode: 'auto' as 'auto' | 'simple' | 'plan' | 'pipeline',
    });
  };

  const editPreset = (preset: ConfigPreset) => {
    setPresetForm({
      name: preset.name,
      serverUrl: preset.serverUrl,
      workdir: preset.workdir || '',
      model: preset.model || '',
      autoApprove: preset.autoApprove,
      agentMode: preset.agentMode,
    });
    setEditingPreset(preset.id);
    setShowNewPreset(true);
  };

  const handleApplyPreset = (presetId: string) => {
    applyPreset(presetId);
    // 应用预设后保持当前标签页为"预设"标签页
    setActiveTab('presets');
  };

  const cancelEdit = () => {
    setEditingPreset(null);
    setShowNewPreset(false);
    resetPresetForm();
  };

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '24px', maxWidth: '700px', margin: '0 auto', width: '100%' }}>
      <h2 style={{ fontSize: '18px', fontWeight: '600', color: 'var(--text)', marginBottom: '12px' }}>设置</h2>

      {/* Environment Info */}
      {isDesktopApp() && (
        <div style={{
          background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
          padding: '12px 16px',
          borderRadius: '10px',
          marginBottom: '20px',
          display: 'flex',
          alignItems: 'center',
          gap: '10px',
        }}>
          <span style={{ fontSize: '20px' }}>🖥️</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: '13px', fontWeight: '600', color: 'white' }}>
              桌面应用模式
            </div>
            <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.8)', marginTop: '2px' }}>
              请先启动 Agent 服务器，然后在下方配置连接地址
            </div>
          </div>
        </div>
      )}

      {/* Tab Navigation */}
      <div style={{ display: 'flex', gap: '4px', marginBottom: '20px', background: 'var(--bg2)', padding: '4px', borderRadius: '10px' }}>
        {[
          { key: 'current', label: '当前配置' },
          { key: 'presets', label: `配置预设 (${presets.length})` },
        ].map(tab => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key as any)}
            style={{
              flex: 1, padding: '8px 12px',
              background: activeTab === tab.key ? 'var(--surface)' : 'transparent',
              color: activeTab === tab.key ? 'var(--text)' : 'var(--text2)',
              borderRadius: '8px', fontSize: '13px', fontWeight: '500',
              border: activeTab === tab.key ? '1px solid var(--border)' : '1px solid transparent',
              transition: 'all 0.15s',
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {activeTab === 'current' && (
        <div>
          <Section title="服务器">
            <Field label="WebSocket 地址">
              <input
                value={urlDraft}
                onChange={(e) => setUrlDraft(e.target.value)}
                placeholder="ws://localhost:9527"
                disabled={connectionStatus === 'connected'}
                style={inputStyle}
              />
            </Field>
            <Field label="工作目录">
              <input
                value={dirDraft}
                onChange={(e) => setDirDraft(e.target.value)}
                placeholder="/path/to/project"
                style={inputStyle}
              />
            </Field>
          </Section>

          <Section title="模型">
            <Field label="模型名称（留空使用服务器默认）">
              <input
                value={modelDraft}
                onChange={(e) => setModelDraft(e.target.value)}
                placeholder="claude-opus-4-5"
                style={inputStyle}
              />
            </Field>
          </Section>

          <Section title="行为">
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={!!config.autoApprove}
                onChange={(e) => setConfig({ autoApprove: e.target.checked })}
                style={{ accentColor: 'var(--accent)', cursor: 'pointer', width: '14px', height: '14px' }}
              />
              <div>
                <p style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text)' }}>自动确认工具调用</p>
                <p style={{ fontSize: '12px', color: 'var(--text3)' }}>跳过每次工具执行前的人工确认</p>
              </div>
            </label>

            <Field label="执行模式">
              <select
                value={config.agentMode || 'auto'}
                onChange={(e) => setConfig({ agentMode: e.target.value as any })}
                style={selectStyle}
              >
                <option value="auto">自动</option>
                <option value="simple">单层</option>
                <option value="plan">计划</option>
                <option value="pipeline">流水线</option>
              </select>
            </Field>
          </Section>

          <Section title="沙盒模式">
            <label style={{ display: 'flex', alignItems: 'center', gap: '10px', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={sandboxEnabled}
                onChange={(e) => handleSandboxToggle(e.target.checked)}
                disabled={!isConnected}
                style={{ accentColor: 'var(--accent)', cursor: 'pointer', width: '14px', height: '14px' }}
              />
              <div>
                <p style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text)' }}>启用沙盒模式</p>
                <p style={{ fontSize: '12px', color: 'var(--text3)' }}>
                  在隔离环境中执行文件操作，支持回滚和提交
                  {!isConnected && <span style={{ color: 'var(--red)', marginLeft: '5px' }}>(需要连接服务器)</span>}
                </p>
              </div>
            </label>
          </Section>

          <div style={{ display: 'flex', gap: '10px', marginTop: '8px' }}>
            <button onClick={saveCurrentConfig} style={{
              flex: 2, padding: '10px',
              background: saved ? 'var(--green)' : 'var(--accent)',
              color: '#fff', borderRadius: '8px', fontWeight: '600', fontSize: '13px',
              transition: 'background 0.2s',
            }}>
              {saved ? '✓ 已保存' : '保存设置'}
            </button>
            <button onClick={reset} style={{
              flex: 1, padding: '10px',
              background: 'var(--red-dim)', color: 'var(--red)',
              borderRadius: '8px', fontWeight: '500', fontSize: '13px',
              border: '1px solid rgba(239,68,68,0.3)',
            }}>
              重置全部
            </button>
          </div>
        </div>
      )}

      {activeTab === 'presets' && (
        <div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '16px' }}>
            <p style={{ fontSize: '14px', color: 'var(--text2)' }}>保存多组配置，快速切换不同环境</p>
            <button
              onClick={() => setShowNewPreset(true)}
              style={{
                padding: '6px 12px',
                background: 'var(--accent)', color: '#fff',
                borderRadius: '6px', fontSize: '12px', fontWeight: '600',
              }}
            >
              ➕ 新建预设
            </button>
          </div>

          {/* Preset List */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            {presets.map(preset => (
              <div key={preset.id} style={{
                background: 'var(--surface)', border: '1px solid var(--border)',
                borderRadius: '8px', padding: '12px',
              }}>
                <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between' }}>
                  <div style={{ flex: 1 }}>
                    <h4 style={{ fontSize: '14px', fontWeight: '600', color: 'var(--text)', marginBottom: '4px' }}>
                      {preset.name}
                    </h4>
                    <p style={{ fontSize: '12px', color: 'var(--text3)', fontFamily: 'monospace' }}>
                      {preset.serverUrl}
                    </p>
                    {preset.workdir && (
                      <p style={{ fontSize: '12px', color: 'var(--text3)', fontFamily: 'monospace' }}>
                        📁 {preset.workdir}
                      </p>
                    )}
                    <div style={{ display: 'flex', gap: '6px', marginTop: '6px' }}>
                      <span style={{
                        fontSize: '10px', color: 'var(--text3)', background: 'var(--bg3)',
                        padding: '2px 6px', borderRadius: '4px',
                      }}>
                        {preset.agentMode}
                      </span>
                      {preset.autoApprove && (
                        <span style={{
                          fontSize: '10px', color: 'var(--green)', background: 'var(--green-dim)',
                          padding: '2px 6px', borderRadius: '4px',
                        }}>
                          自动确认
                        </span>
                      )}
                    </div>
                  </div>
                  <div style={{ display: 'flex', gap: '6px' }}>
                    <button
                      onClick={() => handleApplyPreset(preset.id)}
                      style={{
                        padding: '4px 8px', background: 'var(--accent)', color: '#fff',
                        borderRadius: '4px', fontSize: '11px', fontWeight: '600',
                      }}
                    >
                      应用
                    </button>
                    <button
                      onClick={() => editPreset(preset)}
                      style={{
                        padding: '4px 8px', background: 'var(--bg3)', color: 'var(--text2)',
                        borderRadius: '4px', fontSize: '11px',
                      }}
                    >
                      编辑
                    </button>
                    <button
                      onClick={() => deletePreset(preset.id)}
                      style={{
                        padding: '4px 8px', background: 'var(--red-dim)', color: 'var(--red)',
                        borderRadius: '4px', fontSize: '11px',
                      }}
                    >
                      删除
                    </button>
                  </div>
                </div>
              </div>
            ))}

            {presets.length === 0 && (
              <div style={{
                textAlign: 'center', padding: '32px',
                color: 'var(--text3)', fontSize: '13px',
              }}>
                <p>暂无保存的配置预设</p>
                <p style={{ marginTop: '4px' }}>点击上方"新建预设"来保存当前配置</p>
              </div>
            )}
          </div>

          {/* New/Edit Preset Form */}
          {showNewPreset && (
            <div style={{
              position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
              background: 'rgba(0,0,0,0.5)', zIndex: 1000,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              padding: '20px',
            }}>
              <div style={{
                background: 'var(--bg)', borderRadius: '12px',
                padding: '20px', width: '100%', maxWidth: '400px',
                border: '1px solid var(--border)',
              }}>
                <h3 style={{ fontSize: '16px', fontWeight: '600', marginBottom: '16px' }}>
                  {editingPreset ? '编辑预设' : '新建预设'}
                </h3>

                <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                  <Field label="预设名称">
                    <input
                      value={presetForm.name}
                      onChange={(e) => setPresetForm(p => ({ ...p, name: e.target.value }))}
                      placeholder="开发环境、测试环境等"
                      style={inputStyle}
                    />
                  </Field>

                  <Field label="服务器地址">
                    <input
                      value={presetForm.serverUrl}
                      onChange={(e) => setPresetForm(p => ({ ...p, serverUrl: e.target.value }))}
                      placeholder="ws://localhost:9527"
                      style={inputStyle}
                    />
                  </Field>

                  <Field label="工作目录">
                    <input
                      value={presetForm.workdir}
                      onChange={(e) => setPresetForm(p => ({ ...p, workdir: e.target.value }))}
                      placeholder="/path/to/project"
                      style={inputStyle}
                    />
                  </Field>

                  <Field label="模型">
                    <input
                      value={presetForm.model}
                      onChange={(e) => setPresetForm(p => ({ ...p, model: e.target.value }))}
                      placeholder="claude-opus-4-5"
                      style={inputStyle}
                    />
                  </Field>

                  <Field label="执行模式">
                    <select
                      value={presetForm.agentMode}
                      onChange={(e) => setPresetForm(p => ({ ...p, agentMode: e.target.value as any }))}
                      style={selectStyle}
                    >
                      <option value="auto">自动</option>
                      <option value="simple">单层</option>
                      <option value="plan">计划</option>
                      <option value="pipeline">流水线</option>
                    </select>
                  </Field>

                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      checked={presetForm.autoApprove}
                      onChange={(e) => setPresetForm(p => ({ ...p, autoApprove: e.target.checked }))}
                      style={{ accentColor: 'var(--accent)' }}
                    />
                    <span style={{ fontSize: '13px' }}>自动确认工具调用</span>
                  </label>
                </div>

                <div style={{ display: 'flex', gap: '8px', marginTop: '16px' }}>
                  <button
                    onClick={savePreset}
                    disabled={!presetForm.name.trim()}
                    style={{
                      flex: 1, padding: '8px',
                      background: presetForm.name.trim() ? 'var(--accent)' : 'var(--bg3)',
                      color: presetForm.name.trim() ? '#fff' : 'var(--text3)',
                      borderRadius: '6px', fontSize: '13px', fontWeight: '600',
                    }}
                  >
                    {editingPreset ? '更新' : '保存'}
                  </button>
                  <button
                    onClick={cancelEdit}
                    style={{
                      flex: 1, padding: '8px',
                      background: 'var(--bg3)', color: 'var(--text2)',
                      borderRadius: '6px', fontSize: '13px',
                    }}
                  >
                    取消
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const inputStyle: React.CSSProperties = {
  width: '100%', padding: '9px 12px',
  background: 'var(--bg3)', border: '1px solid var(--border)',
  borderRadius: '8px', color: 'var(--text)',
  outline: 'none', fontFamily: 'monospace', fontSize: '13px',
};

const selectStyle: React.CSSProperties = {
  ...inputStyle,
  fontFamily: 'inherit',
  cursor: 'pointer',
};

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <div style={{ marginBottom: '24px' }}>
    <h3 style={{ fontSize: '11px', fontWeight: '700', color: 'var(--text3)', textTransform: 'uppercase', letterSpacing: '0.08em', marginBottom: '12px' }}>{title}</h3>
    <div style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius)', padding: '14px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
      {children}
    </div>
  </div>
);

const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div>
    <label style={{ display: 'block', fontSize: '12px', fontWeight: '600', color: 'var(--text2)', marginBottom: '5px' }}>{label}</label>
    {children}
  </div>
);
