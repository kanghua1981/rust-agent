import React, { useState, useEffect } from 'react';
import { useAgentStore } from '../stores/agentStore';
import { isDesktopApp, getEnvironmentInfo } from '../utils/environment';

interface SettingsPanelProps {
  isConnected: boolean;
  onSetWorkdirRemote: (workdir: string) => void;
}

export const SettingsPanel: React.FC<SettingsPanelProps> = ({ isConnected, onSetWorkdirRemote }) => {
  const { 
    serverUrl, setServerUrl, workdir, setWorkdir, config, setConfig, reset,
    clusterToken, setClusterToken, connectionStatus,
    availableModels,
  } = useAgentStore();
  
  // Current config form state
  const [urlDraft, setUrlDraft] = useState(serverUrl);
  const [dirDraft, setDirDraft] = useState(workdir ?? '');
  const [tokenDraft, setTokenDraft] = useState(clusterToken);
  const [modelDraft, setModelDraft] = useState(config.model ?? '');  

  // Keep draft in sync with store when workdir changes externally
  useEffect(() => {
    setDirDraft(workdir ?? '');
  }, [workdir]);
  const [saved, setSaved] = useState(false);

  const handleIsolationChange = (mode: 'normal' | 'container' | 'sandbox') => {
    setConfig({ isolation: mode });
  };

  const saveCurrentConfig = () => {
    setServerUrl(urlDraft.trim() || 'ws://localhost:9527');
    const newWorkdir = dirDraft.trim();
    setWorkdir(newWorkdir);
    if (isConnected && newWorkdir) {
      onSetWorkdirRemote(newWorkdir);
    }
    setClusterToken(tokenDraft.trim());
    if (modelDraft.trim()) setConfig({ model: modelDraft.trim() });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
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
          <Field label="集群 Token（认证，留空则无需鉴权）">
            <input
              type="password"
              value={tokenDraft}
              onChange={(e) => setTokenDraft(e.target.value)}
              placeholder="无 token 则留空"
              style={inputStyle}
            />
          </Field>
        </Section>

        <Section title="模型">
          <Field label="模型名称（留空使用服务器默认）">
            {availableModels.length > 0 ? (
              <select
                value={modelDraft}
                onChange={(e) => setModelDraft(e.target.value)}
                style={selectStyle}
              >
                <option value="">使用服务器默认</option>
                {availableModels.map(m => (
                  <option key={m.alias} value={m.alias}>{m.alias} — {m.model}</option>
                ))}
              </select>
            ) : (
              <input
                value={modelDraft}
                onChange={(e) => setModelDraft(e.target.value)}
                placeholder="claude-opus-4-5"
                style={inputStyle}
              />
            )}
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

        <Section title="隔离模式">
          <Field label="隔离模式（连接时生效）">
            <select
              value={config.isolation ?? 'container'}
              onChange={(e) => handleIsolationChange(e.target.value as 'normal' | 'container' | 'sandbox')}
              style={selectStyle}
            >
              <option value="normal">🕑3 直接运行（无容器，完全兼容）</option>
              <option value="container">🔲 容器模式（namespace 隔离，默认）</option>
              <option value="sandbox">🔒 沙盒模式（overlayfs 保护，支持回滚）</option>
            </select>
          </Field>
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
  padding: '9px 30px 9px 12px',
  fontFamily: 'inherit',
  cursor: 'pointer',
  appearance: 'none',
  WebkitAppearance: 'none',
  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%239499b0' d='M6 8L1 3h10z'/%3E%3C/svg%3E")`,
  backgroundRepeat: 'no-repeat',
  backgroundPosition: 'right 10px center',
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
