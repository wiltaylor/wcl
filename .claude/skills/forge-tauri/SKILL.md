---
name: forge-tauri
description: Scaffolds Tauri v2 desktop apps with a Forge UI and a Rust backend over pure IPC — the frozen Forge contract (doc store, actions, events) plus the streaming widgets (local-PTY terminal, VNC/RDP viewers) with no HTTP server inside the app. Copies examples/tauri-demo and transforms it. Use when the user wants a Tauri app, a native/desktop Forge app, "forge + tauri", or widgets (terminal/VNC/RDP) inside a desktop app.
user-invocable: true
argument-hint: <app name or description>
allowed-tools:
  - Read
  - Write
  - Edit
  - Glob
  - Grep
  - Bash
---

<overview>
Builds a Tauri v2 desktop app where the whole backend is the `forge-tauri`
plugin: a ~12-line `main.rs` gives the webview the Forge v1 API contract
(JSON doc store, named actions, live events) and optionally the streaming
widgets, all over Tauri IPC. The frontend is SolidJS + the Forge design
system, and the only difference from a Forge web app is the client import —
`@forge/tauri`'s `createClient()` implements the same `ForgeClient`
interface as `@forge/client`.

The scaffold is a copy-and-transform of `examples/tauri-demo` (the always-
working reference). `reference/scaffold.md` holds the transform table;
`reference/ipc-patterns.md` explains the Tauri IPC pieces (commands, events,
channels, capabilities) the plugin builds on.
</overview>

<variables>
- `${CLAUDE_SKILL_DIR}`: Path to this skill's directory.
- `$ARGUMENTS`: App name or description (may be empty — then ask).
- Reference app: `examples/tauri-demo` (this repo). Out-of-repo scaffolds
  swap workspace deps for git deps per `reference/scaffold.md`.
- Dev port: 1420 (Vite, must match `tauri.conf.json` devUrl).
</variables>

<workflow>
<step order="1">
If `$ARGUMENTS` is empty and no task is in context, ask what to build. You
need: an app name (kebab-case), a reverse-domain identifier (e.g.
`dev.example.myapp` — NEVER ending in `.app`, that breaks macOS bundles),
and which widgets the app needs (term / vnc / rdp / none).
</step>

<step order="2">
Preflight: `rustc --version && pnpm --version`. On Linux also confirm
webkit2gtk-4.1 is installed (`pkg-config --exists webkit2gtk-4.1` or the
distro package list). Missing webkit = the app cannot build; stop and tell
the user the package names (Arch: `webkit2gtk-4.1 gtk3 librsvg`).
</step>

<step order="3">
Copy `examples/tauri-demo` into the target and apply the transform table in
`${CLAUDE_SKILL_DIR}/reference/scaffold.md`: names, identifier, dependency
sources (workspace `workspace:^` + path deps in-repo; git deps outside),
and trim the widget features/tabs the app does not need — unused widget
features dominate compile time.
</step>

<step order="4">
Widgets decide two extra pieces: keep the matching `with_*()` builder calls
in `main.rs` and the matching features on the `forge-tauri` dependency. If
`rdp` is kept, the app's `src-tauri/Cargo.toml` MUST carry the
`[patch.crates-io] ironrdp-session` entry (it is its own workspace — the
forge root patch does not reach it). No widgets → drop the features, the
patch entry, and the Terminal/Desktop tabs.
</step>

<step order="5">
Icons: generate a 1024×1024 PNG source, then
`pnpm exec tauri icon <source.png>` inside the app dir; commit the generated
`src-tauri/icons/` (drop `android/`/`ios/` unless building mobile).
</step>

<step order="6">
Verify: `pnpm install`, then `pnpm tauri dev` — a native window must open
with the Overview tab live (health stats answer over IPC). If invokes fail
with a permissions error, re-check the capability file against
`reference/ipc-patterns.md`. Data persists under
`~/.local/share/<identifier>/data/` — prove it by writing a note,
restarting, and reading it back.
</step>
</workflow>

<boundaries>
<always>
- Scaffold by copying `examples/tauri-demo`, then transform — never from
  memory
- Keep `capabilities/default.json` permissions at
  `["core:default", "forge:default"]`
- Keep the ironrdp `[patch.crates-io]` entry whenever the `rdp` feature is
  on, and say why in a comment
- Use the factory form for widget transports:
  `transport={() => api.widget('term')}`
- Keep Vite on port 1420 with `strictPort: true` (must match devUrl)
</always>

<ask>
- App name / identifier if not provided
- Which widgets to keep (each one costs compile time; rdp also drags TLS)
- Whether the target is inside this repo (workspace deps) or standalone
  (git deps)
</ask>

<never>
- Edit `docs/api-contract.md` or `docs/widgets-protocol.md` — both are
  frozen; forge-tauri conforms to them, never the other way around
- Embed forge-server (axum/HTTP) inside a Tauri app — the plugin exists so
  apps are pure IPC
- Add forge-tauri or an app's src-tauri to the forge root cargo workspace
  (they stay excluded; tauri would drag webkit into every root build)
- Enable `with_term()` in an app that isn't a trusted dev tool — it hands
  the webview a real shell (RCE by design)
</never>
</boundaries>
