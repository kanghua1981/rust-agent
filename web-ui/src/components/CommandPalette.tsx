/**
 * CommandPalette — 轻量级命令面板（类似 VS Code Ctrl+Shift+P）
 *
 * - Ctrl+Shift+P 唤起
 * - 输入关键词即时过滤
 * - ↑↓ 导航，Enter 执行，Esc 关闭
 * - 命令按分类分组显示
 */

import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';

// ── Types ──────────────────────────────────────────────────────────────────────

export interface CommandAction {
  /** Unique identifier, e.g. "session.new" */
  id: string;
  /** Display label */
  label: string;
  /** Short description shown in the palette */
  description: string;
  /** Category for grouping */
  category: string;
  /** Extra search keywords (space-separated) */
  keywords?: string;
  /** Whether this command is currently available */
  enabled?: boolean;
  /** Execute the command */
  action: () => void;
}

// ── Props ──────────────────────────────────────────────────────────────────────

interface Props {
  open: boolean;
  onClose: () => void;
  /** All commands (built-in + extra) supplied by the host */
  extraActions: CommandAction[];
}

// ── Component ──────────────────────────────────────────────────────────────────

export const CommandPalette: React.FC<Props> = ({ open, onClose, extraActions }) => {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Filter
  const allCommands = extraActions;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      // Show all, enabled first
      return [...allCommands].sort((a, b) => (a.enabled === false ? 1 : 0) - (b.enabled === false ? 1 : 0));
    }
    const words = q.split(/\s+/);
    return allCommands
      .filter((c) => {
        const haystack = `${c.label} ${c.description} ${c.keywords ?? ''} ${c.category}`.toLowerCase();
        return words.every((w) => haystack.includes(w));
      })
      .sort((a, b) => (a.enabled === false ? 1 : 0) - (b.enabled === false ? 1 : 0));
  }, [allCommands, query]);

  // Group by category
  const grouped = useMemo(() => {
    const map = new Map<string, CommandAction[]>();
    for (const c of filtered) {
      const arr = map.get(c.category) || [];
      arr.push(c);
      map.set(c.category, arr);
    }
    return map;
  }, [filtered]);

  // Flatten for index-based navigation
  const flatList = useMemo(() => {
    const out: { category: string; cmd: CommandAction }[] = [];
    for (const [cat, cmds] of grouped) {
      for (const c of cmds) out.push({ category: cat, cmd: c });
    }
    return out;
  }, [grouped]);

  // Reset state on open
  useEffect(() => {
    if (open) {
      setQuery('');
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Clamp selectedIndex
  useEffect(() => {
    if (selectedIndex >= flatList.length) setSelectedIndex(Math.max(0, flatList.length - 1));
  }, [flatList.length, selectedIndex]);

  // Scroll selected into view
  useEffect(() => {
    const el = listRef.current?.querySelector('[data-selected="true"]');
    el?.scrollIntoView({ block: 'nearest' });
  }, [selectedIndex]);

  const execute = useCallback(
    (cmd: CommandAction) => {
      if (cmd.enabled === false) return;
      cmd.action();
      onClose();
    },
    [onClose],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, flatList.length - 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case 'Enter':
          e.preventDefault();
          if (flatList[selectedIndex]) execute(flatList[selectedIndex].cmd);
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
      }
    },
    [flatList, selectedIndex, execute, onClose],
  );

  if (!open) return null;

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 1100,
        background: 'rgba(0,0,0,0.5)',
        backdropFilter: 'blur(3px)',
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'center',
        paddingTop: '16vh',
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: '100%',
          maxWidth: '560px',
          background: 'var(--bg2)',
          border: '1px solid var(--border)',
          borderRadius: '14px',
          boxShadow: '0 16px 48px rgba(0,0,0,0.5)',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
          maxHeight: '60vh',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '10px',
            padding: '14px 18px',
            borderBottom: '1px solid var(--border)',
            flexShrink: 0,
          }}
        >
          <span style={{ fontSize: '16px', flexShrink: 0 }}>🔍</span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={handleKeyDown}
            placeholder="输入命令名称搜索…"
            style={{
              flex: 1,
              background: 'transparent',
              border: 'none',
              outline: 'none',
              color: 'var(--text)',
              fontSize: '14px',
              fontFamily: 'inherit',
            }}
          />
          <span
            style={{
              fontSize: '10px',
              color: 'var(--text3)',
              background: 'var(--bg3)',
              border: '1px solid var(--border)',
              borderRadius: '4px',
              padding: '2px 6px',
              flexShrink: 0,
            }}
          >
            Esc
          </span>
        </div>

        {/* Results */}
        <div ref={listRef} style={{ flex: 1, overflowY: 'auto', padding: '8px' }}>
          {flatList.length === 0 ? (
            <div style={{ textAlign: 'center', padding: '32px 16px', color: 'var(--text3)', fontSize: '13px' }}>
              没有匹配的命令
            </div>
          ) : (
            [...grouped].map(([category, cmds]) => (
              <div key={category} style={{ marginBottom: '4px' }}>
                <div
                  style={{
                    fontSize: '10px',
                    fontWeight: '600',
                    color: 'var(--text3)',
                    letterSpacing: '0.08em',
                    textTransform: 'uppercase',
                    padding: '8px 10px 4px',
                  }}
                >
                  {category}
                </div>
                {cmds.map((cmd) => {
                  const idx = flatList.findIndex((f) => f.cmd.id === cmd.id);
                  const selected = idx === selectedIndex;
                  const disabled = cmd.enabled === false;

                  return (
                    <div
                      key={cmd.id}
                      data-selected={selected}
                      onClick={() => execute(cmd)}
                      onMouseEnter={() => setSelectedIndex(idx)}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: '10px',
                        padding: '8px 10px',
                        borderRadius: '8px',
                        cursor: disabled ? 'not-allowed' : 'pointer',
                        background: selected ? 'var(--accent-glow)' : 'transparent',
                        border: selected ? '1px solid rgba(99,102,241,0.3)' : '1px solid transparent',
                        opacity: disabled ? 0.4 : 1,
                        transition: 'background 0.1s',
                      }}
                    >
                      <span
                        style={{
                          fontSize: '14px',
                          width: '20px',
                          textAlign: 'center',
                          flexShrink: 0,
                          color: selected ? 'var(--accent)' : 'var(--text3)',
                        }}
                      >
                        {cmd.label.includes('切换') || cmd.label.includes('模式') ? '⚡' : '▶'}
                      </span>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: '13px', fontWeight: '500', color: selected ? 'var(--accent)' : 'var(--text)' }}>
                          {cmd.label}
                        </div>
                        <div style={{ fontSize: '11px', color: 'var(--text3)', marginTop: '1px' }}>
                          {cmd.description}
                        </div>
                      </div>
                      {disabled && (
                        <span style={{ fontSize: '10px', color: 'var(--text3)', flexShrink: 0 }}>不可用</span>
                      )}
                    </div>
                  );
                })}
              </div>
            ))
          )}
        </div>

        {/* Footer hint */}
        <div
          style={{
            display: 'flex',
            gap: '16px',
            padding: '8px 16px',
            borderTop: '1px solid var(--border)',
            background: 'var(--bg3)',
            flexShrink: 0,
            fontSize: '10px',
            color: 'var(--text3)',
          }}
        >
          <span>↑↓ 导航</span>
          <span>↵ 执行</span>
          <span>Esc 关闭</span>
          <span style={{ marginLeft: 'auto' }}>{flatList.length} 个命令</span>
        </div>
      </div>
    </div>
  );
};
