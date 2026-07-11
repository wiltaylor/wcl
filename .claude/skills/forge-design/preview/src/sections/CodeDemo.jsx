import { createSignal } from 'solid-js';
import { PageHead, Card, Toggle, toast } from '@forge/ui.jsx';
import { CodeEditor, DiffEditor } from '@forge/code.jsx';

const SAMPLE = `import { createSignal } from 'solid-js';

/** Poll a node's health endpoint. */
export function useHealth(url, intervalMs = 5000) {
  const [status, setStatus] = createSignal('unknown');
  const tick = async () => {
    try {
      const res = await fetch(url);
      setStatus(res.ok ? 'healthy' : 'degraded');
    } catch (err) {
      setStatus('down');   // network error
    }
  };
  const timer = setInterval(tick, interval);
  tick();
  return status;
}
`;

const ANNOTATIONS = [
  { from: { line: 14, col: 34 }, to: { line: 14, col: 42 }, severity: 'error',
    message: "'interval' is not defined — did you mean 'intervalMs'?", source: 'eslint(no-undef)' },
  { from: { line: 14, col: 8 }, to: { line: 14, col: 13 }, severity: 'warning',
    message: "'timer' is assigned but never cleared — possible leak.", source: 'eslint(no-unused-vars)' },
  { from: { line: 3, col: 0 }, to: { line: 3, col: 40 }, severity: 'info',
    message: 'JSDoc: consider documenting the return signal.', source: 'tsserver' },
];

const ORIGINAL = `export function retry(fn, attempts) {
  for (let i = 0; i < attempts; i++) {
    try {
      return fn();
    } catch (err) {
      console.log(err);
    }
  }
  throw new Error('retry failed');
}
`;

const MODIFIED = `export async function retry(fn, attempts, delayMs = 250) {
  let lastErr;
  for (let i = 0; i < attempts; i++) {
    try {
      return await fn();
    } catch (err) {
      lastErr = err;
      await new Promise((r) => setTimeout(r, delayMs * 2 ** i));
    }
  }
  throw lastErr;
}
`;

export default function CodeDemo() {
  const [source, setSource] = createSignal(SAMPLE);
  const [unified, setUnified] = createSignal(false);

  const menuItems = [
    { label: 'Copy selection', kbd: '⌘C',
      onSelect: (view) => {
        const sel = view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);
        navigator.clipboard?.writeText(sel);
        toast('Copied selection', { tone: 'success' });
      } },
    { label: 'Select all', kbd: '⌘A',
      onSelect: (view) => view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } }) },
    { separator: true },
    { label: 'Delete line', danger: true,
      onSelect: (view) => {
        const line = view.state.doc.lineAt(view.state.selection.main.head);
        view.dispatch({ changes: { from: line.from, to: Math.min(line.to + 1, view.state.doc.length) } });
      } },
  ];

  return (
    <>
      <PageHead title="Code" sub="CodeMirror 6 in Forge clothes — viewer with LSP-style annotations, editable editor with context menu, diff views" />

      <Card title="Read-only viewer with diagnostics (hover the squiggles)" padded={false}>
        <CodeEditor value={SAMPLE} language="js" readOnly annotations={ANNOTATIONS} height="320px" />
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Editable (right-click for the context menu)" padded={false}
            action={<span style={{ 'font-size': '12px', color: 'var(--fg-2)', 'font-family': 'var(--font-mono)' }}>
              {source().length} chars
            </span>}>
        <CodeEditor value={source()} onChange={setSource} language="js"
                    contextMenuItems={menuItems} height="240px" />
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Diff" padded={false}
            action={<Toggle checked={unified()} onChange={setUnified}>Unified</Toggle>}>
        <DiffEditor original={ORIGINAL} modified={MODIFIED} language="js"
                    unified={unified()} height="300px" />
      </Card>
    </>
  );
}
