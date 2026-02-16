/**
 * Webview chat panel for the Rust Coding Agent.
 *
 * Renders a chat-style UI in the VS Code sidebar:
 *  - Streaming text output from the Agent
 *  - Tool use / result indicators
 *  - User message input
 *  - Connection status badge
 */

import * as vscode from 'vscode';

export interface ChatViewCallbacks {
    onSendMessage: (text: string) => void;
}

export class ChatViewProvider implements vscode.WebviewViewProvider {
    public static readonly viewType = 'rustAgent.chatView';

    private view?: vscode.WebviewView;
    private readonly extensionUri: vscode.Uri;
    private readonly callbacks: ChatViewCallbacks;

    constructor(extensionUri: vscode.Uri, callbacks: ChatViewCallbacks) {
        this.extensionUri = extensionUri;
        this.callbacks = callbacks;
    }

    /** Called by VS Code when the sidebar panel becomes visible. */
    resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ) {
        this.view = webviewView;

        webviewView.webview.options = {
            enableScripts: true,
            localResourceRoots: [this.extensionUri],
        };

        webviewView.webview.html = this.getHtml();

        // Handle messages from the webview (user input)
        webviewView.webview.onDidReceiveMessage((msg: any) => {
            switch (msg.type) {
                case 'send':
                    if (msg.text?.trim()) {
                        this.callbacks.onSendMessage(msg.text.trim());
                    }
                    break;
            }
        });
    }

    /** Forward an Agent event to the webview. */
    postMessage(msg: any) {
        this.view?.webview.postMessage(msg);
    }

    /** Generate the full HTML for the chat webview. */
    private getHtml(): string {
        return /*html*/ `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<style>
/* ── Reset & root ────────────────────────────────────── */
* { box-sizing: border-box; margin: 0; padding: 0; }

html, body {
    height: 100%;
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    color: var(--vscode-foreground);
    background: var(--vscode-sideBar-background, var(--vscode-editor-background));
}

body {
    display: flex;
    flex-direction: column;
}

/* ── Status bar ──────────────────────────────────────── */
#status-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    font-size: 11px;
    border-bottom: 1px solid var(--vscode-panel-border, #333);
    flex-shrink: 0;
}

#status-dot {
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--vscode-editorWarning-foreground, #ccc);
}
#status-dot.connected { background: var(--vscode-terminal-ansiGreen, #4ec9b0); }
#status-dot.disconnected { background: var(--vscode-editorError-foreground, #f44); }

/* ── Chat area ───────────────────────────────────────── */
#chat {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.msg {
    max-width: 100%;
    padding: 6px 10px;
    border-radius: 6px;
    line-height: 1.45;
    word-wrap: break-word;
    white-space: pre-wrap;
}

.msg.user {
    align-self: flex-end;
    background: var(--vscode-button-background, #0e639c);
    color: var(--vscode-button-foreground, #fff);
    border-radius: 12px 12px 2px 12px;
    max-width: 85%;
}

.msg.assistant {
    align-self: flex-start;
    background: var(--vscode-editor-inactiveSelectionBackground, #264f78);
    border-radius: 12px 12px 12px 2px;
}

.msg.thinking {
    font-style: italic;
    opacity: 0.7;
    font-size: 0.9em;
}

.msg.tool {
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: 0.85em;
    background: var(--vscode-textBlockQuote-background, #222);
    border-left: 3px solid var(--vscode-textLink-foreground, #3794ff);
    padding: 6px 10px;
}
.msg.tool.error {
    border-left-color: var(--vscode-editorError-foreground, #f44);
}

.msg.system {
    align-self: center;
    font-size: 0.85em;
    opacity: 0.6;
    background: transparent;
}

.msg.confirm {
    background: var(--vscode-inputValidation-warningBackground, #352a05);
    border: 1px solid var(--vscode-inputValidation-warningBorder, #9d8617);
    font-size: 0.9em;
}
.msg.confirm .approved { color: var(--vscode-terminal-ansiGreen, #4ec9b0); }
.msg.confirm .denied { color: var(--vscode-editorError-foreground, #f44); }

.msg.context-warn {
    font-size: 0.85em;
    opacity: 0.8;
    color: var(--vscode-editorWarning-foreground, #cca700);
}

/* ── Input area ──────────────────────────────────────── */
#input-area {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-top: 1px solid var(--vscode-panel-border, #333);
    flex-shrink: 0;
}

#input {
    flex: 1;
    padding: 6px 10px;
    border: 1px solid var(--vscode-input-border, #3c3c3c);
    background: var(--vscode-input-background, #1e1e1e);
    color: var(--vscode-input-foreground, #ccc);
    border-radius: 4px;
    font-family: inherit;
    font-size: inherit;
    resize: none;
    min-height: 34px;
    max-height: 120px;
}
#input:focus { outline: 1px solid var(--vscode-focusBorder, #007fd4); }

#send-btn {
    padding: 6px 12px;
    background: var(--vscode-button-background, #0e639c);
    color: var(--vscode-button-foreground, #fff);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: inherit;
    align-self: flex-end;
}
#send-btn:hover {
    background: var(--vscode-button-hoverBackground, #1177bb);
}
</style>
</head>
<body>

<div id="status-bar">
    <span id="status-dot"></span>
    <span id="status-text">Not connected</span>
</div>

<div id="chat"></div>

<div id="input-area">
    <textarea id="input" rows="1" placeholder="Ask the agent…"></textarea>
    <button id="send-btn">Send</button>
</div>

<script>
(function () {
    const vscode = acquireVsCodeApi();
    const chat = document.getElementById('chat');
    const input = document.getElementById('input');
    const sendBtn = document.getElementById('send-btn');
    const statusDot = document.getElementById('status-dot');
    const statusText = document.getElementById('status-text');

    // Current streaming bubble (reused until stream_end)
    let streamBubble = null;

    // ── Send user message ────────────────────────────────
    function send() {
        const text = input.value.trim();
        if (!text) return;
        appendMsg('user', text);
        vscode.postMessage({ type: 'send', text });
        input.value = '';
        autoResize();
    }

    sendBtn.addEventListener('click', send);
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            send();
        }
    });

    // Auto-resize textarea
    function autoResize() {
        input.style.height = 'auto';
        input.style.height = Math.min(input.scrollHeight, 120) + 'px';
    }
    input.addEventListener('input', autoResize);

    // ── Append a message bubble ──────────────────────────
    function appendMsg(cls, text) {
        const div = document.createElement('div');
        div.className = 'msg ' + cls;
        div.textContent = text;
        chat.appendChild(div);
        chat.scrollTop = chat.scrollHeight;
        return div;
    }

    // ── Handle events from the extension host ────────────
    window.addEventListener('message', (e) => {
        const msg = e.data;

        switch (msg.type) {
            // Connection status
            case 'status':
                statusDot.className = msg.status;
                statusText.textContent = msg.status === 'connected'
                    ? 'Connected' : 'Disconnected';
                if (msg.status === 'disconnected') {
                    appendMsg('system', '⚡ Disconnected');
                }
                break;

            case 'ready':
                appendMsg('system', '🤖 Agent ready (v' + (msg.version || '?') + ')');
                break;

            // Thinking indicator
            case 'thinking':
                appendMsg('thinking', '💭 Thinking…');
                break;

            // Streaming tokens
            case 'stream_start':
                streamBubble = appendMsg('assistant', '');
                break;

            case 'streaming_token':
                if (streamBubble) {
                    streamBubble.textContent += msg.token;
                    chat.scrollTop = chat.scrollHeight;
                }
                break;

            case 'stream_end':
                streamBubble = null;
                break;

            // Full assistant text (non-streamed)
            case 'assistant_text':
                appendMsg('assistant', msg.text);
                break;

            // Tool calls
            case 'tool_use': {
                const inputStr = typeof msg.input === 'string'
                    ? msg.input
                    : JSON.stringify(msg.input, null, 2);
                const label = toolIcon(msg.tool) + ' ' + msg.tool;
                const preview = inputStr.length > 200
                    ? inputStr.slice(0, 200) + '…' : inputStr;
                appendMsg('tool', label + '\\n' + preview);
                break;
            }

            case 'tool_result': {
                const cls = msg.is_error ? 'tool error' : 'tool';
                const prefix = msg.is_error ? '❌ ' : '✅ ';
                const output = (msg.output || '').length > 300
                    ? msg.output.slice(0, 300) + '…' : (msg.output || '');
                appendMsg(cls, prefix + msg.tool + '\\n' + output);
                break;
            }

            // Diff applied
            case 'diff':
                appendMsg('system', '📄 Diff applied: ' + msg.path);
                break;

            // Confirmation result
            case 'confirm_result': {
                const status = msg.approved
                    ? '<span class="approved">✅ Approved</span>'
                    : '<span class="denied">❌ Denied</span>';
                const div = appendMsg('confirm', '');
                div.innerHTML = status + ' — ' + escapeHtml(msg.action)
                    + ': ' + escapeHtml(msg.detail || '');
                break;
            }

            // Done
            case 'done':
                appendMsg('system', '✓ Done');
                break;

            // Warnings / errors
            case 'warning':
                appendMsg('system', '⚠️ ' + msg.message);
                break;

            case 'error':
                appendMsg('system', '🚨 ' + msg.message);
                break;

            case 'context_warning':
                appendMsg('context-warn',
                    '📊 Context usage: ' + msg.usage_percent + '%');
                break;
        }
    });

    // ── Helpers ──────────────────────────────────────────
    function toolIcon(name) {
        const icons = {
            read_file: '📖',
            write_file: '✏️',
            edit_file: '✏️',
            run_command: '🔨',
            list_dir: '📂',
            search: '🔍',
        };
        return icons[name] || '🔧';
    }

    function escapeHtml(s) {
        const el = document.createElement('span');
        el.textContent = s || '';
        return el.innerHTML;
    }
})();
</script>
</body>
</html>`;
    }
}
