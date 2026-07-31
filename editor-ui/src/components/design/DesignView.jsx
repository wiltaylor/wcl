/* The Design-mode full-page view: NavPanel | DesignCanvas in a split, plus
   the structured block editors (all Forge modals in the parent):

   - fragment  — the universal fallback: the block's WCL source in a
                 CodeMirror, committed via replace_source
   - code      — language + source heredoc for `code` blocks
   - table     — cell grid over the pipe-table rows (computed `rows` fall
                 back to the fragment editor)
   - image     — property form (source / alt / width / height)
   - component — slot property form for wdoc_component instances
   - insert    — the add-block palette (body blocks + components) */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Button, Checkbox, Input, Modal, SplitPane, Tabs, toast } from '@forge/ui';
import { CodeEditor } from '@forge/code';

import { api } from '../../api';
import { wclLanguage } from '../../lang/wcl';
import { loadSites, selected } from '../../state/sites';
import {
  busy,
  commitOps,
  commitOpsLocal,
  loadNav,
  loadPalette,
  palette,
  popover,
  selection,
  setPopover,
  setSelection,
} from '../../state/design';
import { reloadGraph } from '../../state/graph';
import { anchorEls, restampExcept, shapeEls } from '../../preview/anchors';
import { patchAnchors } from '../../preview/localops';
import { freshShapeId, shapeSnippet } from '../../preview/schemaform';
import { wclString } from '../../preview/wysiwyg';
import {
  delColAt,
  insertColAt,
  insertRowAt,
  moveCol,
  moveRow,
  parseTable,
  tableCommit,
} from '../../preview/table';
import { designTab } from '../../state/design';
import DesignCanvas, { viewLabel } from './DesignCanvas';
import GraphView from './GraphView';
import IndexPanel from './IndexPanel';
import NavPanel from './NavPanel';
import SystemsPanel from './SystemsPanel';
import SystemsView from './SystemsView';

/** The panel + surface pair for the active tab. */
const SURFACES = {
  graph: { first: IndexPanel, second: GraphView },
  systems: { first: SystemsPanel, second: SystemsView },
  canvas: { first: NavPanel, second: DesignCanvas },
};

export default function DesignView() {
  const surface = () => SURFACES[designTab()] ?? SURFACES.canvas;
  return (
    <div class="ed-design-view">
      <SplitPane
        first={<Dynamic component={surface().first} />}
        second={<Dynamic component={surface().second} />}
        initial={280}
        min={200}
      />
      <BlockEditorModals />
    </div>
  );
}

/** Escape hatch shared by the modals: replace the whole block. */
const replaceSource = (anchor, source, etag) =>
  commitOps(anchor.file, [{ op: 'replace_source', span: anchor.span, source }], {
    etag,
    reveal: 'edited',
  });

function BlockEditorModals() {
  const p = popover;
  const anchor = () => p()?.anchor;
  /** The opened block's /api/block/source payload (null while loading). */
  const [src, setSrc] = createSignal(null);

  createEffect(() => {
    const a = anchor();
    setSrc(null);
    if (!a || p()?.type === 'insert' || p()?.type === 'profiles') return;
    api.blockSource({ file: a.file, span: a.span }).then((res) => {
      if (!res.ok) {
        toast(res.error, { tone: 'danger', duration: 6000 });
        setPopover(null);
        return;
      }
      setSrc(res);
    });
  });

  const close = () => setPopover(null);
  const commitAnd = async (promise) => {
    const res = await promise;
    if (res.ok) close();
  };

  return (
    <>
      <Show when={p()?.type === 'fragment' && src()}>
        <FragmentEditor anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'code' && src()}>
        <CodeBlockEditor anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'table' && src()}>
        <TableEditor anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'image' && src()}>
        <ImageEditor anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'component' && src()}>
        <ComponentEditor anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'visibility' && src()}>
        <VisibilityEditor
          anchor={anchor()}
          src={src()}
          surface={p()?.surface}
          onClose={close}
          onCommit={commitAnd}
        />
      </Show>
      <Show when={p()?.type === 'insert'}>
        <InsertPalette anchor={anchor()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'add-shape' && src()}>
        <AddShapeModal anchor={anchor()} src={src()} onClose={close} onCommit={commitAnd} />
      </Show>
      <Show when={p()?.type === 'profiles'}>
        <ProfilesModal onClose={close} />
      </Show>
    </>
  );
}

// ---------------------------------------------------------------------------
// Wskill profiles (whole views on/off)
// ---------------------------------------------------------------------------

const ALL_PROFILES = ['book', 'ai_skill', 'presentation', 'training'];

function ProfilesModal(props) {
  const node = () => selected();
  const enabled = (kind) => (node()?.views ?? []).some((v) => v.kind === kind);
  const [confirm, setConfirm] = createSignal(null); // kind pending disable
  const [working, setWorking] = createSignal(false);

  const apply = async (kind, enable) => {
    setWorking(true);
    const res = await api.wskillProfile(node().registry, kind, enable);
    setWorking(false);
    setConfirm(null);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 8000 });
      return;
    }
    toast(enable ? `Enabled ${viewLabel(kind)}` : `Removed ${viewLabel(kind)}`, {
      tone: 'success',
      duration: 3000,
    });
    // The view set changed: refresh discovery, the design models, and the
    // graph (its per-view chips, filters and placement lists are keyed by
    // the site set) — the button lives on the graph toolbar too.
    await loadSites();
    loadNav();
    loadPalette();
    reloadGraph({ keepPositions: true });
  };

  return (
    <Modal
      open
      onClose={props.onClose}
      title="Wskill profiles"
      footer={<Button onClick={props.onClose}>Close</Button>}
    >
      <div class="ed-form">
        <For each={ALL_PROFILES}>
          {(kind) => (
            <div class="ed-profile-row">
              <Checkbox
                checked={enabled(kind)}
                disabled={working() || (kind === 'book' && enabled('book'))}
                onChange={(on) => (on ? apply(kind, true) : setConfirm(kind))}
              >
                {viewLabel(kind)}
              </Checkbox>
              <Show when={confirm() === kind}>
                <span class="ed-profile-confirm">
                  Delete its <code>wdoc/</code> folder? (recoverable via git)
                  <Button size="sm" variant="danger" disabled={working()} onClick={() => apply(kind, false)}>
                    Remove
                  </Button>
                  <Button size="sm" onClick={() => setConfirm(null)}>
                    Keep
                  </Button>
                </span>
              </Show>
            </div>
          )}
        </For>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Visibility (per-view toggles → @except(sites = [...]))
// ---------------------------------------------------------------------------

function VisibilityEditor(props) {
  const views = () => (selected()?.wskill ? (selected().views ?? []) : []);
  const custom = props.src.visibility?.custom === true;
  const initialExcept = new Set(props.src.visibility?.except_sites ?? []);
  // site name → visible?
  const [visible, setVisible] = createSignal(
    Object.fromEntries(views().map((v) => [v.site, !initialExcept.has(v.site)])),
  );
  // @except names sites this wskill's views don't cover — preserve them.
  const foreign = [...initialExcept].filter((s) => !views().some((v) => v.site === s));

  const save = () => {
    const except = [
      ...views()
        .filter((v) => v.site && visible()[v.site] === false)
        .map((v) => v.site),
      ...foreign,
    ];
    // Apply IN PLACE on the owning surface — restamp (merged) or remove
    // (hidden in the shown view); no rebuild, no iframe reload. Un-hiding
    // a block absent from a non-merged DOM can't be shown without a
    // render, so that one case falls back to the full loop (`false`).
    // Without a surface (the content modal's block-list fallback) the
    // graph reload inside commitOpsLocal refreshes the rows.
    const onApplied = (res) => {
      const s = props.surface;
      const d = s?.doc?.();
      if (!d) return true;
      const els = anchorEls(d, props.anchor.file, props.anchor.span);
      const site = s.currentSite();
      const hidesHere = !s.merged() && site && except.includes(site);
      const unhidesHere =
        !s.merged() && site && initialExcept.has(site) && !except.includes(site);
      if (unhidesHere && els.length === 0) return false;
      patchAnchors(d, props.anchor.file, res.span_map ?? []);
      for (const el of els) {
        if (hidesHere) el.remove();
        else restampExcept(el, except);
      }
      if (hidesHere) {
        const sel = selection();
        if (sel && sel.file === props.anchor.file && sel.span.start === props.anchor.span.start) {
          setSelection(null);
        }
      }
      s.redecorate();
      return true;
    };
    props.onCommit(
      commitOpsLocal(
        props.anchor.file,
        [{ op: 'set_visibility', span: props.anchor.span, except_sites: except }],
        { etag: props.src.etag, onApplied },
      ),
    );
  };

  return (
    <Modal
      open
      onClose={props.onClose}
      title={`Views showing this ${props.src.kind}`}
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Show when={!custom}>
            <Button variant="primary" disabled={busy()} onClick={save}>
              Save
            </Button>
          </Show>
        </>
      }
    >
      <Show
        when={!custom}
        fallback={
          <p>
            This block carries custom visibility (<code>@only</code> or non-site axes) — edit it as
            source instead.
          </p>
        }
      >
        <div class="ed-form">
          <For each={views()}>
            {(v) => (
              <Checkbox
                checked={visible()[v.site] !== false}
                onChange={(on) => setVisible({ ...visible(), [v.site]: on })}
              >
                {viewLabel(v.kind)} ({v.site})
              </Checkbox>
            )}
          </For>
          <Show when={foreign.length > 0}>
            <p class="ed-design-kind">Also hidden from: {foreign.join(', ')}</p>
          </Show>
        </div>
      </Show>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Fragment (universal WCL source editor)
// ---------------------------------------------------------------------------

function FragmentEditor(props) {
  const [text, setText] = createSignal(props.src.source);
  return (
    <Modal
      open
      onClose={props.onClose}
      title={`Edit ${props.src.kind} source`}
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Button
            variant="primary"
            disabled={busy()}
            onClick={() => props.onCommit(replaceSource(props.anchor, text(), props.src.etag))}
          >
            Save
          </Button>
        </>
      }
    >
      <Show when={props.notice}>
        <p class="ed-fragment-notice">{props.notice}</p>
      </Show>
      <div class="ed-design-code">
        <CodeEditor value={text()} onChange={setText} language={wclLanguage} height="320px" />
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Code block (language + source)
// ---------------------------------------------------------------------------

function CodeBlockEditor(props) {
  const langSlot = () => props.src.labels?.[0];
  const srcField = () => props.src.fields?.source;
  // Computed language / source → fragment editing is the truth.
  if (langSlot()?.state === 'computed' || srcField()?.state === 'computed') {
    return <FragmentEditor {...props} />;
  }
  const [lang, setLang] = createSignal(langSlot()?.text ?? 'text');
  const [text, setText] = createSignal(srcField()?.text ?? '');
  const save = () =>
    props.onCommit(
      commitOps(
        props.anchor.file,
        [
          { op: 'set_label', span: props.anchor.span, slot: 0, text: lang() },
          { op: 'set_field', span: props.anchor.span, field: 'source', text: text() },
        ],
        { etag: props.src.etag, reveal: 'edited' },
      ),
    );
  return (
    <Modal
      open
      onClose={props.onClose}
      title="Edit code block"
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Button variant="primary" disabled={busy()} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div class="ed-form">
        <Input value={lang()} onInput={(e) => setLang(e.currentTarget.value)} placeholder="language" />
        <div class="ed-design-code">
          <CodeEditor value={text()} onChange={setText} height="320px" />
        </div>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Table (cell grid over pipe rows OR all-literal `rows = [[…]]` lists)
// ---------------------------------------------------------------------------

function TableEditor(props) {
  // Shared source model: all-literal `rows = [[…]]` lists, literal pipe
  // rows, or null for computed tables (fragment editor + explanation).
  const model = parseTable(props.src);
  if (!model) {
    return (
      <FragmentEditor
        {...props}
        notice={
          props.src.fields?.rows
            ? 'This table is generated: its rows come from an expression over data, so there is no cell grid — edit the expression here, or edit the underlying data objects it draws from.'
            : 'No literal pipe rows found in this table — edit its source directly.'
        }
      />
    );
  }
  const [grid, setGrid] = createSignal(model.grid.map((r) => [...r]));
  const cols = () => Math.max(...grid().map((r) => r.length), 1);

  const setCell = (r, c, v) =>
    setGrid(grid().map((row, ri) => (ri === r ? row.map((x, ci) => (ci === c ? v : x)) : row)));
  const addRow = () => setGrid(insertRowAt(grid(), grid().length - 1, cols()));
  const delRow = (r) => grid().length > 1 && setGrid(grid().filter((_, ri) => ri !== r));
  const addCol = () => setGrid(insertColAt(grid(), cols() - 1));

  const save = () => {
    const w = tableCommit(model, grid(), props.anchor.span);
    props.onCommit(
      w.ops
        ? commitOps(props.anchor.file, w.ops, { etag: props.src.etag, reveal: 'edited' })
        : replaceSource(props.anchor, w.source, props.src.etag),
    );
  };

  return (
    <Modal
      open
      onClose={props.onClose}
      title={model.headerRows ? 'Edit table (first row is the header)' : 'Edit table'}
      footer={
        <>
          <Button onClick={addRow}>+ Row</Button>
          <Button onClick={addCol}>+ Column</Button>
          <span style={{ flex: 1 }} />
          <Button onClick={props.onClose}>Cancel</Button>
          <Button variant="primary" disabled={busy()} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div class="ed-table-grid">
        <table>
          <tbody>
            <tr>
              <For each={Array.from({ length: cols() })}>
                {(_, c) => (
                  <td class="ed-table-rowctl">
                    <Button size="sm" title="Move column left" onClick={() => setGrid(moveCol(grid(), c(), -1))}>
                      ←
                    </Button>
                    <Button size="sm" title="Move column right" onClick={() => setGrid(moveCol(grid(), c(), 1))}>
                      →
                    </Button>
                    <Button size="sm" title="Insert column right" onClick={() => setGrid(insertColAt(grid(), c()))}>
                      ＋
                    </Button>
                    <Button size="sm" title="Delete column" onClick={() => setGrid(delColAt(grid(), c()))}>
                      ×
                    </Button>
                  </td>
                )}
              </For>
              <td />
            </tr>
            <For each={grid()}>
              {(row, r) => (
                <tr>
                  <For each={Array.from({ length: cols() })}>
                    {(_, c) => (
                      <td>
                        <Input
                          value={row[c()] ?? ''}
                          onInput={(e) => setCell(r(), c(), e.currentTarget.value)}
                        />
                      </td>
                    )}
                  </For>
                  <td class="ed-table-rowctl">
                    <Button size="sm" title="Move row up" onClick={() => setGrid(moveRow(grid(), r(), -1))}>
                      ↑
                    </Button>
                    <Button size="sm" title="Move row down" onClick={() => setGrid(moveRow(grid(), r(), 1))}>
                      ↓
                    </Button>
                    <Button size="sm" title="Insert row below" onClick={() => setGrid(insertRowAt(grid(), r(), cols()))}>
                      ＋
                    </Button>
                    <Button size="sm" title="Delete row" onClick={() => delRow(r())}>
                      ×
                    </Button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Image (property form)
// ---------------------------------------------------------------------------

function ImageEditor(props) {
  const lit = (slot) => (slot?.state === 'literal' ? (slot.text ?? '') : null);
  const source0 = lit(props.src.labels?.[0]);
  if (source0 === null) return <FragmentEditor {...props} />;
  const [form, setForm] = createSignal({
    source: source0,
    alt: lit(props.src.fields?.alt) ?? '',
    width: lit(props.src.fields?.width) ?? '',
    height: lit(props.src.fields?.height) ?? '',
  });
  const save = () => {
    const f = form();
    const ops = [{ op: 'set_label', span: props.anchor.span, slot: 0, text: f.source }];
    if (f.alt !== '') ops.push({ op: 'set_field', span: props.anchor.span, field: 'alt', text: f.alt });
    for (const dim of ['width', 'height']) {
      const v = f[dim].trim();
      if (v !== '' && !Number.isNaN(Number(v))) {
        ops.push({ op: 'set_field', span: props.anchor.span, field: dim, expr: v });
      }
    }
    props.onCommit(
      commitOps(props.anchor.file, ops, { etag: props.src.etag, reveal: 'edited' }),
    );
  };
  const bind = (key) => ({
    value: form()[key],
    onInput: (e) => setForm({ ...form(), [key]: e.currentTarget.value }),
  });
  return (
    <Modal
      open
      onClose={props.onClose}
      title="Edit image"
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Button variant="primary" disabled={busy()} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div class="ed-form">
        <Input {...bind('source')} placeholder="source (path / URL)" />
        <Input {...bind('alt')} placeholder="alt text" />
        <Input {...bind('width')} placeholder="width (number, optional)" />
        <Input {...bind('height')} placeholder="height (number, optional)" />
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Component instance (slot property form)
// ---------------------------------------------------------------------------

function ComponentEditor(props) {
  const def = () => palette()?.components?.find((c) => c.name === props.src.kind);
  if (!def()) return <FragmentEditor {...props} />;
  const initial = {};
  for (const slot of def().slots) {
    const f = props.src.fields?.[slot.name];
    initial[slot.name] = {
      value: f?.state === 'literal' ? (f.text ?? '') : '',
      computed: f?.state === 'computed',
      present: !!f,
    };
  }
  const [form, setForm] = createSignal(initial);
  const save = () => {
    const ops = [];
    for (const slot of def().slots) {
      const f = form()[slot.name];
      if (f.computed) continue;
      const orig = props.src.fields?.[slot.name]?.text ?? '';
      if (f.value === orig && f.present) continue;
      if (f.value === '' && !slot.required) continue;
      ops.push({ op: 'set_field', span: props.anchor.span, field: slot.name, text: f.value });
    }
    if (ops.length === 0) return props.onClose();
    props.onCommit(
      commitOps(props.anchor.file, ops, { etag: props.src.etag, reveal: 'edited' }),
    );
  };
  return (
    <Modal
      open
      onClose={props.onClose}
      title={`${props.src.kind} properties`}
      footer={
        <>
          <Button onClick={props.onClose}>Cancel</Button>
          <Button variant="primary" disabled={busy()} onClick={save}>
            Save
          </Button>
        </>
      }
    >
      <div class="ed-form">
        <For each={def().slots}>
          {(slot) => (
            <Input
              value={form()[slot.name].value}
              disabled={form()[slot.name].computed}
              onInput={(e) =>
                setForm({
                  ...form(),
                  [slot.name]: { ...form()[slot.name], value: e.currentTarget.value },
                })
              }
              placeholder={
                form()[slot.name].computed
                  ? `${slot.name} (computed — edit as source)`
                  : `${slot.name}${slot.required ? ' (required)' : slot.default ? ` (default: ${slot.default})` : ''}`
              }
            />
          )}
        </For>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Insert palette (body blocks + components)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Add-shape palette (diagram children)
// ---------------------------------------------------------------------------

/** Curated first-class shapes, in display order; wf_* get their own tab and
    everything else lands under "All" (user-declared kinds included). */
const CURATED_SHAPES = [
  'process',
  'decision',
  'terminator',
  'node',
  'rect',
  'circle',
  'label',
  'line',
  'polygon',
  'container',
  'card',
  'node_table',
  'tree',
  'state',
  'icon',
  'image',
];

function AddShapeModal(props) {
  const [tab, setTab] = createSignal('shapes');
  const kinds = () => palette()?.diagram_kinds ?? [];
  const curated = () =>
    CURATED_SHAPES.map((k) => kinds().find((e) => e.kind === k)).filter(Boolean);
  const wireframe = () => kinds().filter((e) => e.kind.startsWith('wf_'));
  const rest = () =>
    kinds().filter((e) => !CURATED_SHAPES.includes(e.kind) && !e.kind.startsWith('wf_'));

  const insert = (entry) => {
    const a = props.anchor;
    const source = props.src.source ?? '';
    const manual = ['free', 'none'].includes(a.layout ?? 'free');
    const index = a.el ? shapeEls(a.el).length : (source.match(/\bid\s*=/g) ?? []).length;
    const snippet = shapeSnippet(entry, { uid: freshShapeId(entry.kind, source), manual, index });
    props.onCommit(
      commitOps(
        a.file,
        [{ op: 'insert_child', span: a.span, index: 9999, source: snippet }],
        { etag: props.src.etag, reveal: 'inserted' },
      ),
    );
  };

  const grid = (entries) => (
    <div class="ed-insert-grid">
      <For each={entries}>
        {(entry) => (
          <Button onClick={() => insert(entry)} disabled={busy()} title={entry.doc ?? undefined}>
            {entry.kind}
          </Button>
        )}
      </For>
      <Show when={entries.length === 0}>
        <div class="ed-empty">No shape kinds in this tab</div>
      </Show>
    </div>
  );

  return (
    <Modal open onClose={props.onClose} title="Add a diagram shape">
      <Tabs
        tabs={[
          { id: 'shapes', label: 'Shapes' },
          { id: 'wireframe', label: 'Wireframe' },
          { id: 'all', label: 'All' },
        ]}
        active={tab()}
        onChange={setTab}
      />
      <Show when={tab() === 'shapes'}>{grid(curated())}</Show>
      <Show when={tab() === 'wireframe'}>{grid(wireframe())}</Show>
      <Show when={tab() === 'all'}>{grid(rest())}</Show>
    </Modal>
  );
}

function InsertPalette(props) {
  const [tab, setTab] = createSignal('blocks');
  const [compForm, setCompForm] = createSignal(null); // {def, values}

  const insert = (source) => {
    const a = props.anchor;
    // A container (an empty addressable `body`, or any selected `body`)
    // takes the new block as its child; everything else gets a sibling.
    const op =
      a.kind === 'body'
        ? { op: 'insert_child', span: a.span, index: 9999, source }
        : { op: 'insert_after', span: a.span, source };
    props.onCommit(commitOps(a.file, [op], { reveal: 'inserted' }));
  };

  const insertComponent = () => {
    const { def, values } = compForm();
    const missing = def.slots.filter((s) => s.required && !(values[s.name] ?? '').trim());
    if (missing.length) {
      toast(`Fill: ${missing.map((s) => s.name).join(', ')}`, { tone: 'danger', duration: 4000 });
      return;
    }
    const fields = def.slots
      .filter((s) => (values[s.name] ?? '').trim() !== '')
      .map((s) => `  ${s.name} = ${wclString(values[s.name])}`)
      .join('\n');
    insert(fields ? `${def.name} {\n${fields}\n}` : `${def.name} {}`);
  };

  return (
    <Modal
      open
      onClose={props.onClose}
      title="Insert block"
      footer={
        <Show when={compForm()}>
          <Button onClick={() => setCompForm(null)}>Back</Button>
          <Button variant="primary" disabled={busy()} onClick={insertComponent}>
            Insert
          </Button>
        </Show>
      }
    >
      <Show
        when={!compForm()}
        fallback={
          <div class="ed-form">
            <For each={compForm().def.slots}>
              {(slot) => (
                <Input
                  value={compForm().values[slot.name] ?? slot.default ?? ''}
                  onInput={(e) =>
                    setCompForm({
                      ...compForm(),
                      values: { ...compForm().values, [slot.name]: e.currentTarget.value },
                    })
                  }
                  placeholder={`${slot.name}${slot.required ? ' (required)' : ''}`}
                />
              )}
            </For>
          </div>
        }
      >
        <Tabs
          tabs={[
            { id: 'blocks', label: 'Blocks' },
            { id: 'components', label: 'Components' },
          ]}
          active={tab()}
          onChange={setTab}
        />
        <Show when={tab() === 'blocks'}>
          <div class="ed-insert-grid">
            <For each={palette()?.body_kinds ?? []}>
              {(k) => (
                <Button onClick={() => insert(k.template_source)} disabled={busy()}>
                  {k.label}
                </Button>
              )}
            </For>
          </div>
        </Show>
        <Show when={tab() === 'components'}>
          <div class="ed-insert-grid">
            <For each={palette()?.components ?? []}>
              {(c) => (
                <Button
                  onClick={() => {
                    const values = {};
                    for (const s of c.slots) if (s.default != null) values[s.name] = s.default;
                    setCompForm({ def: c, values });
                  }}
                  disabled={busy()}
                >
                  {c.name}
                </Button>
              )}
            </For>
            <Show when={(palette()?.components ?? []).length === 0}>
              <div class="ed-empty">No components declared in this document</div>
            </Show>
          </div>
        </Show>
      </Show>
    </Modal>
  );
}
