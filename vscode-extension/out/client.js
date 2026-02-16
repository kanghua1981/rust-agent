"use strict";
/**
 * WebSocket client that connects to the Agent server.
 *
 * Handles reconnection, JSON parsing, and message framing.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.AgentClient = void 0;
const ws_1 = __importDefault(require("ws"));
class AgentClient {
    ws;
    url;
    callbacks;
    _isConnected = false;
    constructor(url, callbacks) {
        this.url = url;
        this.callbacks = callbacks;
    }
    get isConnected() {
        return this._isConnected;
    }
    /** Open the WebSocket connection. */
    connect() {
        if (this._isConnected) {
            return;
        }
        try {
            this.ws = new ws_1.default(this.url);
        }
        catch (e) {
            this.callbacks.onError(`Failed to create WebSocket: ${e.message}`);
            return;
        }
        this.ws.on('open', () => {
            this._isConnected = true;
            this.callbacks.onConnected();
        });
        this.ws.on('message', async (raw) => {
            const text = raw.toString();
            // The server may send multiple JSON objects in one frame (unlikely but safe)
            for (const line of text.split('\n')) {
                const trimmed = line.trim();
                if (!trimmed) {
                    continue;
                }
                try {
                    const event = JSON.parse(trimmed);
                    await this.callbacks.onEvent(event);
                }
                catch {
                    // Not JSON — ignore
                }
            }
        });
        this.ws.on('close', () => {
            this._isConnected = false;
            this.callbacks.onDisconnected();
        });
        this.ws.on('error', (err) => {
            this._isConnected = false;
            this.callbacks.onError(err.message);
        });
    }
    /** Close the WebSocket connection. */
    disconnect() {
        this._isConnected = false;
        this.ws?.close();
        this.ws = undefined;
    }
    /** Send a user message to the Agent. */
    sendUserMessage(text, id) {
        this.send({
            type: 'user_message',
            data: { text },
            ...(id !== undefined ? { id } : {}),
        });
    }
    /** Send a confirmation response. */
    sendConfirmResponse(approved) {
        this.send({
            type: 'confirm_response',
            data: { approved },
        });
    }
    /** Send a raw JSON message. */
    send(msg) {
        if (!this._isConnected || !this.ws) {
            return;
        }
        this.ws.send(JSON.stringify(msg));
    }
}
exports.AgentClient = AgentClient;
//# sourceMappingURL=client.js.map