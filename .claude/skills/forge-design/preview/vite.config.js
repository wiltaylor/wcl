import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import { fileURLToPath } from 'node:url';

const r = (p) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      // The gallery previews the live skill assets — no copies.
      '@forge': r('../assets'),
      // ../assets/*.jsx have no node_modules on their walk-up path; pin their
      // bare imports (prefix-matched) to the preview's copies.
      'solid-js': r('node_modules/solid-js'),
      '@codemirror': r('node_modules/@codemirror'),
      '@lezer': r('node_modules/@lezer'),
    },
    dedupe: ['solid-js'],
  },
  server: { port: 4890, strictPort: true, fs: { allow: [r('..')] } },
});
