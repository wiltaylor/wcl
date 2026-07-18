/* One CodeEditor per open buffer, all kept mounted (hidden when inactive)
   so each file retains its own undo history, cursor, and LSP document
   binding. The active buffer's LSP diagnostics arrive through the store as
   CodeAnnotation[] and render via CodeEditor's own `annotations` prop —
   the only diagnostics path that survives its per-value setDiagnostics. */

import { For, Show, onCleanup } from 'solid-js';
import { CodeEditor } from '@forge/code';

import { buffers, active, editBuffer, setAnnotations } from '../state/buffers';
import { registerView } from '../state/views';
import { wclLanguage } from '../lang/wcl';
import { wclLspExtensions, toAnnotations } from '../lsp/extension';
import { lsp, isWcl, docUri } from '../lsp/client';

const EXT_LANG = {
  js: 'js', mjs: 'js', cjs: 'js', jsx: 'jsx', ts: 'ts', tsx: 'tsx',
  py: 'python', json: 'json', css: 'css', html: 'html', htm: 'html',
  sh: 'shell', bash: 'shell',
};

function languageFor(path) {
  if (isWcl(path)) {
    // Arrays flatten: smuggle the whole LSP stack (and the view registry
    // the edit_object reveal uses) through `language`.
    return [wclLanguage, wclLspExtensions(docUri(path)), registerView(path)];
  }
  return EXT_LANG[path.split('.').pop()];
}

function Buffer(props) {
  const path = props.path;
  if (isWcl(path)) {
    const uri = docUri(path);
    lsp.onDiagnostics(uri, (diags) => setAnnotations(path, toAnnotations(diags)));
    onCleanup(() => lsp.offDiagnostics(uri));
  }
  if (buffers.buffers[path]?.binary) {
    return (
      <div class="ed-buffer" hidden={active() !== path}>
        <div class="ed-empty">Binary file — shown in the preview pane</div>
      </div>
    );
  }
  return (
    <div class="ed-buffer" hidden={active() !== path}>
      <CodeEditor
        value={buffers.buffers[path]?.text ?? ''}
        onChange={(text) => editBuffer(path, text)}
        language={languageFor(path)}
        annotations={buffers.buffers[path]?.annotations ?? []}
        height="100%"
      />
    </div>
  );
}

export default function EditorPane() {
  return (
    <div class="ed-editor-host">
      <For each={buffers.order}>{(path) => <Buffer path={path} />}</For>
      <Show when={buffers.order.length === 0}>
        <div class="ed-empty">Select a file to start editing</div>
      </Show>
    </div>
  );
}
