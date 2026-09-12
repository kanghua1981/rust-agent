/**
 * Regression tests for per-connection isolation.
 *
 * Several workspaces can be connected and running at once; each WebSocket owns
 * its own connection slot. The hook keeps the streaming bookkeeping (which
 * message is being appended to, which tokens are buffered) in refs that must
 * follow the active slot. These tests drive two connections through overlapping
 * turns and assert that neither slot ever receives the other's text.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act } from 'react-dom/test-utils';
import { createRoot } from 'react-dom/client';
import React from 'react';
import { useWebSocket } from './useWebSocket';
import { useAgentStore } from '../stores/agentStore';

// ── Minimal WebSocket double ────────────────────────────────────────────────
class FakeSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  static CLOSED = 3;
  /** Every socket constructed during a test, in creation order. */
  static all: FakeSocket[] = [];

  readyState = FakeSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {
    FakeSocket.all.push(this);
  }
  send() {}
  close() { this.readyState = FakeSocket.CLOSED; }
  open() { this.readyState = FakeSocket.OPEN; this.onopen?.(); }
  emit(event: unknown) { this.onmessage?.({ data: JSON.stringify(event) }); }
}

(globalThis as any).WebSocket = FakeSocket;
(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;

function renderHook<T>(hook: () => T): T {
  const box = { current: undefined as unknown as T };
  function Probe() { box.current = hook(); return null; }
  act(() => { createRoot(document.createElement('div')).render(React.createElement(Probe)); });
  return box.current;
}

/** Assistant message contents of one connection slot. */
const contents = (slotId: string) =>
  (useAgentStore.getState().projectSlots[slotId]?.messages ?? [])
    .filter(m => m.role === 'assistant')
    .map(m => m.content);

/** Connect slots A and B and hand back their sockets. */
function connectTwo() {
  const api = renderHook(() => useWebSocket());
  const st = useAgentStore.getState();
  st.createConnectionSlot('A', 'A', 'ws://a', '/a');
  st.createConnectionSlot('B', 'B', 'ws://b', '/b');
  useAgentStore.getState().setActiveConnection('A');
  act(() => { api.connect('A'); api.connect('B'); });
  const [a, b] = FakeSocket.all;
  act(() => { a.open(); b.open(); });
  return { api, a, b };
}

/** Drive one complete assistant turn on a socket. */
const turn = (socket: FakeSocket, text: string) => {
  socket.emit({ type: 'role_header', data: { label: '🤖 Agent', model: 'test-model' } });
  socket.emit({ type: 'stream_start', data: {} });
  for (const ch of text) socket.emit({ type: 'streaming_token', data: { token: ch } });
  socket.emit({ type: 'stream_end', data: {} });
  socket.emit({ type: 'done', data: {} });
};

const settle = () => act(() => { vi.advanceTimersByTime(500); });

describe('useWebSocket — connection isolation', () => {
  beforeEach(() => {
    FakeSocket.all = [];
    vi.useFakeTimers();
    useAgentStore.setState({
      projects: {},
      projectSlots: {},
      activeProjectId: null,
      activeConnectionId: null,
      messages: [],
      toolCalls: [],
      pendingConfirmations: [],
      connectionStatus: 'disconnected',
      openFile: null,
    });
  });

  it('keeps a background turn\'s text in its own slot while another slot is active', () => {
    const { a, b } = connectTwo();

    act(() => { turn(a, 'A1A2'); });
    settle();
    expect(contents('A')).toEqual(['A1A2']);

    // B runs a full turn while the user is still looking at A.
    act(() => { turn(b, 'X1X2'); });
    settle();
    expect(contents('B')).toEqual(['X1X2']);

    // Switching to B shows what B produced, not what A produced.
    act(() => { useAgentStore.getState().setActiveProject('B'); });
    settle();
    expect(contents('B')).toEqual(['X1X2']);
    expect(contents('A')).toEqual(['A1A2']);
  });

  it('lands streamed text on the active slot before the turn ends', () => {
    const { a } = connectTwo();

    act(() => {
      a.emit({ type: 'role_header', data: { label: '🤖 Agent', model: 'test-model' } });
      a.emit({ type: 'stream_start', data: {} });
      a.emit({ type: 'streaming_token', data: { token: '你' } });
      a.emit({ type: 'streaming_token', data: { token: '好' } });
    });
    settle();

    // No stream_end yet — the buffered tokens must already be visible.
    expect(contents('A')).toEqual(['你好']);
  });

  it('loads a file into the in-app viewer and ignores results for other paths', () => {
    const { a, api } = connectTwo();

    act(() => { api.openFileInApp('src/main.rs'); });
    expect(useAgentStore.getState().openFile).toEqual({ path: 'src/main.rs', loading: true });

    // A response for a file the user is no longer looking at must not land.
    act(() => { a.emit({ type: 'file_content_result', data: { path: 'other.txt', content: 'x', size: 1 } }); });
    expect(useAgentStore.getState().openFile?.loading).toBe(true);

    act(() => { a.emit({ type: 'file_content_result', data: { path: 'src/main.rs', content: 'fn main() {}', size: 12 } }); });
    const opened = useAgentStore.getState().openFile;
    expect(opened?.loading).toBe(false);
    expect(opened?.content).toBe('fn main() {}');
    expect(opened?.size).toBe(12);
  });

  it('surfaces a rejected read as a viewer error', () => {
    const { a, api } = connectTwo();

    act(() => { api.openFileInApp('secret.txt'); });
    act(() => {
      a.emit({ type: 'error', data: { message: "Access denied: 'secret.txt' is outside the project directory." } });
    });

    const opened = useAgentStore.getState().openFile;
    expect(opened?.loading).toBe(false);
    expect(opened?.error).toContain('Access denied');
  });

  it('does not carry buffered text from the slot left behind into the new one', () => {
    const { a, b } = connectTwo();

    // A is mid-stream (its text is buffered, not yet written) when the user switches.
    act(() => {
      a.emit({ type: 'role_header', data: { label: '🤖 Agent', model: 'test-model' } });
      a.emit({ type: 'stream_start', data: {} });
      a.emit({ type: 'streaming_token', data: { token: '你' } });
      a.emit({ type: 'streaming_token', data: { token: '好' } });
    });
    act(() => { useAgentStore.getState().setActiveProject('B'); });

    // A finishes in the background while B starts its own turn.
    act(() => { a.emit({ type: 'stream_end', data: {} }); a.emit({ type: 'done', data: {} }); });
    act(() => { turn(b, 'B1B2'); });
    settle();

    expect(contents('A')).toEqual(['你好']);
    // The regression: A's buffered "你好" used to be flushed into B's message.
    expect(contents('B')).toEqual(['B1B2']);
  });
});
