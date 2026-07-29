import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

// Dev loop: `wcl editor --addr 127.0.0.1:8877` in one terminal, `pnpm dev`
// here in another — the proxy carries the REST endpoints and the /api/lsp
// WebSocket. Production embeds `dist/` into the wcl binary (build.rs).
const apiTarget = `http://127.0.0.1:${process.env.WCL_EDITOR_PORT ?? 8877}`;

export default defineConfig({
  base: '/',
  plugins: [solid()],
  resolve: {
    // Load-bearing: @forge/code ships preserved-JSX source importing
    // @codemirror/* bare — our injected LSP/language extensions must
    // resolve to the SAME module instances or CodeMirror facets and
    // instanceof checks silently break.
    dedupe: [
      'solid-js',
      '@codemirror/state',
      '@codemirror/view',
      '@codemirror/language',
      '@codemirror/lint',
      '@codemirror/autocomplete',
      '@codemirror/commands',
      '@codemirror/search',
      '@lezer/common',
      '@lezer/highlight',
    ],
  },
  optimizeDeps: {
    // Dev-server counterpart of the dedupe list: without this, our direct
    // @codemirror imports get prebundled into .vite/deps while @forge/code's
    // (served as preserved-JSX source) resolve to the raw files — two
    // @codemirror/state instances, and every injected extension fails
    // "Unrecognized extension value" instanceof checks. Production (rollup)
    // needs only `dedupe`.
    exclude: [
      '@codemirror/state',
      '@codemirror/view',
      '@codemirror/language',
      '@codemirror/lint',
      '@codemirror/autocomplete',
      '@codemirror/commands',
      '@codemirror/search',
      '@lezer/common',
      '@lezer/highlight',
    ],
  },
  server: {
    port: 5174,
    strictPort: true,
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true, ws: true },
    },
  },
  test: {
    environment: 'happy-dom',
    // Node ≥22's experimental `localStorage` global shadows happy-dom's —
    // the setup file swaps in a working in-memory Storage.
    setupFiles: ['./vitest.setup.js'],
  },
});
