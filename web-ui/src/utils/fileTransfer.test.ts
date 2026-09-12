import { describe, it, expect, afterEach } from 'vitest';
import { buildFileUrl, isLocalServer, localOpenAvailability } from './fileTransfer';

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

  it('hides the local open in a browser, since nothing can make it work', () => {
    expect(localOpenAvailability('ws://localhost:9527', 'normal')).toEqual({ kind: 'unsupported' });
  });

  it('explains why a local open is blocked when the cause is fixable', () => {
    setTauri(true);
    expect(localOpenAvailability('ws://localhost:9527', 'normal')).toEqual({ kind: 'available' });

    // A remote server's path does not exist on this machine.
    const remote = localOpenAvailability('ws://10.0.0.5:9527', 'normal');
    expect(remote.kind).toBe('blocked');
    expect(remote.kind === 'blocked' ? remote.reason : '').toContain('远端');

    // Container/sandbox paths belong to the server, not to the desktop.
    for (const mode of ['container', 'sandbox'] as const) {
      const blocked = localOpenAvailability('ws://localhost:9527', mode);
      expect(blocked.kind).toBe('blocked');
      expect(blocked.kind === 'blocked' ? blocked.reason : '').toContain('隔离');
    }
  });
});
