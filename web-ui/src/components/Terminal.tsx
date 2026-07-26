import React, { useEffect, useRef, useCallback } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface Props {
  onPtyOpen: (workdir: string | undefined, rows: number, cols: number) => void;
  onPtyInput: (input: string) => void;
  onPtyResize: (rows: number, cols: number) => void;
  onPtyClose: () => void;
  registerPtyOutput: (cb: (data: string) => void) => void;
  isConnected: boolean;
}

export const TerminalView: React.FC<Props> = ({
  onPtyOpen,
  onPtyInput,
  onPtyResize,
  onPtyClose,
  registerPtyOutput,
  isConnected,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const ptyOpenedRef = useRef(false);

  // ── Initialise terminal on mount (re-initialise on reconnect) ──
  useEffect(() => {
    if (!containerRef.current || !isConnected) return;
    if (termRef.current) return; // already initialised

    const container = containerRef.current;

    const term = new XTerm({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontSize: 14,
      fontFamily: "'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Source Code Pro', 'Ubuntu Mono', 'DejaVu Sans Mono', 'Liberation Mono', 'Consolas', 'Menlo', 'Courier New', monospace",
      theme: {
        background: '#1a1a2e',
        foreground: '#e0e0e0',
        cursor: '#00bcd4',
        cursorAccent: '#1a1a2e',
        selectionBackground: '#37474f',
        black: '#1a1a2e',
        red: '#ff6e6e',
        green: '#69f0ae',
        yellow: '#ffd54f',
        blue: '#40c4ff',
        magenta: '#ea80fc',
        cyan: '#00bcd4',
        white: '#e0e0e0',
        brightBlack: '#4a4a6a',
        brightRed: '#ff8a80',
        brightGreen: '#b9f6ca',
        brightYellow: '#ffe57f',
        brightBlue: '#80d8ff',
        brightMagenta: '#f48fb1',
        brightCyan: '#84ffff',
        brightWhite: '#ffffff',
      },
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);

    // Force initial fit using the container's current dimensions
    const fitNow = () => {
      try { fitAddon.fit(); } catch {}
    };

    // Defer fit + PTY open to next frame so layout has a chance to settle
    const raf = requestAnimationFrame(() => {
      fitNow();

      // Register output callback BEFORE opening PTY (avoid race)
      registerPtyOutput((b64: string) => {
        const t = termRef.current;
        if (!t) return;
        try { t.write(atob(b64)); } catch {}
      });

      // Open PTY — callback is already registered
      ptyOpenedRef.current = true;
      onPtyOpen(undefined, term.rows, term.cols);

      // User input → backend
      term.onData((data: string) => {
        onPtyInput(btoa(data));
      });

      // Terminal resize → backend
      term.onResize(({ rows, cols }: { cols: number; rows: number }) => {
        onPtyResize(rows, cols);
      });
    });

    termRef.current = term;
    fitAddonRef.current = fitAddon;

    // ── ResizeObserver to keep terminal fitted ──
    const resizeObserver = new ResizeObserver(() => {
      if (fitAddonRef.current && termRef.current) {
        try { fitAddonRef.current.fit(); } catch {}
      }
    });
    resizeObserver.observe(container);

    return () => {
      cancelAnimationFrame(raf);
      resizeObserver.disconnect();
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
      if (ptyOpenedRef.current) {
        ptyOpenedRef.current = false;
        onPtyClose();
      }
    };
  }, [isConnected]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── If not connected, show a very obvious message ──
  if (!isConnected) {
    return (
      <div style={{
        flex: 1, minHeight: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        flexDirection: 'column',
        gap: 8,
        color: '#ff6e6e',
        fontSize: 14,
        userSelect: 'none',
        background: '#1a1a2e',
      }}>
        <span style={{ fontSize: 32 }}>🔌</span>
        <span>未连接到服务器</span>
        <span style={{ fontSize: 11, color: '#888' }}>请先连接到 Agent 服务器</span>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      style={{
        flex: 1,
        minHeight: 0,
        width: '100%',
        overflow: 'hidden',
      }}
    />
  );
};
