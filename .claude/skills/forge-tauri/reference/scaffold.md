# Scaffolding a Forge Tauri app from examples/tauri-demo

Copy the whole `examples/tauri-demo` directory (minus `dist/`, `node_modules/`,
`src-tauri/target/`, `src-tauri/gen/`), then apply every row.

## Transform table

| Where | What to change |
|---|---|
| `package.json` → `name` | `@forge/tauri-demo` → the app's npm name (private) |
| `package.json` → `description` | Describe the app |
| `src-tauri/Cargo.toml` → `[package] name` | `tauri-demo` → app crate name (kebab-case) |
| `src-tauri/Cargo.toml` → `forge-tauri` dep | In-repo: `path = "../../../crates/forge-tauri"`. Standalone: `git = "https://github.com/wiltaylor/forge"`. Either way set `features` to exactly the widgets kept (`"widgets"` = all; or a list like `["term"]`; or drop the key for none) |
| `src-tauri/Cargo.toml` → `[patch.crates-io]` | Keep `ironrdp-session` ONLY if the `rdp` feature is kept. In-repo: `path = "../../../vendor/ironrdp-session"`. Standalone: `git = "https://github.com/wiltaylor/forge"` |
| `src-tauri/tauri.conf.json` → `productName`, `identifier` | App name + reverse-domain identifier (never ending `.app`) |
| `src-tauri/tauri.conf.json` → window `title` | App title |
| `src-tauri/src/main.rs` | `Builder::new("<app>")`; keep only the `with_*()` calls for kept widgets; replace the demo actions with the app's own |
| `src/sections/*` + `src/App.jsx` | Keep Overview as the data-plane starting point; drop TermTab/DesktopTab if their widgets were dropped (and the tab entries) |
| Frontend deps | Drop `@forge/term` / `@forge/desktop` if unused. Standalone: all `workspace:^` deps become `github:wiltaylor/forge#main&path:packages/<name>` git deps (each package has a `prepare` script) |
| `src-tauri/icons/` | Regenerate: `pnpm exec tauri icon <1024px source.png>` |
| `.gitignore` (app repo) | `node_modules/`, `dist/`, `src-tauri/target/`, `src-tauri/gen/` |

## Invariants that must survive the transform

- Vite port 1420 + `strictPort` ↔ `tauri.conf.json` `build.devUrl`.
- `capabilities/default.json`: `"permissions": ["core:default", "forge:default"]`,
  `"windows": ["main"]`.
- `beforeDevCommand: "pnpm dev"`, `beforeBuildCommand: "pnpm build"`,
  `frontendDist: "../dist"`.
- `src-tauri/Cargo.toml` keeps its empty `[workspace]` table — the app must
  not join a parent cargo workspace by accident (and inside the forge repo
  the root `exclude` list must name it).
- `main.jsx` CSS import order: tokens fonts → tokens → base → ui → (term)
  → (desktop).
- `<Terminal webgl={false}>` — xterm's WebGL addon composites a black canvas
  under webkitgtk; the DOM renderer works. Keep the prop when copying TermTab.
- Widget tabs mount lazily on first activation (App.jsx `visited` pattern):
  xterm.js cannot initialize inside a `display:none` container.

## Widget feature cost notes

- `term` = portable-pty (cheap). `term-ssh` adds russh (moderate).
- `vnc` = vnc-rs (cheap). `rdp` = the ironrdp stack + rustls (expensive,
  and requires the patch row above).
- The first build compiles ~600 crates either way (tauri itself); widget
  trimming matters on rebuilds and CI.
