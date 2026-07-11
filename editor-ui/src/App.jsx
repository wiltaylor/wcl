/* Layout composition: a Forge AppShell — "WCL" brand + site picker +
   Rebuild in the topbar, the file tree in the sidebar (mobile drawer for
   free), and the main area holding SplitPane(editor | preview) over the
   status bar — plus global Ctrl+S and the save conflict modal. */

import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { AppShell, Button, Modal, SplitPane, Spinner, Toaster, toast } from '@forge/ui';

import FileTree from './components/FileTree';
import TabStrip from './components/TabStrip';
import EditorPane from './components/EditorPane';
import PreviewPane from './components/PreviewPane';
import SitePicker from './components/SitePicker';
import StatusBar from './components/StatusBar';
import {
  active,
  conflict,
  dismissConflict,
  reloadFromDisk,
  saveBuffer,
} from './state/buffers';
import { treeData } from './state/tree';
import { building, loadSites, rebuild, selected } from './state/sites';

export default function App() {
  const [previewOpen, setPreviewOpen] = createSignal(true);

  const saveActive = async () => {
    const path = active();
    if (!path) return;
    const res = await saveBuffer(path);
    if (res.ok) toast('Saved', { tone: 'success', duration: 1500 });
    else if (res.status !== 409) toast(res.error, { tone: 'danger', duration: 8000 });
  };

  onMount(() => {
    const onKey = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        saveActive();
      }
    };
    window.addEventListener('keydown', onKey);
    onCleanup(() => window.removeEventListener('keydown', onKey));
  });

  // Discover sites once the tree has loaded (it supplies the workspace
  // root that keys the persisted selection).
  let sitesLoaded = false;
  createEffect(() => {
    if (treeData() && !sitesLoaded) {
      sitesLoaded = true;
      loadSites();
    }
  });

  const runRebuild = async () => {
    const res = await rebuild();
    if (!res.ok) toast('Build failed — see the preview pane', { tone: 'danger', duration: 4000 });
  };

  const editorColumn = (
    <div class="ed-editor-col">
      <div class="ed-topbar">
        <TabStrip />
      </div>
      <EditorPane />
    </div>
  );

  return (
    <AppShell
      topbar={
        <>
          <div class="ftopbar-brand">
            <strong>WCL</strong>
            <span class="ed-brand-sub">editor</span>
          </div>
          <SitePicker />
          <Button size="sm" onClick={runRebuild} disabled={!selected() || building()}>
            Rebuild
          </Button>
          <Show when={building()}>
            <Spinner size={16} label="Building preview" />
          </Show>
          <div style={{ flex: 1 }} />
        </>
      }
      sidebar={<FileTree />}
    >
      <div class="ed-shell-main">
        <div class="ed-main">
          <Show when={previewOpen()} fallback={editorColumn}>
            <SplitPane
              first={editorColumn}
              second={<PreviewPane />}
              initial={Math.round(window.innerWidth * 0.42)}
              min={320}
            />
          </Show>
        </div>
        <StatusBar previewOpen={previewOpen()} onTogglePreview={() => setPreviewOpen(!previewOpen())} />
      </div>

      <Modal
        open={conflict() !== null}
        onClose={dismissConflict}
        title="File changed on disk"
        footer={
          <>
            <Button onClick={() => reloadFromDisk(conflict().path)}>Reload from disk</Button>
            <Button
              variant="danger"
              onClick={async () => {
                const res = await saveBuffer(conflict().path, { overwrite: true });
                if (res.ok) toast('Saved (overwrote disk)', { tone: 'success', duration: 1500 });
                else toast(res.error, { tone: 'danger', duration: 8000 });
              }}
            >
              Overwrite
            </Button>
          </>
        }
      >
        <p>
          <code>{conflict()?.path}</code> changed on disk while you were editing. Reload to take
          the disk version (your buffer changes are lost), or overwrite it with your buffer.
        </p>
      </Modal>

      <Toaster />
    </AppShell>
  );
}
