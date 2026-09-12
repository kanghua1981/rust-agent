import React, { useState, useCallback } from 'react';
import { useAgentStore } from '../stores/agentStore';

interface ModelsPanelProps {
  onSetModelRemote: (alias: string) => void;
  onFetchModels: (url: string, apiKey?: string) => void;
  onAddModel: (alias: string, model: string, endpoint: string) => void;
  onDeleteModel: (alias: string) => void;
  onListEndpoints: () => void;
  onAddEndpoint: (name: string, provider: string, baseUrl: string, apiKey?: string) => void;
  onDeleteEndpoint: (name: string) => void;
}

type SubTab = 'models' | 'endpoints' | 'fetch';

const subTabs: { key: SubTab; label: string; icon: string }[] = [
  { key: 'models', label: 'Models', icon: '🧠' },
  { key: 'endpoints', label: 'Endpoints', icon: '🔗' },
  { key: 'fetch', label: 'Fetch', icon: '📡' },
];

export const ModelsPanel: React.FC<ModelsPanelProps> = ({
  onSetModelRemote, onFetchModels, onAddModel, onDeleteModel,
  onListEndpoints, onAddEndpoint, onDeleteEndpoint,
}) => {
  const connectionStatus = useAgentStore(s => s.connectionStatus);
  const availableModels = useAgentStore(s => s.availableModels);
  const activeModel = useAgentStore(s => s.activeModel);
  const endpoints = useAgentStore(s => s.endpoints);
  const isConnected = connectionStatus === 'connected';

  const [subTab, setSubTab] = useState<SubTab>('models');

  // Fetch form state
  const [fetchUrl, setFetchUrl] = useState('');
  const [fetchKey, setFetchKey] = useState('');
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [fetchedSource, setFetchedSource] = useState('');
  const [fetchedUrlBase, setFetchedUrlBase] = useState('');
  const [addAliasMap, setAddAliasMap] = useState<Record<string, string>>({});

  // Add endpoint form
  const [epName, setEpName] = useState('');
  const [epProvider, setEpProvider] = useState('openai');
  const [epUrl, setEpUrl] = useState('');
  const [epKey, setEpKey] = useState('');

  // Listen for model_state events from useWebSocket
  React.useEffect(() => {
    if (isConnected) {
      onListEndpoints();
    }
  }, [isConnected, onListEndpoints]);

  // Listen for models_fetched event via store-like approach
  const handleFetchResult = useCallback((models: string[], source: string, url: string) => {
    setFetchedModels(models);
    setFetchedSource(source);
    setFetchedUrlBase(url);
    const defaultMap: Record<string, string> = {};
    models.forEach(m => {
      defaultMap[m] = m.replace(/[^a-zA-Z0-9]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '').toLowerCase();
    });
    setAddAliasMap(defaultMap);
  }, []);

  // Expose fetch result handler globally (useWebSocket will call this)
  React.useEffect(() => {
    (window as any).__onModelsFetched = handleFetchResult;
    return () => { delete (window as any).__onModelsFetched; };
  }, [handleFetchResult]);

  if (!isConnected) {
    return (
      <div className="empty-state">
        <span className="empty-icon">🧠</span>
        <p className="empty-title">未连接</p>
        <p className="empty-sub">连接到服务器后，模型配置将自动显示。</p>
      </div>
    );
  }

  const addEndpoint = () => {
    if (!epName.trim() || !epUrl.trim()) return;
    onAddEndpoint(epName.trim(), epProvider, epUrl.trim(), epKey.trim() || undefined);
    setEpName(''); setEpUrl(''); setEpKey('');
  };

  return (
    <div className="panel">
      {/* Sub-tab bar */}
      <div className="subtabs">
        {subTabs.map(t => (
          <button key={t.key} className={`subtab${subTab === t.key ? ' active' : ''}`} onClick={() => setSubTab(t.key)}>
            {t.icon} {t.label}
          </button>
        ))}
      </div>

      {/* ── Models tab ── */}
      {subTab === 'models' && (
        <div className="panel-content">
          <h3 className="panel-title-sm">🧠 Models ({availableModels.length})</h3>
          <p className="panel-hint">
            Click a model to switch. Use the <b>Fetch</b> tab to add models from an API.
          </p>
          {availableModels.length === 0 && (
            <div className="empty-box">No models configured. Go to the <b>Fetch</b> tab.</div>
          )}
          {availableModels.map(m => {
            const isActive = m.alias === activeModel;
            return (
              <div key={m.alias} className={`card tight${isActive ? ' active' : ''}`}>
                <div className="row-between">
                  <div className="fill">
                    <div className="row" style={{ gap: 8, marginBottom: 4 }}>
                      <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{m.alias}</span>
                      {isActive && <span className="pill-active">active</span>}
                    </div>
                    <div style={{ fontSize: 11, color: 'var(--text3)' }}>
                      {m.model} via {m.endpoint ? <span className="text-accent">{m.endpoint}</span> : <span>{m.provider || 'inline'}</span>}
                      {m.base_url && <span className="text-dim" style={{ marginLeft: 8 }}>{m.base_url}</span>}
                    </div>
                    {(m.thinking_enabled || m.reasoning_effort) && (
                      <div style={{ fontSize: 10, color: 'var(--text3)', marginTop: 2 }}>
                        {m.thinking_enabled ? 'thinking ' : ''}{m.reasoning_effort ? `effort=${m.reasoning_effort}` : ''}
                      </div>
                    )}
                  </div>
                  <div className="row" style={{ gap: 6, flexShrink: 0 }}>
                    {!isActive && <button className="btn-xs primary" onClick={() => onSetModelRemote(m.alias)}>Switch</button>}
                    <button className="btn-xs danger" onClick={() => { if (confirm(`Delete model '${m.alias}'?`)) onDeleteModel(m.alias); }}>Del</button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ── Endpoints tab ── */}
      {subTab === 'endpoints' && (
        <div className="panel-content">
          <h3 className="panel-title-sm">🔗 Endpoints ({endpoints.length})</h3>
          <p className="panel-hint">Endpoints define shared connection parameters for models.</p>

          {/* Add endpoint form */}
          <details style={{ marginBottom: 16 }}>
            <summary className="summary-link">+ Add Endpoint</summary>
            <div className="inline-form">
              <div><div className="form-label">Name</div><input className="field inline" value={epName} onChange={e => setEpName(e.target.value)} placeholder="e.g. deepseek" /></div>
              <div className="row" style={{ gap: 8 }}>
                <div className="fill"><div className="form-label">Base URL</div><input className="field inline" value={epUrl} onChange={e => setEpUrl(e.target.value)} placeholder="https://api.example.com/v1" /></div>
                <div><div className="form-label">Provider</div>
                  <select className="field sm" value={epProvider} onChange={e => setEpProvider(e.target.value)}>
                    <option value="openai">openai</option>
                    <option value="anthropic">anthropic</option>
                    <option value="compatible">compatible</option>
                  </select>
                </div>
              </div>
              <div><div className="form-label">API Key (optional)</div><input className="field inline" type="password" value={epKey} onChange={e => setEpKey(e.target.value)} placeholder="sk-..." /></div>
              <button className="btn-xs primary" style={{ alignSelf: 'flex-start' }} onClick={addEndpoint}>Add Endpoint</button>
            </div>
          </details>

          {endpoints.length === 0 && <div className="empty-box">No endpoints configured.</div>}
          {endpoints.map((ep: import('../types/agent').EndpointInfo) => {
            const usingModels = availableModels.filter(m => m.endpoint === ep.name);
            return (
              <div key={ep.name} className="card tight">
                <div className="row-between">
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)', marginBottom: 2 }}>{ep.name}</div>
                    <div style={{ fontSize: 11, color: 'var(--text3)' }}>
                      {ep.provider} · {ep.base_url}
                      {ep.has_api_key
                        ? <span className="text-ok" style={{ marginLeft: 8 }}>key ✓</span>
                        : <span className="text-warn" style={{ marginLeft: 8 }}>no key</span>}
                    </div>
                    {usingModels.length > 0 && (
                      <div style={{ fontSize: 10, color: 'var(--text3)', marginTop: 2 }}>
                        used by: {usingModels.map(m => m.alias).join(', ')}
                      </div>
                    )}
                  </div>
                  <button className="btn-xs danger" onClick={() => {
                    if (usingModels.length > 0) {
                      alert(`Cannot delete: endpoint '${ep.name}' is used by models: ${usingModels.map(m => m.alias).join(', ')}. Delete those models first.`);
                      return;
                    }
                    if (confirm(`Delete endpoint '${ep.name}'?`)) onDeleteEndpoint(ep.name);
                  }}>Del</button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ── Fetch tab ── */}
      {subTab === 'fetch' && (
        <div className="panel-content">
          <h3 className="panel-title-sm">📡 Fetch Models</h3>
          <p className="panel-hint">
            Enter an API URL to discover available models. Supports OpenAI-compatible and Ollama endpoints.
          </p>

          {/* Fetch form */}
          <div className="col" style={{ gap: 8, marginBottom: 16 }}>
            <div className="row" style={{ gap: 8 }}>
              <input className="field inline" value={fetchUrl} onChange={e => setFetchUrl(e.target.value)}
                placeholder="https://api.openai.com/v1 or http://localhost:11434" />
              <button className="btn-xs primary" onClick={() => {
                if (!fetchUrl.trim()) return;
                onFetchModels(fetchUrl.trim(), fetchKey.trim() || undefined);
              }}>Fetch</button>
            </div>
            <input className="field inline" style={{ maxWidth: 300 }} type="password" value={fetchKey} onChange={e => setFetchKey(e.target.value)}
              placeholder="API Key (optional, or uses LLM_API_KEY env)" />
          </div>

          {/* Results */}
          {fetchedModels.length > 0 && (
            <div>
              <div style={{ fontSize: 11, color: 'var(--text3)', marginBottom: 8 }}>
                Found {fetchedModels.length} model(s) via {fetchedSource} from {fetchedUrlBase}
              </div>
              <p style={{ fontSize: 10, color: 'var(--text3)', marginBottom: 12 }}>
                Select models to add. An endpoint will be auto-created. Adjust aliases as needed.
              </p>
              {fetchedModels.map(modelName => {
                const alias = addAliasMap[modelName] || modelName;
                return (
                  <div key={modelName} className="card tight row" style={{ gap: 8 }}>
                    <span className="mono fill" style={{ fontSize: 12, color: 'var(--text)' }}>{modelName}</span>
                    <input className="field mono-sm" style={{ maxWidth: 160 }}
                      value={alias} onChange={e => setAddAliasMap(prev => ({ ...prev, [modelName]: e.target.value }))}
                      placeholder="alias" />
                    <button className="btn-xs primary" onClick={() => {
                      onAddModel(alias, modelName, fetchedUrlBase);
                      // Remove from list
                      setFetchedModels(prev => prev.filter(m => m !== modelName));
                    }}>+ Add</button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
