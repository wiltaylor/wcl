/* Open-file tabs with a dirty dot and a close button (@forge/ui's Tabs has
   neither, so this is a small custom strip on the same token vocabulary).
   Closing a dirty tab confirms through a Forge Modal, not a native dialog. */

import { For, Show, createSignal } from 'solid-js';
import { X } from 'lucide-solid';
import { Button, Modal } from '@forge/ui';

import { buffers, active, closeBuffer, openFile } from '../state/buffers';

export default function TabStrip() {
  const name = (path) => path.split('/').pop();
  const [pendingClose, setPendingClose] = createSignal(null);

  const close = (e, path) => {
    e.stopPropagation();
    if (buffers.buffers[path]?.dirty) setPendingClose(path);
    else closeBuffer(path);
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
              <span class="dot" />
            </Show>
            {name(path)}
            <button type="button" class="close" onClick={(e) => close(e, path)} title="Close">
              <X size={14} strokeWidth={1.5} />
            </button>
          </div>
        )}
      </For>
      <Modal
        open={pendingClose() !== null}
        onClose={() => setPendingClose(null)}
        title="Unsaved changes"
        footer={
          <>
            <Button onClick={() => setPendingClose(null)}>Keep editing</Button>
            <Button
              variant="danger"
              onClick={() => {
                closeBuffer(pendingClose());
                setPendingClose(null);
              }}
            >
              Close without saving
            </Button>
          </>
        }
      >
        <p>
          <code>{pendingClose() && name(pendingClose())}</code> has unsaved changes. Closing the
          tab discards them.
        </p>
      </Modal>
    </div>
  );
}
