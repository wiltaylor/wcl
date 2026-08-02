/* Design-mode nav panel: the site's menu structure as an editable tree.
   Entries come from /api/nav with their source bindings; ops go through
   commitNavOp (parse → mutate → commit → rebuild → nav reload). For a
   wskill the tree is `index` blocks + their pinned units; for book /
   website / presentation it's the literal toc / menu / deck entries
   (repeater output shows read-only). Clicking an entry navigates the
   canvas iframe to its page. */

import { For, Show, createSignal } from 'solid-js';
import { ArrowDown, ArrowUp, FilePlus, FileText, FolderPlus, Pencil, Pin, PinOff, Trash2 } from 'lucide-solid';
import { Badge, Button, IconButton, Input, Modal, Select, Checkbox, toast } from '@forge/ui';
import { CodeEditor } from '@forge/code';

import { api } from '../../api';
import { wclLanguage } from '../../lang/wcl';
import {
  busy,
  commitNavOp,
  commitUnitCreate,
  loadNav,
  navModel,
  setGotoPage,
} from '../../state/design';
import AddUnitDialog from './AddUnitDialog';

export default function NavPanel() {
  const model = () => navModel();
  const wskill = () => model()?.wskill && model()?.site_type === 'book';
  /** { type: 'rename'|'body'|'add_section'|'add_page'|'add_unit', ... } | null */
  const [dialog, setDialog] = createSignal(null);

  // ------------------------------------------------------------------
  // Static-site ops (source-binding addressed)
  // ------------------------------------------------------------------

  const bindingOp = (entry, op, extra = {}) =>
    commitNavOp({
      op,
      file: entry.source.file,
      span: entry.source.span,
      kind: entry.kind,
      ...extra,
    });

  // ------------------------------------------------------------------
  // Wskill related-list ops
  // ------------------------------------------------------------------

  const reorderChild = (indexEntry, childId, dir) => {
    const ids = indexEntry.children.filter((c) => c.kind === 'unit').map((c) => c.unit.id);
    const i = ids.indexOf(childId);
    const j = dir === 'up' ? i - 1 : i + 1;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    commitNavOp({ op: 'reorder_children', index_id: indexEntry.id, order: ids });
  };

  const unpinnableUnits = (indexEntry) => {
    const pinned = new Set(
      indexEntry.children.filter((c) => c.kind === 'unit').map((c) => c.unit.id),
    );
    return (model()?.units ?? []).filter((u) => !pinned.has(u.id));
  };

  const openIndexBody = async (entry) => {
    setDialog({ type: 'body', entry, source: null });
    const res = await api.blockSource({ file: entry.source.file, span: entry.source.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      setDialog(null);
      return;
    }
    setDialog((current) =>
      current?.type === 'body' && current.entry.id === entry.id
        ? {
            ...current,
            source: res.body?.source ?? 'body {\n  p "Describe this section."\n}',
          }
        : current,
    );
  };

  // ------------------------------------------------------------------
  // Dialogs
  // ------------------------------------------------------------------

  const submitDialog = async () => {
    const d = dialog();
    if (!d) return;
    let res = { ok: true };
    if (d.type === 'rename') {
      res = await bindingOp(d.entry, 'rename', { title: d.title });
    } else if (d.type === 'body') {
      if (!d.source) return;
      res = await commitNavOp({
        op: 'set_index_body',
        index_id: d.entry.id,
        source: d.source,
      });
    } else if (d.type === 'add_section') {
      if (wskill()) {
        if (!d.id) return toast('The section needs an id', { tone: 'danger', duration: 4000 });
        res = await commitNavOp({
          op: 'add_section',
          id: d.id,
          title: d.title || d.id,
          file: d.file,
        });
      } else {
        const kind =
          model()?.site_type === 'website' ? 'item' : model()?.site_type === 'presentation' ? 'section' : 'chapter';
        res = await commitNavOp({
          op: 'add_section',
          kind,
          title: d.title,
          file: model().container.file,
          span: model().container.span,
        });
      }
    } else if (d.type === 'add_page') {
      res = await commitNavOp({
        op: 'add_page',
        name: d.name,
        title: d.title || d.name,
        nav: d.link
          ? {
              container_span: model().container.span,
              kind: model()?.site_type === 'website' ? 'item' : 'chapter',
            }
          : null,
      });
    }
    if (res.ok) setDialog(null);
  };

  // ------------------------------------------------------------------
  // Tree rendering
  // ------------------------------------------------------------------

  const Entry = (props) => {
    const e = () => props.entry;
    const parent = () => props.parent;
    return (
      <li>
        <div class="ed-nav-row" classList={{ 'is-synthetic': e().synthetic }}>
          <button
            type="button"
            class="ed-nav-title"
            disabled={!e().page}
            onClick={() => e().page && setGotoPage(e().page)}
            title={e().page ? `Open ${e().page}` : undefined}
          >
            {e().title}
          </button>
          <Show when={e().synthetic}>
            <Badge>generated</Badge>
          </Show>
          <Show when={e().missing}>
            <Badge tone="danger">missing</Badge>
          </Show>
          <span class="ed-nav-actions">
            <Show when={!e().synthetic && !busy()}>
              {/* Wskill pinned unit: reorder within / unpin from its index. */}
              <Show
                when={e().kind === 'unit' && parent()}
                fallback={
                  <>
                    <Show when={e().source}>
                      <Show when={e().kind === 'index'}>
                        <IconButton
                          icon={FileText}
                          label={e().page ? 'Edit body' : 'Add body'}
                          onClick={() => openIndexBody(e())}
                        />
                      </Show>
                      <IconButton
                        icon={Pencil}
                        label="Rename"
                        onClick={() => setDialog({ type: 'rename', entry: e(), title: e().title })}
                      />
                      <IconButton icon={ArrowUp} label="Move up" onClick={() => bindingOp(e(), 'move', { dir: 'up' })} />
                      <IconButton icon={ArrowDown} label="Move down" onClick={() => bindingOp(e(), 'move', { dir: 'down' })} />
                      <IconButton icon={Trash2} label="Remove" onClick={() => bindingOp(e(), 'remove')} />
                    </Show>
                  </>
                }
              >
                <IconButton icon={ArrowUp} label="Move up" onClick={() => reorderChild(parent(), e().unit.id, 'up')} />
                <IconButton icon={ArrowDown} label="Move down" onClick={() => reorderChild(parent(), e().unit.id, 'down')} />
                <IconButton
                  icon={PinOff}
                  label="Unpin from section"
                  onClick={() => commitNavOp({ op: 'unpin_unit', index_id: parent().id, unit_id: e().unit.id })}
                />
              </Show>
            </Show>
          </span>
        </div>
        <Show when={e().children?.length || (e().kind === 'index' && !e().synthetic)}>
          <ul class="ed-nav-children">
            <For each={e().children ?? []}>
              {(child) => <Entry entry={child} parent={e().kind === 'index' ? e() : undefined} />}
            </For>
            {/* Pin an existing unit into a wskill section. */}
            <Show when={e().kind === 'index' && unpinnableUnits(e()).length > 0 && !busy()}>
              <li class="ed-nav-pinrow">
                <Pin size={12} />
                <Select
                  options={unpinnableUnits(e()).map((u) => ({
                    value: u.id,
                    label: `${u.title} (${u.kind})`,
                  }))}
                  placeholder="Pin a unit…"
                  value={undefined}
                  onChange={(id) =>
                    id && commitNavOp({ op: 'pin_unit', index_id: e().id, unit_id: id })
                  }
                />
              </li>
            </Show>
          </ul>
        </Show>
      </li>
    );
  };

  const d = dialog;
  const setD = (patch) => setDialog({ ...d(), ...patch });

  return (
    <div class="ed-nav-panel">
      <div class="ed-nav-head">
        <strong>{wskill() ? 'Sections' : 'Navigation'}</strong>
        <span class="spacer" />
        <IconButton
          icon={FolderPlus}
          label={wskill() ? 'Add section (index)' : 'Add section'}
          disabled={busy() || !model()}
          onClick={() =>
            setDialog(
              wskill()
                ? {
                    type: 'add_section',
                    id: '',
                    title: '',
                    file: model()?.nav?.[0]?.source?.file ?? '',
                  }
                : { type: 'add_section', title: '' },
            )
          }
        />
        <IconButton
          icon={FilePlus}
          label={wskill() ? 'Add unit' : 'Add page'}
          disabled={busy() || !model()}
          onClick={() =>
            setDialog(
              wskill() ? { type: 'add_unit' } : { type: 'add_page', name: '', title: '', link: true },
            )
          }
        />
      </div>

      <Show when={model()} fallback={<div class="ed-empty">Loading navigation…</div>}>
        <ul class="ed-nav-tree">
          <For each={model().nav}>{(entry) => <Entry entry={entry} />}</For>
        </ul>
      </Show>

      {/* ---- dialogs ---- */}
      <AddUnitDialog
        open={d()?.type === 'add_unit'}
        onClose={() => setDialog(null)}
        indexes={(model()?.nav ?? []).filter((n) => n.kind === 'index')}
        onSubmit={commitUnitCreate}
      />
      <Modal
        open={d() !== null && d()?.type !== 'add_unit'}
        onClose={() => setDialog(null)}
        title={
          {
            rename: 'Rename entry',
            body: d()?.entry?.page ? 'Edit section body' : 'Add section body',
            add_section: 'Add section',
            add_page: 'Add page',
          }[d()?.type] ?? ''
        }
        footer={
          <>
            <Button onClick={() => setDialog(null)}>Cancel</Button>
            <Button variant="primary" onClick={submitDialog} disabled={busy()}>
              {d()?.type === 'rename' ? 'Rename' : d()?.type === 'body' ? 'Save body' : 'Add'}
            </Button>
          </>
        }
      >
        <Show when={d()?.type === 'rename'}>
          <Input value={d().title} onInput={(e) => setD({ title: e.currentTarget.value })} placeholder="Title" />
        </Show>
        <Show
          when={d()?.type === 'body' && d()?.source !== null}
          fallback={d()?.type === 'body' ? <div class="ed-empty">Reading the body…</div> : null}
        >
          <CodeEditor
            value={d().source}
            onChange={(source) => setD({ source })}
            language={wclLanguage}
            height="320px"
          />
        </Show>
        <Show when={d()?.type === 'add_section'}>
          <div class="ed-form">
            <Show when={wskill()}>
              <Input value={d().id} onInput={(e) => setD({ id: e.currentTarget.value })} placeholder="id (identifier)" />
              <Input value={d().file} onInput={(e) => setD({ file: e.currentTarget.value })} placeholder="target .wcl file" />
            </Show>
            <Input value={d().title} onInput={(e) => setD({ title: e.currentTarget.value })} placeholder="Title" />
          </div>
        </Show>
        <Show when={d()?.type === 'add_page'}>
          <div class="ed-form">
            <Input value={d().name} onInput={(e) => setD({ name: e.currentTarget.value })} placeholder="page name (identifier)" />
            <Input value={d().title} onInput={(e) => setD({ title: e.currentTarget.value })} placeholder="Title" />
            <Show when={model()?.container}>
              <Checkbox checked={d().link} onChange={(v) => setD({ link: v })}>
                Link it in the navigation
              </Checkbox>
            </Show>
          </div>
        </Show>
      </Modal>
    </div>
  );
}
