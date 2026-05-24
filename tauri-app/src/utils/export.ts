/**
 * Shared Tauri / browser export utilities.
 *
 * Used by SessionsPanel, TaskPanel, and any other component that needs to
 * export data as Markdown or JSON — either via Tauri native save-dialog or
 * a browser download blob.
 */

// ── Tauri helpers ────────────────────────────────────────────────────────────

/** Detect Tauri runtime (window.__TAURI__ is injected by Tauri WebView). */
export const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI__' in window;

/**
 * Return the Tauri `invoke` function, or `null` when not running in Tauri.
 * Supports both Tauri v2 (`__TAURI__.core.invoke`) and v1 (`__TAURI__.tauri.invoke`).
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const tauriInvoke = (): ((cmd: string, args?: Record<string, unknown>) => Promise<any>) | null => {
  if (!isTauri()) return null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (window as any).__TAURI__.core?.invoke ?? (window as any).__TAURI__.tauri?.invoke ?? null;
};

/** Trigger a browser download for the given Blob. */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Save `content` to `~/Downloads/{filename}` through Tauri's filesystem API.
 * Returns the absolute path where the file was written.
 */
export async function saveViaTauri(content: string, filename: string): Promise<string> {
  const invoke = tauriInvoke()!;
  const homeDir = await invoke('get_home_dir');
  const filePath = `${homeDir}/Downloads/${filename}`;
  await invoke('write_file', { path: filePath, content });
  return filePath;
}

// ── Generic export helpers ────────────────────────────────────────────────────

/** Minimal message shape expected by the generic exporters. */
export interface ExportableMessage {
  role: string;
  content: string;
  timestamp: number;
  meta?: Record<string, unknown>;
}

/** Minimal tool-call shape. */
export interface ExportableToolCall {
  tool: string;
  status: string;
  input: unknown;
  output?: string;
}

export interface ExportSession {
  messages: ExportableMessage[];
  toolCalls?: ExportableToolCall[];
  extraHeader?: string; // e.g. "> 任务: …"
}

/**
 * Export a session as a Markdown file.
 * Calls `onSaved` with the filename (browser) or absolute path (Tauri).
 */
export async function exportSessionAsMarkdown(
  session: ExportSession,
  onSaved: (path: string) => void,
  onError: (err: string) => void,
): Promise<void> {
  const lines: string[] = [
    `# Agent 对话导出\n\n> 导出时间: ${new Date().toLocaleString()}\n`,
  ];
  if (session.extraHeader) lines.push(session.extraHeader + '\n');

  for (const msg of session.messages) {
    if (msg.role === 'system') continue;
    const label = msg.role === 'user' ? '**用户**' : '**助手**';
    lines.push(`---\n\n${label}\n\n${msg.content}\n`);
  }

  if (session.toolCalls && session.toolCalls.length > 0) {
    lines.push('---\n\n## 工具调用记录\n');
    for (const c of session.toolCalls) {
      lines.push(
        `- **${c.tool}** (${c.status}): \`${JSON.stringify(c.input).slice(0, 120)}\`\n`,
      );
    }
  }

  const filename = `agent-chat-${Date.now()}.md`;
  const text = lines.join('\n');

  if (isTauri()) {
    try {
      const p = await saveViaTauri(text, filename);
      onSaved(p);
    } catch (e) {
      onError(String(e));
    }
  } else {
    downloadBlob(new Blob([text], { type: 'text/markdown;charset=utf-8' }), filename);
    onSaved(filename);
  }
}

/**
 * Export a session as a JSON file.
 */
export async function exportSessionAsJson(
  session: ExportSession,
  onSaved: (path: string) => void,
  onError: (err: string) => void,
): Promise<void> {
  const data: Record<string, unknown> = {
    exported_at: new Date().toISOString(),
    messages: session.messages
      .filter((m) => m.role !== 'system')
      .map((m) => ({ role: m.role, content: m.content, timestamp: m.timestamp })),
  };
  if (session.toolCalls && session.toolCalls.length > 0) {
    data.tool_calls = session.toolCalls.map((c) => ({
      tool: c.tool,
      status: c.status,
      input: c.input,
      output: c.output,
    }));
  }

  const filename = `agent-chat-${Date.now()}.json`;
  const text = JSON.stringify(data, null, 2);

  if (isTauri()) {
    try {
      const p = await saveViaTauri(text, filename);
      onSaved(p);
    } catch (e) {
      onError(String(e));
    }
  } else {
    downloadBlob(new Blob([text], { type: 'application/json;charset=utf-8' }), filename);
    onSaved(filename);
  }
}
