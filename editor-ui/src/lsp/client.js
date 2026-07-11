/* Minimal LSP client over the backend's /api/lsp WebSocket.
   One shared connection for the whole session — the server runs one fresh
   wcl_lsp instance per connection, and cross-file resolution (the root
   document) only works within one session. JSON-RPC messages travel as
   plain WS text frames; the backend adds/strips the LSP wire framing and
   injects `initializationOptions.root` into `initialize`, so this client
   stays transport-dumb. */

import { createSignal } from 'solid-js';

const CHANGE_DEBOUNCE_MS = 200;
const RECONNECT_MS = 2000;

const [status, setStatus] = createSignal('connecting'); // connecting|ready|closed
export { status as lspStatus };

export function isWcl(path) {
  return path.endsWith('.wcl');
}

/* The served directory's absolute path (from /api/files). Document URIs
   must resolve to REAL on-disk paths: the LSP overlays open buffers keyed
   by the URI's file path, so cross-file resolution through the root
   document only sees unsaved edits when the URIs are truthful. */
let workspaceRoot = '';
export function setWorkspaceRoot(root) {
  workspaceRoot = (root ?? '').replace(/\\/g, '/');
}

/** file:// URI of a repo-relative path. */
export function docUri(path) {
  const base = workspaceRoot.startsWith('/') ? workspaceRoot : `/${workspaceRoot}`;
  return `file://${encodeURI(`${base}/${path}`)}`;
}

class LspClient {
  constructor() {
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map(); // id → {resolve, reject}
    this.diagnosticsHandlers = new Map(); // uri → fn(lspDiagnostics)
    this.docs = new Map(); // uri → {text, version}
    this.changeTimers = new Map(); // uri → timeout id
    this.pendingChange = new Set(); // uris with an edit not yet sent
    this.ready = false;
    this.connect();
  }

  connect() {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${location.host}/api/lsp`);
    this.ws = ws;
    setStatus('connecting');
    ws.onopen = async () => {
      try {
        await this.request('initialize', {
          processId: null,
          rootUri: null, // the backend injects the served directory + root
          capabilities: {
            textDocument: {
              publishDiagnostics: {},
              hover: { contentFormat: ['plaintext', 'markdown'] },
              completion: { completionItem: { snippetSupport: false } },
            },
          },
        });
        this.notify('initialized', {});
        this.ready = true;
        setStatus('ready');
        // Re-open every tracked doc after a reconnect. didOpen carries the
        // latest text, so nothing is pending afterwards.
        for (const [uri, doc] of this.docs) {
          this.notify('textDocument/didOpen', {
            textDocument: { uri, languageId: 'wcl', version: doc.version, text: doc.text },
          });
        }
        this.pendingChange.clear();
      } catch {
        setStatus('closed');
      }
    };
    ws.onmessage = (ev) => this.onMessage(ev.data);
    ws.onclose = () => {
      this.ready = false;
      setStatus('closed');
      for (const p of this.pending.values()) p.reject(new Error('lsp connection closed'));
      this.pending.clear();
      setTimeout(() => this.connect(), RECONNECT_MS);
    };
    ws.onerror = () => ws.close();
  }

  onMessage(raw) {
    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }
    if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
      const p = this.pending.get(msg.id);
      if (p) {
        this.pending.delete(msg.id);
        if (msg.error) p.reject(new Error(msg.error.message ?? 'lsp error'));
        else p.resolve(msg.result);
      }
      return;
    }
    if (msg.method === 'textDocument/publishDiagnostics') {
      const { uri, diagnostics } = msg.params ?? {};
      this.diagnosticsHandlers.get(uri)?.(diagnostics ?? []);
      return;
    }
    // Server→client requests (none expected beyond capability negotiation):
    // answer with null so the server never hangs on us.
    if (msg.id !== undefined && msg.method) {
      this.send({ jsonrpc: '2.0', id: msg.id, result: null });
    }
  }

  send(obj) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(obj));
  }

  request(method, params) {
    return new Promise((resolve, reject) => {
      if (this.ws?.readyState !== WebSocket.OPEN) {
        reject(new Error('lsp not connected'));
        return;
      }
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.send({ jsonrpc: '2.0', id, method, params });
    });
  }

  notify(method, params) {
    this.send({ jsonrpc: '2.0', method, params });
  }

  onDiagnostics(uri, fn) {
    this.diagnosticsHandlers.set(uri, fn);
  }

  offDiagnostics(uri) {
    this.diagnosticsHandlers.delete(uri);
  }

  didOpen(uri, text) {
    this.docs.set(uri, { text, version: 1 });
    if (this.ready) {
      this.notify('textDocument/didOpen', {
        textDocument: { uri, languageId: 'wcl', version: 1, text },
      });
    }
  }

  /** Debounced full-document didChange. */
  scheduleChange(uri, text) {
    const doc = this.docs.get(uri);
    if (!doc) return;
    doc.text = text;
    this.pendingChange.add(uri);
    clearTimeout(this.changeTimers.get(uri));
    this.changeTimers.set(
      uri,
      setTimeout(() => this.flushChange(uri), CHANGE_DEBOUNCE_MS),
    );
  }

  /* No-op when nothing is pending: a didChange with unchanged text makes
     the server republish identical diagnostics, and that round-trip closes
     any open lint tooltip (setDiagnostics dismisses it) — the "tooltip
     flickers away on hover" bug, since requestAt flushes before every
     hover request. */
  flushChange(uri) {
    const doc = this.docs.get(uri);
    if (!doc || !this.ready || !this.pendingChange.has(uri)) return;
    this.pendingChange.delete(uri);
    doc.version += 1;
    this.notify('textDocument/didChange', {
      textDocument: { uri, version: doc.version },
      contentChanges: [{ text: doc.text }], // no range = full replacement
    });
  }

  didClose(uri) {
    clearTimeout(this.changeTimers.get(uri));
    this.changeTimers.delete(uri);
    this.pendingChange.delete(uri);
    this.docs.delete(uri);
    this.offDiagnostics(uri);
    if (this.ready) {
      this.notify('textDocument/didClose', { textDocument: { uri } });
    }
  }

  /** Request helper that flushes pending edits first so positions match. */
  async requestAt(method, uri, extraParams) {
    clearTimeout(this.changeTimers.get(uri));
    this.flushChange(uri);
    return this.request(method, { textDocument: { uri }, ...extraParams });
  }
}

export const lsp = new LspClient();
