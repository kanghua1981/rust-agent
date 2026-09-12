import { describe, it, expect, afterEach } from 'vitest';
import { buildFileUrl, isLocalServer, canOpenLocally } from './fileTransfer';

const setTauri = (present: boolean) => {
  if (present) (globalThis as any).__TAURI__ = { core: { invoke: () => Promise.resolve() } };
  else delete (globalThis as any).__TAURI__;
};

describe('fileTransfer', () => {
  afterEach(() => setTauri(false));

  it('builds a root-level /file URL and keeps the token', () => {
    expect(buildFileUrl('ws://host:9527/agent', 'a/b.txt', 'tok'))
      .toBe('http://host:9527/file?path=a%2Fb.txt&token=tok');
  });

  it('drops the ws path prefix so the endpoint is reachable', () => {
    expect(new URL(buildFileUrl('wss://host/agent', 'x')).pathname).toBe('/file');
  });

  it('detects a same-machine server', () => {
    expect(isLocalServer('ws://localhost:9527')).toBe(true);
    expect(isLocalServer('ws://127.0.0.1:9527')).toBe(true);
    expect(isLocalServer('ws://10.0.0.5:9527')).toBe(false);
  });

  it('only opens locally on the same filesystem with no container in between', () => {
    setTauri(true);
    expect(canOpenLocally('ws://localhost:9527', 'normal')).toBe(true);
    // Container/sandbox paths are the server's, not the desktop's.
    expect(canOpenLocally('ws://localhost:9527', 'container')).toBe(false);
    expect(canOpenLocally('ws://localhost:9527', 'sandbox')).toBe(false);
    // A remote server's path does not exist locally.
    expect(canOpenLocally('ws://10.0.0.5:9527', 'normal')).toBe(false);
    // Without Tauri there is no local filesystem to open.
    setTauri(false);
    expect(canOpenLocally('ws://localhost:9527', 'normal')).toBe(false);
  });
});
