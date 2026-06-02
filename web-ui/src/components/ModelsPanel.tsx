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
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: '12px', color: 'var(--text3)', padding: '40px' }}>
        <span style={{ fontSize: '40px' }}>🧠</span>
        <p style={{ fontSize: '14px', fontWeight: '500' }}>未连接</p>
        <p style={{ fontSize: '12px', textAlign: 'center', lineHeight: 1.6 }}>连接到服务器后，模型配置将自动显示。</p>
      </div>
    );
  }

  const panelStyle: React.CSSProperties = { flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' };
  const subTabBar: React.CSSProperties = { display: 'flex', gap: '4px', padding: '12px 20px 0', borderBottom: '1px solid var(--border)', flexShrink: 0 };
  const subTabBtn = (active: boolean): React.CSSProperties => ({
    padding: '6px 14px', fontSize: '12px', fontWeight: active ? 600 : 400,
    color: active ? 'var(--text)' : 'var(--text3)', background: active ? 'var(--bg3)' : 'transparent',
    border: 'none', borderRadius: '6px 6px 0 0', cursor: 'pointer', transition: 'all 0.15s',
  });
  const contentStyle: React.CSSProperties = { flex: 1, overflowY: 'auto', padding: '16px 20px' };
  const cardStyle: React.CSSProperties = { background: 'var(--bg2)', border: '1px solid var(--border)', borderRadius: '8px', padding: '12px 16px', marginBottom: '8px' };
  const btnSmall: React.CSSProperties = { padding: '3px 10px', fontSize: '11px', borderRadius: '4px', border: '1px solid var(--border)', background: 'var(--bg3)', color: 'var(--text)', cursor: 'pointer' };
  const btnPrimary: React.CSSProperties = { ...btnSmall, background: '#6366f1', border: '1px solid #6366f1', color: '#fff' };
  const btnDanger: React.CSSProperties = { ...btnSmall, background: 'transparent', border: '1px solid #ef4444', color: '#ef4444' };
  const inputStyle: React.CSSProperties = { padding: '6px 10px', fontSize: '12px', background: 'var(--bg3)', border: '1px solid var(--border)', borderRadius: '6px', color: 'var(--text)', flex: 1 };
  const selectStyle: React.CSSProperties = { ...inputStyle, flex: 'none', width: '120px' };
  const labelStyle: React.CSSProperties = { fontSize: '11px', color: 'var(--text3)', marginBottom: '4px' };

  return (
    <div style={panelStyle}>
      {/* Sub-tab bar */}
      <div style={subTabBar}>
        {subTabs.map(t => (
          <button key={t.key} style={subTabBtn(subTab === t.key)} onClick={() => setSubTab(t.key)}>
            {t.icon} {t.label}
          </button>
        ))}
      </div>

      {/* ── Models tab ── */}
      {subTab === 'models' && (
        <div style={contentStyle}>
          <h3 style={{ fontSize: '13px', fontWeight: 600, marginBottom: '4px', color: 'var(--text)' }}>
            🧠 Models ({availableModels.length})
          </h3>
          <p style={{ fontSize: '11px', color: 'var(--text3)', marginBottom: '12px' }}>
            Click a model to switch. Use the <b>Fetch</b> tab to add models from an API.
          </p>
          {availableModels.length === 0 && (
            <div style={{ textAlign: 'center', padding: '40px', color: 'var(--text3)' }}>
              No models configured. Go to the <b>Fetch</b> tab.
            </div>
          )}
          {availableModels.map(m => {
            const isActive = m.alias === activeModel;
            return (
              <div key={m.alias} style={{ ...cardStyle, borderColor: isActive ? '#6366f1' : 'var(--border)', background: isActive ? 'rgba(99,102,241,0.08)' : 'var(--bg2)' }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <div style={{ flex: 1 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '4px' }}>
                      <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--text)' }}>{m.alias}</span>
                      {isActive && <span style={{ fontSize: '10px', padding: '1px 6px', borderRadius: '3px', background: '#6366f1', color: '#fff' }}>active</span>}
                    </div>
                    <div style={{ fontSize: '11px', color: 'var(--text3)' }}>
                      {m.model} via {m.endpoint ? <span style={{ color: '#818cf8' }}>{m.endpoint}</span> : <span>{m.provider || 'inline'}</span>}
                      {m.base_url && <span style={{ marginLeft: '8px', opacity: 0.6 }}>{m.base_url}</span>}
                    </div>
                    {(m.thinking_enabled || m.reasoning_effort) && (
                      <div style={{ fontSize: '10px', color: 'var(--text3)', marginTop: '2px' }}>
                        {m.thinking_enabled ? 'thinking ' : ''}{m.reasoning_effort ? `effort=${m.reasoning_effort}` : ''}
                      </div>
                    )}
                  </div>
                  <div style={{ display: 'flex', gap: '6px', flexShrink: 0 }}>
                    {!isActive && <button style={btnPrimary} onClick={() => onSetModelRemote(m.alias)}>Switch</button>}
                    <button style={btnDanger} onClick={() => { if (confirm(`Delete model '${m.alias}'?`)) onDeleteModel(m.alias); }}>Del</button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ── Endpoints tab ── */}
      {subTab === 'endpoints' && (
        <div style={contentStyle}>
          <h3 style={{ fontSize: '13px', fontWeight: 600, marginBottom: '4px', color: 'var(--text)' }}>
            🔗 Endpoints ({endpoints.length})
          </h3>
          <p style={{ fontSize: '11px', color: 'var(--text3)', marginBottom: '12px' }}>
            Endpoints define shared connection parameters for models.
          </p>

          {/* Add endpoint form */}
          <details style={{ marginBottom: '16px' }}>
            <summary style={{ fontSize: '12px', fontWeight: 500, cursor: 'pointer', color: 'var(--text2)', padding: '4px 0' }}>+ Add Endpoint</summary>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', padding: '10px', background: 'var(--bg3)', borderRadius: '8px', marginTop: '8px' }}>
              <div><div style={labelStyle}>Name</div><input style={inputStyle} value={epName} onChange={e => setEpName(e.target.value)} placeholder="e.g. deepseek" /></div>
              <div style={{ display: 'flex', gap: '8px' }}>
                <div style={{ flex: 1 }}><div style={labelStyle}>Base URL</div><input style={inputStyle} value={epUrl} onChange={e => setEpUrl(e.target.value)} placeholder="https://api.example.com/v1" /></div>
                <div><div style={labelStyle}>Provider</div>
                  <select style={selectStyle} value={epProvider} onChange={e => setEpProvider(e.target.value)}>
                    <option value="openai">openai</option>
                    <option value="anthropic">anthropic</option>
                    <option value="compatible">compatible</option>
                  </select>
                </div>
              </div>
              <div><div style={labelStyle}>API Key (optional)</div><input style={inputStyle} type="password" value={epKey} onChange={e => setEpKey(e.target.value)} placeholder="sk-..." /></div>
              <button style={{ ...btnPrimary, alignSelf: 'flex-start' }} onClick={() => {
                if (!epName.trim() || !epUrl.trim()) return;
                onAddEndpoint(epName.trim(), epProvider, epUrl.trim(), epKey.trim() || undefined);
                setEpName(''); setEpUrl(''); setEpKey('');
              }}>Add Endpoint</button>
            </div>
          </details>

          {endpoints.length === 0 && (
            <div style={{ textAlign: 'center', padding: '40px', color: 'var(--text3)' }}>No endpoints configured.</div>
          )}
          {endpoints.map((ep: import('../types/agent').EndpointInfo) => {
            const usingModels = availableModels.filter(m => m.endpoint === ep.name);
            return (
              <div key={ep.name} style={cardStyle}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <div>
                    <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--text)', marginBottom: '2px' }}>{ep.name}</div>
                    <div style={{ fontSize: '11px', color: 'var(--text3)' }}>
                      {ep.provider} · {ep.base_url}
                      {ep.has_api_key ? <span style={{ color: '#10b981', marginLeft: '8px' }}>key ✓</span> : <span style={{ color: '#f59e0b', marginLeft: '8px' }}>no key</span>}
                    </div>
                    {usingModels.length > 0 && (
                      <div style={{ fontSize: '10px', color: 'var(--text3)', marginTop: '2px' }}>
                        used by: {usingModels.map(m => m.alias).join(', ')}
                      </div>
                    )}
                  </div>
                  <button style={btnDanger} onClick={() => {
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
        <div style={contentStyle}>
          <h3 style={{ fontSize: '13px', fontWeight: 600, marginBottom: '4px', color: 'var(--text)' }}>
            📡 Fetch Models
          </h3>
          <p style={{ fontSize: '11px', color: 'var(--text3)', marginBottom: '12px' }}>
            Enter an API URL to discover available models. Supports OpenAI-compatible and Ollama endpoints.
          </p>

          {/* Fetch form */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginBottom: '16px' }}>
            <div style={{ display: 'flex', gap: '8px' }}>
              <input style={{ ...inputStyle, flex: 1 }} value={fetchUrl} onChange={e => setFetchUrl(e.target.value)}
                placeholder="https://api.openai.com/v1 or http://localhost:11434" />
              <button style={btnPrimary} onClick={() => {
                if (!fetchUrl.trim()) return;
                onFetchModels(fetchUrl.trim(), fetchKey.trim() || undefined);
              }}>Fetch</button>
            </div>
            <input style={{ ...inputStyle, maxWidth: '300px' }} type="password" value={fetchKey} onChange={e => setFetchKey(e.target.value)}
              placeholder="API Key (optional, or uses LLM_API_KEY env)" />
          </div>

          {/* Results */}
          {fetchedModels.length > 0 && (
            <div>
              <div style={{ fontSize: '11px', color: 'var(--text3)', marginBottom: '8px' }}>
                Found {fetchedModels.length} model(s) via {fetchedSource} from {fetchedUrlBase}
              </div>
              <p style={{ fontSize: '10px', color: 'var(--text3)', marginBottom: '12px' }}>
                Select models to add. An endpoint will be auto-created. Adjust aliases as needed.
              </p>
              {fetchedModels.map(modelName => {
                const alias = addAliasMap[modelName] || modelName;
                return (
                  <div key={modelName} style={{ ...cardStyle, display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <span style={{ flex: 1, fontSize: '12px', color: 'var(--text)', fontFamily: 'monospace' }}>{modelName}</span>
                    <input style={{ ...inputStyle, maxWidth: '160px', fontSize: '11px' }}
                      value={alias} onChange={e => setAddAliasMap(prev => ({ ...prev, [modelName]: e.target.value }))}
                      placeholder="alias" />
                    <button style={btnPrimary} onClick={() => {
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
