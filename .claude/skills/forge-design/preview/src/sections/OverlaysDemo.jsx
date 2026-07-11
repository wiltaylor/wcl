import { createSignal, createEffect, onCleanup } from 'solid-js';
import { Bell, Copy, Pencil, Rocket, Settings, Trash2, LayoutGrid, History } from 'lucide-solid';
import {
  PageHead, Card, Button, IconButton, Input, Tooltip, Popover, DropdownMenu,
  ContextMenu, Command, Sheet, Modal, Toaster, toast, Kbd, Checkbox,
} from '@forge/ui.jsx';

export default function OverlaysDemo() {
  const [cmdOpen, setCmdOpen] = createSignal(false);
  const [sheetOpen, setSheetOpen] = createSignal(false);
  const [leftSheet, setLeftSheet] = createSignal(false);
  const [modalOpen, setModalOpen] = createSignal(false);
  const [lastAction, setLastAction] = createSignal('—');

  createEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); setCmdOpen(true); }
    };
    document.addEventListener('keydown', onKey);
    onCleanup(() => document.removeEventListener('keydown', onKey));
  });

  const menuItems = [
    { label: 'Rename', icon: Pencil, kbd: 'R', onSelect: () => setLastAction('rename') },
    { label: 'Duplicate', icon: Copy, onSelect: () => setLastAction('duplicate') },
    { label: 'Settings', icon: Settings, disabled: true },
    { separator: true },
    { label: 'Delete', icon: Trash2, danger: true, onSelect: () => setLastAction('delete') },
  ];

  return (
    <>
      <Toaster />
      <PageHead title="Overlays & menus" sub="Tooltip, Popover, DropdownMenu, ContextMenu, Command (try ⌘K / Ctrl-K), Sheet, Toaster" />

      <Card title="Tooltips (hover or keyboard-focus)">
        <div style={{ display: 'flex', gap: '20px', 'align-items': 'center', padding: '12px 0' }}>
          <Tooltip label="Notifications" side="top"><IconButton icon={Bell} label="Notifications" /></Tooltip>
          <Tooltip label="Deploy fleet" side="bottom"><IconButton icon={Rocket} label="Deploy" /></Tooltip>
          <Tooltip label="Settings" side="left"><IconButton icon={Settings} label="Settings" /></Tooltip>
          <Tooltip label="Delete forever" side="right"><IconButton icon={Trash2} label="Delete" /></Tooltip>
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Popover, dropdown & context menus">
        <div style={{ display: 'flex', gap: '12px', 'flex-wrap': 'wrap', 'align-items': 'flex-start' }}>
          <Popover label="Filters" icon={LayoutGrid} width="240px">
            <div style={{ display: 'grid', gap: '10px' }}>
              <Input label="Service" placeholder="vllm-*" />
              <Checkbox checked>Only failures</Checkbox>
              <Button variant="primary" size="sm">Apply</Button>
            </div>
          </Popover>
          <DropdownMenu label="Actions" items={menuItems} />
          <DropdownMenu label="End-aligned" align="end" items={menuItems} />
          <span style={{ 'font-size': '12px', color: 'var(--fg-2)', 'align-self': 'center' }}>
            last action: <span style={{ 'font-family': 'var(--font-mono)' }}>{lastAction()}</span>
          </span>
        </div>
        <div style={{ height: '12px' }} />
        <ContextMenu items={menuItems}>
          <div class="empty" style={{ cursor: 'context-menu' }}>
            <h3>Right-click me</h3>Context menu with the same items.
          </div>
        </ContextMenu>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Command palette, sheet & toaster">
        <div style={{ display: 'flex', gap: '12px', 'flex-wrap': 'wrap' }}>
          <Button onClick={() => setCmdOpen(true)}>Command palette <Kbd>⌘K</Kbd></Button>
          <Button onClick={() => setSheetOpen(true)}>Open sheet (right)</Button>
          <Button onClick={() => setLeftSheet(true)}>Open sheet (left)</Button>
          <Button onClick={() => toast('Deploy started', { tone: 'info', icon: Rocket })}>Toast info</Button>
          <Button onClick={() => toast('Deploy finished in 41 s', { tone: 'success' })}>Toast success</Button>
          <Button onClick={() => toast('Disk almost full on nas', { tone: 'warning', duration: 0 })}>Sticky warning</Button>
          <Button variant="danger" onClick={() => toast('severus unreachable', { tone: 'danger' })}>Toast danger</Button>
        </div>
      </Card>

      <Command open={cmdOpen()} onClose={() => setCmdOpen(false)}
               items={[
                 { group: 'Navigate', label: 'Go to overview', icon: LayoutGrid, kbd: 'G O', onSelect: () => setLastAction('nav:overview') },
                 { group: 'Navigate', label: 'Go to runs', icon: History, kbd: 'G R', onSelect: () => setLastAction('nav:runs') },
                 { group: 'Actions', label: 'Deploy service', icon: Rocket, onSelect: () => toast('Deploying…') },
                 { group: 'Actions', label: 'Rename node', icon: Pencil, onSelect: () => setLastAction('rename') },
                 { group: 'Actions', label: 'Delete run', icon: Trash2, onSelect: () => setLastAction('delete') },
               ]} />

      <Sheet open={sheetOpen()} onClose={() => setSheetOpen(false)} title="Node details"
             footer={<>
               <Button onClick={() => setSheetOpen(false)}>Close</Button>
               <Button variant="primary" onClick={() => setModalOpen(true)}>Open modal above</Button>
             </>}>
        <p>Sheets sit at z 40 — above the nav drawer, below modals, so a confirm dialog can stack on top.</p>
        <div style={{ height: '12px' }} />
        <Input label="Display name" value="DGX Spark" />
      </Sheet>
      <Sheet open={leftSheet()} onClose={() => setLeftSheet(false)} title="Filters" side="left">
        <Checkbox checked>Only failures</Checkbox>
      </Sheet>
      <Modal open={modalOpen()} onClose={() => setModalOpen(false)} title="Confirm change"
             footer={<>
               <Button onClick={() => setModalOpen(false)}>Cancel</Button>
               <Button variant="primary" onClick={() => { setModalOpen(false); toast('Saved', { tone: 'success' }); }}>Save</Button>
             </>}>
        This modal renders above the open sheet (50 &gt; 40); toasts render above both.
      </Modal>
    </>
  );
}
