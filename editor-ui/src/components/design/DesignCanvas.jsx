/* The Design-mode canvas: the edit-mode preview build in an iframe, plus
   the selection/session layer (preview/wysiwyg.js) inside it and the
   floating block toolbar in the parent (Forge chrome can't render inside
   the plain wdoc build). Click selects a block; click again enters a
   source-swap text session for prose kinds (the block shows its raw markup
   while everything around it stays rendered) or opens the right structured
   editor for everything else. Every commit runs the disk → targeted
   rebuild → re-anchor loop in state/design.js. */

import { Show, createEffect, createSignal, onCleanup } from 'solid-js';
import {
  ArrowDown,
  ArrowUp,
  Bold,
  Braces,
  Code,
  IndentDecrease,
  IndentIncrease,
  Italic,
  Link,
  Plus,
  Settings2,
  Trash2,
} from 'lucide-solid';
import { Eye, FileCode2 } from 'lucide-solid';
import { Badge, Button, IconButton, Select, Spinner, Tabs, ToggleGroup, toast } from '@forge/ui';

import { api } from '../../api';
import {
  activeEntry,
  activeView,
  buildSeq,
  previewHref,
  rebuild,
  selectView,
  selected,
} from '../../state/sites';
import {
  busy,
  canvasStale,
  commitOps,
  commitUnitField,
  currentPage,
  designTab,
  editingSession,
  frameReady,
  gotoPage,
  loadNav,
  loadPalette,
  openPageSource,
  palette,
  pendingReveal,
  selection,
  setCanvasStale,
  setCurrentPage,
  setDesignTab,
  setEditingSession,
  setGotoPage,
  setPendingReveal,
  setPopover,
  setSelection,
} from '../../state/design';
import { pageInfo } from '../../preview/frame';
import SkillBrowser from './SkillBrowser';
import {
  anchorOf,
  beginTextSession,
  blockAt,
  elBySpan,
  installDesign,
  markSelected,
  wclString,
  wrapSelection,
} from '../../preview/wysiwyg';

const PROSE = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'li']);
const HEADING_KINDS = ['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'];

/** Display name for an artifact-kind view tab. */
export function viewLabel(kind) {
  switch (kind) {
    case 'book':
      return 'Book';
    case 'presentation':
      return 'Deck';
    case 'training':
      return 'Training';
    case 'ai_skill':
      return 'Skill';
    default:
      return kind.charAt(0).toUpperCase() + kind.slice(1);
  }
}

export default function DesignCanvas() {
  let iframe;
  let wrapper;
  let lastSeq = 0;
  let lastHref = null;
  let teardown = null;
  let session = null; // active beginTextSession handle
  const [toolbarPos, setToolbarPos] = createSignal(null);
  const [sessionRegion, setSessionRegion] = createSignal(null); // {el, plain}

  const doc = () => iframe?.contentDocument;

  // ------------------------------------------------------------------
  // Sessions
  // ------------------------------------------------------------------

  const endSession = () => {
    session = null;
    setSessionRegion(null);
    setEditingSession(null);
  };

  /** Source-swap prose session on one region of the selected block. */
  const proseSession = async (anchor, region, point) => {
    const res = await api.blockSource({ file: anchor.file, span: anchor.span });
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    const slot =
      region.type === 'label' ? res.labels?.[region.slot] : res.fields?.[region.name];
    if (!slot || slot.state !== 'literal') {
      // Interpolated / computed content: the fragment editor is the truth.
      setPopover({ type: 'fragment', anchor });
      return;
    }
    setEditingSession({ anchor, region });
    setSessionRegion({ el: region.regionEl, plain: false });
    const splitKinds = anchor.kind === 'p' || anchor.kind === 'li';
    session = beginTextSession(doc(), region.regionEl, slot.text ?? '', {
      caretAt: point,
      onCancel: endSession,
      onCommit: (text) => {
        endSession();
        const op =
          region.type === 'label'
            ? { op: 'set_label', span: anchor.span, slot: region.slot, text }
            : { op: 'set_field', span: anchor.span, field: region.name, text };
        commitOps(anchor.file, [op], { etag: res.etag, reveal: 'edited' });
      },
      onEnter: splitKinds
        ? (before, after) => {
            endSession();
            commitOps(
              anchor.file,
              [
                { op: 'set_label', span: anchor.span, slot: 0, text: before },
                {
                  op: 'insert_after',
                  span: anchor.span,
                  source: `${anchor.kind} ${wclString(after)}`,
                },
              ],
              { etag: res.etag, reveal: 'inserted', edit: true },
            );
          }
        : undefined,
    });
  };

  /** Inline session on an edit_field binding: read the object's field,
      edit in place, write through /api/unit/field. */
  const fieldSession = async (binding, point) => {
    const loc = await api.locateObject({
      entry: activeEntry(),
      page_file: currentPage()?.file ?? undefined,
      kind: binding.kind,
      target: binding.target ?? undefined,
      files: [],
    });
    if (!loc.ok) {
      toast(loc.error, { tone: 'danger', duration: 6000 });
      return;
    }
    const src = await api.blockSource({ file: loc.file, span: loc.span });
    const slot = src.ok ? src.fields?.[binding.field] : null;
    if (!slot || slot.state !== 'literal') {
      toast(`${binding.field} is computed — edit it in the source`, {
        tone: 'danger',
        duration: 5000,
      });
      return;
    }
    setEditingSession({ binding });
    setSessionRegion({ el: binding.el, plain: binding.plain });
    session = beginTextSession(doc(), binding.el, slot.text ?? '', {
      caretAt: point,
      onCancel: endSession,
      onCommit: (text) => {
        endSession();
        commitUnitField(binding, text);
      },
    });
  };

  /** Route a second click on the selected block to the right editor. */
  const startEditFor = (anchor, targetEl, point) => {
    if (anchor.kind === 'callout') {
      const body = targetEl?.closest?.('.callout-body');
      const title = targetEl?.closest?.('.callout-title, .callout-heading');
      if (body) return proseSession(anchor, { type: 'field', name: 'body', regionEl: body }, point);
      if (title) return proseSession(anchor, { type: 'label', slot: 0, regionEl: title }, point);
      return setPopover({ type: 'fragment', anchor });
    }
    if (PROSE.has(anchor.kind)) {
      return proseSession(anchor, { type: 'label', slot: 0, regionEl: anchor.el }, point);
    }
    if (anchor.kind === 'code') return setPopover({ type: 'code', anchor });
    if (anchor.kind === 'table') return setPopover({ type: 'table', anchor });
    if (anchor.kind === 'image') return setPopover({ type: 'image', anchor });
    if (palette()?.components?.some((c) => c.name === anchor.kind)) {
      return setPopover({ type: 'component', anchor });
    }
    return setPopover({ type: 'fragment', anchor });
  };

  // ------------------------------------------------------------------
  // Frame lifecycle
  // ------------------------------------------------------------------

  const onFrameLoad = () => {
    const d = doc();
    if (!d) return;
    teardown?.();
    session = null;
    setSessionRegion(null);
    setEditingSession(null);
    setSelection(null);
    setCurrentPage(pageInfo(d));
    loadPalette();
    teardown = installDesign(d, {
      enabled: () => !busy(),
      onSelect: (anchor) => {
        setSelection(anchor);
        markSelected(d, anchor?.el ?? null, anchor?.shared);
      },
      onEditIntent: (anchor, targetEl, ev) =>
        startEditFor(anchor, targetEl, ev ? { x: ev.clientX, y: ev.clientY } : null),
      onFieldIntent: (binding, ev) =>
        fieldSession(binding, ev ? { x: ev.clientX, y: ev.clientY } : null),
    });
    // Re-anchor after a commit rebuild.
    const reveal = pendingReveal();
    if (reveal) {
      setPendingReveal(null);
      const el = elBySpan(d, reveal.file, reveal.span);
      if (el) {
        const anchor = anchorOf(d, el);
        setSelection(anchor);
        markSelected(d, el, anchor?.shared);
        el.scrollIntoView({ block: 'nearest' });
        if (reveal.edit && anchor) startEditFor(anchor, el, null);
      }
    }
    frameReady();
  };

  // Rebuild → reload in place (scroll survives); new href → src swap.
  createEffect(() => {
    const seq = buildSeq();
    const href = previewHref();
    if (seq === lastSeq) return;
    lastSeq = seq;
    if (href === lastHref) iframe?.contentWindow?.location.reload();
    lastHref = href;
  });

  // Graph-mode commits leave the canvas anchors stale — rebuild when the
  // canvas becomes the active surface again.
  createEffect(() => {
    if (designTab() === 'canvas' && canvasStale()) {
      setCanvasStale(false);
      rebuild();
    }
  });

  // NavPanel navigation: swap the iframe to the requested page.
  createEffect(() => {
    const page = gotoPage();
    if (!page) return;
    setGotoPage(null);
    const base = previewHref();
    if (!base || !iframe) return;
    iframe.src = `${base.slice(0, base.lastIndexOf('/') + 1)}${page}.html`;
  });

  // ------------------------------------------------------------------
  // Toolbar positioning (parent overlay tracking an iframe-content rect)
  // ------------------------------------------------------------------

  let raf = 0;
  const trackToolbar = () => {
    const target = sessionRegion()?.el ?? selection()?.el;
    if (!target || !wrapper || !iframe) {
      setToolbarPos(null);
      return;
    }
    const r = target.getBoundingClientRect();
    const ir = iframe.getBoundingClientRect();
    const wr = wrapper.getBoundingClientRect();
    // The iframe viewport maps 1:1 onto the iframe element's box.
    const top = ir.top - wr.top + r.top;
    setToolbarPos({
      left: Math.max(4, ir.left - wr.left + r.left),
      top: Math.max(4, top - 36),
      below: top - 36 < 4,
      bottom: ir.top - wr.top + r.bottom,
    });
  };
  createEffect(() => {
    void selection();
    void sessionRegion();
    void busy();
    cancelAnimationFrame(raf);
    const loop = () => {
      trackToolbar();
      raf = requestAnimationFrame(loop);
    };
    if (selection() || sessionRegion()) loop();
    else setToolbarPos(null);
    onCleanup(() => cancelAnimationFrame(raf));
  });
  onCleanup(() => {
    cancelAnimationFrame(raf);
    teardown?.();
  });

  // ------------------------------------------------------------------
  // Toolbar actions
  // ------------------------------------------------------------------

  const structural = (ops, opts = {}) => {
    const a = selection();
    if (!a) return;
    commitOps(a.file, ops, opts);
  };
  const moveSel = (dir) =>
    structural([{ op: 'move', span: selection().span, dir }], { reveal: 'edited' });
  const deleteSel = () => structural([{ op: 'delete', span: selection().span }], { reveal: null });
  const changeKind = (kind) =>
    structural([{ op: 'set_kind', span: selection().span, kind }], { reveal: 'edited' });

  /** li indent/outdent: re-home the item's source under the previous
      sibling / after the parent item, in one atomic batch. */
  const reindent = async (indent) => {
    const a = selection();
    const d = doc();
    if (!a || a.kind !== 'li' || !d) return;
    const src = await api.blockSource({ file: a.file, span: a.span });
    if (!src.ok) return toast(src.error, { tone: 'danger', duration: 5000 });
    let ops = null;
    if (indent) {
      let prev = a.el.previousElementSibling;
      while (prev && prev.getAttribute('data-wcl-kind') !== 'li') prev = prev.previousElementSibling;
      const prevAnchor = prev && anchorOf(d, prev);
      if (!prevAnchor) return toast('No previous item to nest under', { duration: 3000 });
      ops = [
        { op: 'insert_child', span: prevAnchor.span, index: 9999, source: src.source },
        { op: 'delete', span: a.span },
      ];
    } else {
      const parentLi = a.el.parentElement?.closest?.('[data-wcl-kind="li"]');
      const parentAnchor = parentLi && anchorOf(d, parentLi);
      if (!parentAnchor) return toast('Already at the top level', { duration: 3000 });
      ops = [
        { op: 'insert_after', span: parentAnchor.span, source: src.source },
        { op: 'delete', span: a.span },
      ];
    }
    commitOps(a.file, ops, { etag: src.etag, reveal: 'inserted' });
  };

  const marker = (open, close) => {
    const region = sessionRegion();
    const d = doc();
    if (!region || !d) return;
    wrapSelection(d, region.el, open, close ?? open);
    region.el.focus();
  };

  const isComponent = () => palette()?.components?.some((c) => c.name === selection()?.kind);

  return (
    <div class="ed-design-col">
      <div class="ed-design-note">
        <ToggleGroup
          options={[
            { value: 'canvas', label: 'Canvas' },
            { value: 'graph', label: 'Graph', disabled: !selected()?.wskill },
          ]}
          value={designTab()}
          onChange={(t) => setDesignTab(t)}
        />
        <Show when={selected()?.wskill}>
          <Tabs
            tabs={(selected().views ?? []).map((v) => ({
              id: v.id,
              label: viewLabel(v.kind),
            }))}
            active={activeView()?.id}
            onChange={async (id) => {
              selectView(id);
              await rebuild();
              loadNav();
              loadPalette();
            }}
          />
        </Show>
        <span class="ed-design-page">{currentPage()?.name ?? 'no page'}</span>
        <Show when={busy()}>
          <Spinner size={12} label="Applying edit" />
          <span>saving…</span>
        </Show>
        <span class="spacer" />
        <Show when={selected()?.wskill}>
          <Button size="sm" onClick={() => setPopover({ type: 'profiles' })}>
            Profiles
          </Button>
        </Show>
        <Button size="sm" onClick={openPageSource} disabled={!currentPage()}>
          <FileCode2 size={13} /> Open code
        </Button>
      </div>
      <div class="ed-design-canvas" ref={wrapper}>
        <Show when={!activeView()?.skill} fallback={<SkillBrowser />}>
          <Show
            when={previewHref()}
            fallback={
              <div class="ed-empty">
                {selected() ? 'Building the design canvas…' : 'No wdoc sites found in this directory'}
              </div>
            }
          >
            <iframe ref={iframe} src={previewHref()} title="design canvas" onLoad={onFrameLoad} />
          </Show>
        </Show>

      <Show when={toolbarPos() && !busy()}>
        <div
          class="ed-design-toolbar"
          style={{
            left: `${toolbarPos().left}px`,
            top: `${toolbarPos().below ? toolbarPos().bottom + 6 : toolbarPos().top}px`,
          }}
        >
          <Show
            when={editingSession()}
            fallback={
              <>
                <span class="ed-design-kind">{selection()?.kind}</span>
                <Show when={selection()?.shared}>
                  <Badge tone="warning">edits affect all instances</Badge>
                </Show>
                <Show when={HEADING_KINDS.includes(selection()?.kind)}>
                  <Select
                    options={HEADING_KINDS.map((k) => ({ value: k, label: k }))}
                    value={selection()?.kind}
                    onChange={(k) => k !== selection()?.kind && changeKind(k)}
                  />
                </Show>
                <IconButton icon={ArrowUp} label="Move up" onClick={() => moveSel('up')} />
                <IconButton icon={ArrowDown} label="Move down" onClick={() => moveSel('down')} />
                <Show when={selection()?.kind === 'li'}>
                  <IconButton icon={IndentIncrease} label="Nest item" onClick={() => reindent(true)} />
                  <IconButton icon={IndentDecrease} label="Unnest item" onClick={() => reindent(false)} />
                </Show>
                <IconButton
                  icon={Plus}
                  label="Insert below"
                  onClick={() => setPopover({ type: 'insert', anchor: selection() })}
                />
                <Show when={isComponent()}>
                  <IconButton
                    icon={Settings2}
                    label="Properties"
                    onClick={() => setPopover({ type: 'component', anchor: selection() })}
                  />
                </Show>
                <Show when={selected()?.wskill}>
                  <IconButton
                    icon={Eye}
                    label="Views (visibility)"
                    onClick={() => setPopover({ type: 'visibility', anchor: selection() })}
                  />
                </Show>
                <IconButton
                  icon={Braces}
                  label="Edit as source"
                  onClick={() => setPopover({ type: 'fragment', anchor: selection() })}
                />
                <IconButton icon={Trash2} label="Delete block" onClick={deleteSel} />
              </>
            }
          >
            <span class="ed-design-kind">editing — Ctrl+Enter saves, Esc cancels</span>
            <Show when={!sessionRegion()?.plain}>
              <IconButton icon={Bold} label="Bold" onClick={() => marker('**')} />
              <IconButton icon={Italic} label="Italic" onClick={() => marker('_')} />
              <IconButton icon={Code} label="Inline code" onClick={() => marker('`')} />
              <IconButton icon={Link} label="Link" onClick={() => marker('[', '](page)')} />
            </Show>
          </Show>
        </div>
      </Show>
      </div>
    </div>
  );
}
