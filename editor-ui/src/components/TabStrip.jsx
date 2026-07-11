/* Open-file tabs with a dirty dot and a close button (@forge/ui's Tabs has
   neither, so this is a small custom strip on the same token vocabulary). */

import { For, Show } from 'solid-js';

import { buffers, active, closeBuffer, openFile } from '../state/buffers';

export default function TabStrip() {
  const name = (path) => path.split('/').pop();

  const close = (e, path) => {
    e.stopPropagation();
    if (buffers.buffers[path]?.dirty && !confirm(`${name(path)} has unsaved changes — close anyway?`)) {
      return;
    }
    closeBuffer(path);
  };

  return (
    <div class="ed-tabs" role="tablist">
      <For each={buffers.order}>
        {(path) => (
          <div
            class="ed-tab"
            classList={{ 'is-active': active() === path }}
            role="tab"
            onClick={() => openFile(path)}
            title={path}
          >
            <Show when={buffers.buffers[path]?.dirty}>
              <span class="dot">●</span>
            </Show>
            {name(path)}
            <button type="button" class="close" onClick={(e) => close(e, path)} title="Close">
              ×
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
