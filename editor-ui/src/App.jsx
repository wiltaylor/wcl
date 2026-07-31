/* Layout composition: a Forge AppShell — "WCL" brand + site picker +
   Rebuild in the topbar, the file tree in the sidebar (mobile drawer for
   free), and the main area holding SplitPane(editor | preview) over the
   status bar — plus global Ctrl+S and the save conflict modal. */

import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { Code2, Database, Moon, PenTool } from 'lucide-solid';
import {
  AppShell,
  Button,
  IconButton,
  Modal,
  SplitPane,
  Spinner,
  Toaster,
  ToggleGroup,
  toast,
} from '@forge/ui';

import FileTree from './components/FileTree';
import TabStrip from './components/TabStrip';
import EditorPane from './components/EditorPane';
import PreviewPane from './components/PreviewPane';
import SitePicker from './components/SitePicker';
import StatusBar from './components/StatusBar';
import DesignView from './components/design/DesignView';
import DataView from './components/data/DataView';
import {
  active,
  conflict,
  dismissConflict,
  reloadFromDisk,
  saveBuffer,
} from './state/buffers';
import { treeData } from './state/tree';
import { loadSites, selected } from './state/sites';
import { mainPreview } from './state/preview';
import { enterData, enterDesign, exitDesign, mode } from './state/design';

export default function App() {
  const [previewOpen, setPreviewOpen] = createSignal(true);

  // Dark is the default (prefers-color-scheme via the tokens); the toggle
  // pins an explicit data-theme from wherever the system landed.
  const toggleTheme = () => {
    const el = document.documentElement;
    const current =
      el.getAttribute('data-theme') ??
      (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark');
    el.setAttribute('data-theme', current === 'dark' ? 'light' : 'dark');
  };

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

  // Design mode collapses the AppShell's sidebar grid column (the shell
  // always reserves --sidebar-w, even with an empty nav) so the design
  // view sticks to the left edge like the code editor does.
  createEffect(() => {
    document.body.classList.toggle('ed-design-mode', mode() !== 'code');
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
    const res = await mainPreview.build();
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
          <ToggleGroup
            options={[
              { value: 'code', label: 'Code', icon: Code2 },
              { value: 'design', label: 'Design', icon: PenTool, disabled: !selected() },
              { value: 'data', label: 'Data', icon: Database, disabled: !selected() },
            ]}
            value={mode()}
            onChange={async (m) => {
              if (m === mode()) return;
              if (m === 'design') {
                // enterDesign invalidates the main preview (its save pass
                // changed disk); the canvas rebuilds as it mounts.
                await enterDesign();
              } else if (m === 'data') {
                await enterData();
              } else {
                exitDesign();
              }
            }}
          />
          <Show when={mode() === 'code'}>
            <Button size="sm" onClick={runRebuild} disabled={!selected() || mainPreview.building()}>
              Rebuild
            </Button>
          </Show>
          <Show when={mainPreview.building()}>
            <Spinner size={16} label="Building preview" />
          </Show>
          <div style={{ flex: 1 }} />
          <IconButton icon={Moon} label="Toggle dark/light" onClick={toggleTheme} />
        </>
      }
      sidebar={mode() === 'code' ? <FileTree /> : undefined}
    >
      <div class="ed-shell-main">
        <div class="ed-main">
          <Show
            when={mode() !== 'code'}
            fallback={
              <Show when={previewOpen()} fallback={editorColumn}>
                <SplitPane
                  first={editorColumn}
                  second={<PreviewPane />}
                  initial={Math.round(window.innerWidth * 0.42)}
                  min={320}
                />
              </Show>
            }
          >
            <Show when={mode() === 'design'} fallback={<DataView />}>
              <DesignView />
            </Show>
          </Show>
        </div>
        <Show when={mode() === 'code'}>
          <StatusBar previewOpen={previewOpen()} onTogglePreview={() => setPreviewOpen(!previewOpen())} />
        </Show>
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
