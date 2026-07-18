/* Bottom status bar: active path, dirty marker, diagnostic counts, LSP
   connection dot, and format/save actions. */

import { Show } from 'solid-js';
import { CircleX, TriangleAlert } from 'lucide-solid';
import { StatusDot, toast } from '@forge/ui';

import { api } from '../api';
import { buffers, active, editBuffer, saveBuffer } from '../state/buffers';
import { lspStatus, isWcl } from '../lsp/client';

export default function StatusBar(props) {
  const buf = () => (active() ? buffers.buffers[active()] : null);
  const counts = () => {
    const anns = buf()?.annotations ?? [];
    return {
      err: anns.filter((a) => a.severity === 'error').length,
      warn: anns.filter((a) => a.severity === 'warning').length,
    };
  };

  const format = async () => {
    const path = active();
    const b = buf();
    if (!path || !b || !isWcl(path)) return;
    const res = await api.format(b.text);
    if (res.ok) editBuffer(path, res.text);
    else toast(res.error, { tone: 'danger' });
  };

  const save = async () => {
    const path = active();
    if (!path) return;
    const res = await saveBuffer(path);
    if (res.ok) toast('Saved', { tone: 'success', duration: 1500 });
    else if (res.status !== 409) toast(res.error, { tone: 'danger', duration: 8000 });
  };

  const lspTone = () =>
    ({ ready: 'success', connecting: 'warning', closed: 'danger' })[lspStatus()] ?? 'warning';

  return (
    <div class="ed-status">
      <Show when={active()} fallback={<span>wcl editor</span>}>
        <span class="ed-status-path">
          {active()}
          <Show when={buf()?.dirty}>
            <span class="dot" />
          </Show>
        </span>
        <Show when={counts().err + counts().warn > 0}>
          <span class="ed-status-diags">
            <Show when={counts().err > 0}>
              <span class="diag-err">
                <CircleX size={12} strokeWidth={1.5} /> {counts().err}
              </span>
            </Show>
            <Show when={counts().warn > 0}>
              <span class="diag-warn">
                <TriangleAlert size={12} strokeWidth={1.5} /> {counts().warn}
              </span>
            </Show>
          </span>
        </Show>
      </Show>
      <span class="spacer" />
      <Show when={active() && isWcl(active())}>
        <button type="button" onClick={format}>Format</button>
      </Show>
      <Show when={active()}>
        <button type="button" onClick={save} title="Ctrl+S">Save</button>
      </Show>
      <button type="button" onClick={() => props.onTogglePreview?.()}>
        {props.previewOpen ? 'Hide preview' : 'Show preview'}
      </button>
      <span class="lsp" title={`LSP: ${lspStatus()}`}>
        <StatusDot tone={lspTone()} /> lsp
      </span>
    </div>
  );
}
