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
    <div className="page">
      <h2 className="page-title" style={{ marginBottom: 12 }}>设置</h2>

      {/* Environment Info */}
      {isDesktopApp() && (
        <div className="settings-desktop">
          <span style={{ fontSize: 20 }}>🖥️</span>
          <div className="fill">
            <div className="ttl">桌面应用模式</div>
            <div className="sub">请先启动 Agent 服务器，然后在下方配置连接地址</div>
          </div>
        </div>
      )}

      <div>
        <Section title="服务器">
          <Field label="WebSocket 地址">
            <input
              className="field field-lg"
              value={urlDraft}
              onChange={(e) => setUrlDraft(e.target.value)}
              placeholder="ws://localhost:9527"
              disabled={connectionStatus === 'connected'}
            />
          </Field>
          <Field label="工作目录">
            <input
              className="field field-lg"
              value={dirDraft}
              onChange={(e) => setDirDraft(e.target.value)}
              placeholder="/path/to/project"
            />
          </Field>
          <Field label="集群 Token（认证，留空则无需鉴权）">
            <input
              className="field field-lg"
              type="password"
              value={tokenDraft}
              onChange={(e) => setTokenDraft(e.target.value)}
              placeholder="无 token 则留空"
            />
          </Field>
        </Section>

        <Section title="模型">
          <Field label="模型名称（留空使用服务器默认）">
            {availableModels.length > 0 ? (
              <select
                className="field field-select"
                value={modelDraft}
                onChange={(e) => setModelDraft(e.target.value)}
              >
                <option value="">使用服务器默认</option>
                {availableModels.map(m => (
                  <option key={m.alias} value={m.alias}>{m.alias} — {m.model}</option>
                ))}
              </select>
            ) : (
              <input
                className="field field-lg"
                value={modelDraft}
                onChange={(e) => setModelDraft(e.target.value)}
                placeholder="claude-opus-4-5"
              />
            )}
          </Field>
        </Section>

        <Section title="行为">
          <label className="check-row">
            <input
              className="check-box"
              type="checkbox"
              checked={!!config.autoApprove}
              onChange={(e) => setConfig({ autoApprove: e.target.checked })}
            />
            <div>
              <p style={{ fontSize: 13, fontWeight: 500, color: 'var(--text)' }}>自动确认工具调用</p>
              <p style={{ fontSize: 12, color: 'var(--text3)' }}>跳过每次工具执行前的人工确认</p>
            </div>
          </label>

          <Field label="执行模式">
            <select
              className="field field-select"
              value={config.agentMode || 'auto'}
              onChange={(e) => setConfig({ agentMode: e.target.value as any })}
            >
              <option value="auto">自动</option>
              <option value="simple">单层</option>
              <option value="plan">计划</option>
            </select>
          </Field>
        </Section>

        <Section title="隔离模式">
          <Field label="隔离模式（连接时生效）">
            <select
              className="field field-select"
              value={config.isolation ?? 'container'}
              onChange={(e) => handleIsolationChange(e.target.value as 'normal' | 'container' | 'sandbox')}
            >
              <option value="normal">🕑3 直接运行（无容器，完全兼容）</option>
              <option value="container">🔲 容器模式（namespace 隔离，默认）</option>
              <option value="sandbox">🔒 沙盒模式（overlayfs 保护，支持回滚）</option>
            </select>
          </Field>
        </Section>

        <div className="settings-actions">
          <button className={`btn-save${saved ? ' saved' : ''}`} onClick={saveCurrentConfig}>
            {saved ? '✓ 已保存' : '保存设置'}
          </button>
          <button className="btn-reset" onClick={reset}>重置全部</button>
        </div>
      </div>
    </div>
  );
};

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <div className="settings-group">
    <h3 className="settings-group-title">{title}</h3>
    <div className="settings-box">{children}</div>
  </div>
);

const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div>
    <label className="settings-label">{label}</label>
    {children}
  </div>
);
