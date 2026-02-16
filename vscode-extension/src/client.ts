/**
 * WebSocket client that connects to the Agent server.
 *
 * Handles reconnection, JSON parsing, and message framing.
 */

import WebSocket from 'ws';

export interface AgentClientCallbacks {
    onConnected: () => void;
    onDisconnected: () => void;
    onEvent: (event: any) => Promise<void>;
    onError: (err: string) => void;
}

export class AgentClient {
    private ws: WebSocket | undefined;
    private url: string;
    private callbacks: AgentClientCallbacks;
    private _isConnected = false;

    constructor(url: string, callbacks: AgentClientCallbacks) {
        this.url = url;
        this.callbacks = callbacks;
    }

    get isConnected(): boolean {
        return this._isConnected;
    }

    /** Open the WebSocket connection. */
    connect() {
        if (this._isConnected) {
            return;
        }

        try {
            this.ws = new WebSocket(this.url);
        } catch (e: any) {
            this.callbacks.onError(`Failed to create WebSocket: ${e.message}`);
            return;
        }

        this.ws.on('open', () => {
            this._isConnected = true;
            this.callbacks.onConnected();
        });

        this.ws.on('message', async (raw: WebSocket.RawData) => {
            const text = raw.toString();
            // The server may send multiple JSON objects in one frame (unlikely but safe)
            for (const line of text.split('\n')) {
                const trimmed = line.trim();
                if (!trimmed) { continue; }
                try {
                    const event = JSON.parse(trimmed);
                    await this.callbacks.onEvent(event);
                } catch {
                    // Not JSON — ignore
                }
            }
        });

        this.ws.on('close', () => {
            this._isConnected = false;
            this.callbacks.onDisconnected();
        });

        this.ws.on('error', (err: Error) => {
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
    sendUserMessage(text: string, id?: number) {
        this.send({
            type: 'user_message',
            data: { text },
            ...(id !== undefined ? { id } : {}),
        });
    }

    /** Send a confirmation response. */
    sendConfirmResponse(approved: boolean) {
        this.send({
            type: 'confirm_response',
            data: { approved },
        });
    }

    /** Send a raw JSON message. */
    private send(msg: any) {
        if (!this._isConnected || !this.ws) {
            return;
        }
        this.ws.send(JSON.stringify(msg));
    }
}
