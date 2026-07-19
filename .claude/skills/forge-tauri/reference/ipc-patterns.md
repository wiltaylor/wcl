# Tauri IPC primer (as used by forge-tauri)

The plugin uses all four Tauri v2 IPC building blocks. Knowing which is
which makes invoke-permission failures and event plumbing debuggable.

## Commands (webview → Rust, request/response)

`@forge/tauri` funnels the whole data plane through ONE command:

```
invoke('plugin:forge|request', { method, path, body })
  → { status: number, body: <Forge envelope> }
```

`bridge::handle()` routes `path` exactly like forge-server's axum router
(health, auth/me, auth/login→404, data CRUD, actions). Widgets add four
more commands: `widget_open`, `widget_send_text`, `widget_send_binary`,
`widget_close`.

## Permissions / capabilities (the triad)

Plugin commands are deny-by-default. Three pieces must line up:

1. **Plugin build script** (`crates/forge-tauri/build.rs`): `COMMANDS`
   lists every command → autogenerates `allow-*` permissions.
2. **Plugin permission set** (`crates/forge-tauri/permissions/default.toml`):
   `default` bundles all five `allow-*` permissions.
3. **App capability** (`src-tauri/capabilities/default.json`):
   `"permissions": ["core:default", "forge:default"]` grants that set to
   the `main` window.

Symptom of a broken triad: the invoke rejects with
`"<command> not allowed"` in the webview console. The ACL plugin name
("forge") comes from the crate's `links = "tauri-plugin-forge"` key on the
app side and must match the runtime `tauri::plugin::Builder::new("forge")`.

## Events (Rust → webview, broadcast)

The Forge `EventBus` is bridged to a single Tauri event:

```
emit("forge://event", { topic, data })
```

`@forge/tauri`'s `events.on(topic, cb)` opens one shared
`listen('forge://event')` and filters by topic client-side — mirroring how
the web client shares one SSE connection. Nothing to configure per topic.

## Channels (Rust → webview, per-session streams)

Tauri `ipc::Channel` is one-directional (Rust → JS) and ordered. A widget
session is therefore a pair:

- **Down**: `widget_open(kind, onMessage: Channel)` — the engine's frames
  ride the channel. Control JSON is sent as a JSON string (JS receives a
  string), payload bytes are sent raw (JS receives an ArrayBuffer), and
  JSON `null` means the session closed. This preserves the widget
  protocol's load-bearing string-vs-binary frame discriminator.
- **Up**: `widget_send_text` / `widget_send_binary` commands feed a
  bounded tokio mpsc inbox (backpressure = the WebSocket send buffer
  equivalent). `widget_close` (or plugin teardown) drops the sender —
  the engine sees end-of-stream, same as a dropped socket.

`@forge/tauri` chains all sends of one transport on a promise queue:
independent `invoke()` calls may otherwise resolve out of order, and tty
keystrokes must not reorder.

## State

`Builder::build()` stores one `ForgeState` (docstore, action registry,
event bus, widget configs, live session map) via `app.manage()` in the
plugin `setup` hook; commands take `State<'_, ForgeState>`.
