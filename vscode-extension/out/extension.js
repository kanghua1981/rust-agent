"use strict";
/**
 * Rust Coding Agent — VS Code Extension
 *
 * Connects to the Agent WebSocket server and provides:
 *  - Chat panel (Webview) for streaming conversation
 *  - Native VS Code diff viewer for file changes
 *  - Confirmation dialogs for dangerous operations
 *  - Output channel for raw event log
 *  - Server lifecycle management (auto-start with --workdir)
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
const client_1 = require("./client");
const chatView_1 = require("./chatView");
let client;
let chatProvider;
let outputChannel;
let serverProcess;
let serverOutputChannel;
function activate(context) {
    outputChannel = vscode.window.createOutputChannel('Rust Agent');
    serverOutputChannel = vscode.window.createOutputChannel('Rust Agent Server');
    // Chat webview in the sidebar
    chatProvider = new chatView_1.ChatViewProvider(context.extensionUri, {
        onSendMessage: (text) => {
            if (!client?.isConnected) {
                vscode.window.showWarningMessage('Not connected to Agent server. Run "Agent: Connect" first.');
                return;
            }
            client.sendUserMessage(text);
        },
    });
    context.subscriptions.push(vscode.window.registerWebviewViewProvider('rustAgent.chatView', chatProvider));
    // ── Commands ────────────────────────────────────────────────
    context.subscriptions.push(vscode.commands.registerCommand('rustAgent.startServer', () => startServer()), vscode.commands.registerCommand('rustAgent.stopServer', () => stopServer()), vscode.commands.registerCommand('rustAgent.connect', () => connectToAgent()), vscode.commands.registerCommand('rustAgent.disconnect', () => disconnectAgent()), vscode.commands.registerCommand('rustAgent.sendMessage', async () => {
        const text = await vscode.window.showInputBox({
            prompt: 'Send a message to the Agent',
            placeHolder: '帮我重构 main.rs ...',
        });
        if (text) {
            if (!client?.isConnected) {
                await connectToAgent();
            }
            client?.sendUserMessage(text);
        }
    }));
    // Auto-connect if configured
    const config = vscode.workspace.getConfiguration('rustAgent');
    if (config.get('autoConnect')) {
        connectToAgent();
    }
}
/** Get the workspace folder path (first folder or undefined). */
function getWorkspaceDir() {
    return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}
/** Start the Agent server as a child process. */
async function startServer() {
    if (serverProcess && !serverProcess.killed) {
        outputChannel.appendLine('[server] Already running');
        return true;
    }
    const cfg = vscode.workspace.getConfiguration('rustAgent');
    const agentPath = cfg.get('agentPath', 'agent');
    const host = cfg.get('host', '127.0.0.1');
    const port = cfg.get('port', 9527);
    const model = cfg.get('model', 'claude-sonnet-4-20250514');
    const provider = cfg.get('provider', 'anthropic');
    const workdir = getWorkspaceDir();
    if (!workdir) {
        vscode.window.showErrorMessage('No workspace folder open. The agent needs a project directory.');
        return false;
    }
    const args = [
        '--mode', 'server',
        '--host', host,
        '--port', port.toString(),
        '--model', model,
        '--provider', provider,
        '--workdir', workdir,
        '--yes', // auto-approve in VS Code mode (we handle confirms via UI)
    ];
    serverOutputChannel.appendLine(`$ ${agentPath} ${args.join(' ')}`);
    serverOutputChannel.appendLine(`  workdir: ${workdir}`);
    serverOutputChannel.show(true);
    try {
        serverProcess = (0, child_process_1.spawn)(agentPath, args, {
            cwd: workdir,
            stdio: ['ignore', 'pipe', 'pipe'],
            env: { ...process.env },
        });
    }
    catch (e) {
        vscode.window.showErrorMessage(`Failed to start agent: ${e.message}\n\nMake sure "rustAgent.agentPath" is set correctly.`);
        return false;
    }
    serverProcess.stdout?.on('data', (chunk) => {
        serverOutputChannel.append(chunk.toString());
    });
    serverProcess.stderr?.on('data', (chunk) => {
        serverOutputChannel.append(chunk.toString());
    });
    serverProcess.on('error', (err) => {
        vscode.window.showErrorMessage(`Agent server error: ${err.message}`);
        serverOutputChannel.appendLine(`[error] ${err.message}`);
        serverProcess = undefined;
    });
    serverProcess.on('exit', (code, signal) => {
        serverOutputChannel.appendLine(`[exit] code=${code ?? 'null'} signal=${signal ?? 'null'}`);
        serverProcess = undefined;
        // If client is connected, it will get a disconnect event from the WS layer
    });
    // Wait a moment for the server to start listening
    await new Promise((resolve) => setTimeout(resolve, 800));
    if (serverProcess?.killed || !serverProcess) {
        vscode.window.showErrorMessage('Agent server exited immediately. Check the Server output channel.');
        return false;
    }
    vscode.window.showInformationMessage(`🤖 Agent server started on ws://${host}:${port} (workdir: ${workdir})`);
    return true;
}
/** Stop the Agent server process. */
function stopServer() {
    if (!serverProcess || serverProcess.killed) {
        vscode.window.showInformationMessage('Agent server is not running.');
        return;
    }
    serverProcess.kill('SIGTERM');
    serverProcess = undefined;
    serverOutputChannel.appendLine('[stopped]');
    vscode.window.showInformationMessage('Agent server stopped.');
}
/** Connect to the Agent WebSocket server (auto-starts if configured). */
async function connectToAgent() {
    if (client?.isConnected) {
        vscode.window.showInformationMessage('Already connected to Agent server.');
        return;
    }
    const cfg = vscode.workspace.getConfiguration('rustAgent');
    const autoStart = cfg.get('autoStart', true);
    // Auto-start the server if it's not already running
    if (autoStart && (!serverProcess || serverProcess.killed)) {
        const started = await startServer();
        if (!started) {
            return;
        }
    }
    const host = cfg.get('host', '127.0.0.1');
    const port = cfg.get('port', 9527);
    const url = `ws://${host}:${port}`;
    client = new client_1.AgentClient(url, {
        onConnected: () => {
            vscode.window.showInformationMessage(`🤖 Connected to Agent at ${url}`);
            chatProvider.postMessage({ type: 'status', status: 'connected' });
            outputChannel.appendLine(`[connected] ${url}`);
        },
        onDisconnected: () => {
            vscode.window.showWarningMessage('Agent connection closed.');
            chatProvider.postMessage({ type: 'status', status: 'disconnected' });
            outputChannel.appendLine('[disconnected]');
        },
        onEvent: async (event) => {
            outputChannel.appendLine(JSON.stringify(event));
            await handleAgentEvent(event);
        },
        onError: (err) => {
            vscode.window.showErrorMessage(`Agent connection error: ${err}`);
            outputChannel.appendLine(`[error] ${err}`);
        },
    });
    client.connect();
}
/** Disconnect from the Agent server. */
function disconnectAgent() {
    client?.disconnect();
    client = undefined;
    chatProvider.postMessage({ type: 'status', status: 'disconnected' });
}
/** Route an Agent event to the appropriate VS Code API. */
async function handleAgentEvent(event) {
    const { type, data } = event;
    switch (type) {
        // ── Streaming text ────────────────────────────────────
        case 'thinking':
            chatProvider.postMessage({ type: 'thinking' });
            break;
        case 'stream_start':
            chatProvider.postMessage({ type: 'stream_start' });
            break;
        case 'streaming_token':
            chatProvider.postMessage({ type: 'streaming_token', token: data.token });
            break;
        case 'stream_end':
            chatProvider.postMessage({ type: 'stream_end' });
            break;
        case 'assistant_text':
            chatProvider.postMessage({ type: 'assistant_text', text: data.text });
            break;
        // ── Tools ─────────────────────────────────────────────
        case 'tool_use':
            chatProvider.postMessage({
                type: 'tool_use',
                tool: data.tool,
                input: data.input,
            });
            break;
        case 'tool_result':
            chatProvider.postMessage({
                type: 'tool_result',
                tool: data.tool,
                output: data.output,
                is_error: data.is_error,
            });
            break;
        // ── Diff preview ──────────────────────────────────────
        case 'diff':
            chatProvider.postMessage({
                type: 'diff',
                path: data.path,
            });
            // Also show in the native diff editor if possible
            showDiffInEditor(data.path, data.diff);
            break;
        // ── Confirmation ──────────────────────────────────────
        case 'confirm_request':
            await handleConfirmRequest(data);
            break;
        // ── Completion ────────────────────────────────────────
        case 'done':
            chatProvider.postMessage({ type: 'done', text: data.text });
            break;
        // ── Diagnostics ───────────────────────────────────────
        case 'warning':
            vscode.window.showWarningMessage(`Agent: ${data.message}`);
            chatProvider.postMessage({ type: 'warning', message: data.message });
            break;
        case 'error':
            vscode.window.showErrorMessage(`Agent: ${data.message}`);
            chatProvider.postMessage({ type: 'error', message: data.message });
            break;
        case 'context_warning':
            chatProvider.postMessage({
                type: 'context_warning',
                usage_percent: data.usage_percent,
            });
            break;
        case 'ready':
            chatProvider.postMessage({ type: 'ready', version: data.version });
            break;
    }
}
/** Show a VS Code native confirmation dialog. */
async function handleConfirmRequest(data) {
    const action = data.action || 'unknown';
    const detail = data.path || data.command || '';
    const icon = action === 'run_command' ? '⚡' : action.includes('write') ? '📝' : '🔧';
    const message = `${icon} Agent wants to ${action.replace('_', ' ')}: ${detail}`;
    const choice = await vscode.window.showWarningMessage(message, { modal: true }, 'Approve', 'Deny');
    const approved = choice === 'Approve';
    client?.sendConfirmResponse(approved);
    chatProvider.postMessage({
        type: 'confirm_result',
        approved,
        action,
        detail,
    });
}
/** Show a diff in the VS Code native diff editor. */
function showDiffInEditor(filePath, diffText) {
    // Best-effort: show the diff in output channel
    // (Full native diff would require creating temp files from old/new content)
    outputChannel.appendLine(`\n─── Diff: ${filePath} ───`);
    outputChannel.appendLine(diffText);
    outputChannel.appendLine('─'.repeat(40));
}
function deactivate() {
    client?.disconnect();
    // Kill the server process if we started it
    if (serverProcess && !serverProcess.killed) {
        serverProcess.kill('SIGTERM');
        serverProcess = undefined;
    }
    outputChannel?.dispose();
    serverOutputChannel?.dispose();
}
//# sourceMappingURL=extension.js.map