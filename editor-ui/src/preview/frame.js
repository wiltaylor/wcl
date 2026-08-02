/* Plain-DOM helpers the comment UI runs against the preview iframe's
   document (same origin). The locator pair locOf/elByLoc is a verbatim
   port of the old `wdoc serve --comment` client: a block's children come
   from the anchor module's block-tree walk, and a locator is the /-joined
   slot-qualified child-index path from the owning page wrapper down to the
   element. Positional, not fuzzy — a stale locator resolves to null and its
   pin is dropped (the record itself survives in the sidecar).

   The comment marks below (`data-wcl-comment*`) are this module's OWN
   client-side attributes, not build stamps — everything the build stamps
   is read through preview/anchors.js. */

import {
  SEL,
  blockChildren,
  containerOf,
  editButtonOf,
  kindOf,
  pageForSlot,
  pageInfo,
  pageRootOf,
  pageSlotOf,
} from './anchors';

const CSS_ID = 'wcl-comment-css';
/* The iframe holds a plain wdoc build with no Forge tokens, so these are
   literal colors: amber for comment marks, blue for the pick highlight. */
const FRAME_CSS = `
body.wcl-picking, body.wcl-picking * { cursor: crosshair !important; }
.wcl-hot { outline: 2px solid #3b82f6 !important; outline-offset: 2px; }
[data-wcl-comment-id] { outline: 2px dashed #d97706 !important; outline-offset: 3px; position: relative; }
.wcl-pin {
  position: absolute; top: -10px; right: -10px; z-index: 2147483000;
  width: 20px; height: 20px; border-radius: 999px; background: #d97706;
  color: #fff; font: 700 12px/20px system-ui, sans-serif; text-align: center;
  cursor: pointer; box-shadow: 0 1px 4px rgba(0,0,0,.4);
}
.wcl-flash { outline: 3px solid #d97706 !important; outline-offset: 3px;
  transition: outline-color .3s ease; }
`;

/** Idempotently install the highlight/pin styles into the iframe head. */
export function injectCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = FRAME_CSS;
  doc.head.appendChild(style);
}

const BARE_CSS_ID = 'wcl-bare-css';
/* Chrome-stripping for embedded previews (the graph view's content modal):
   hide the book template's nav chrome and let the reading column own the
   frame. An unindexed unit's page has no TOC entry anyway, so the sidebar
   only reads as broken there; deck/website templates lack these classes
   and are untouched. Training uses the book template too. */
const BARE_CSS = `
.book-sidebar, .book-rail, .book-pagenav { display: none !important; }
.book-content { margin-left: 0 !important; margin-right: 0 !important; }
.book-content .book-measure { padding: 1.5rem 2rem 3rem; min-height: auto; }
`;

/** Idempotently hide the book template chrome in a preview iframe. */
export function injectBareCss(doc) {
  if (!doc?.head || doc.getElementById(BARE_CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = BARE_CSS_ID;
  style.textContent = BARE_CSS;
  doc.head.appendChild(style);
}

/** Slot-qualified child-index path to `el`, e.g. "@hero/0/2/1". */
export function locOf(pageEl, el) {
  const root = pageRootOf(el) ?? pageEl;
  if (!root || !root.contains(el)) return null;
  const path = [];
  let cur = el;
  while (cur && cur !== root) {
    const container = containerOf(root, cur);
    const index = blockChildren(container).indexOf(cur);
    if (index < 0) return null;
    path.unshift(index);
    cur = container; // the page root ends the walk
  }
  const pathLoc = path.join('/');
  const slot = pageSlotOf(root);
  return slot ? `@${slot}/${pathLoc}` : pathLoc;
}

/** Inverse of locOf; null when the slot or path no longer resolves. Older
    unqualified locators remain relative to the reserved content slot. */
export function elByLoc(pageEl, loc) {
  if (loc === '' || loc == null || !pageEl) return null;
  const parts = String(loc).split('/');
  const doc = pageEl.ownerDocument;
  let node;
  if (parts[0]?.startsWith('@')) {
    node = pageForSlot(doc, parts.shift().slice(1));
  } else {
    node = pageForSlot(doc, 'content') ?? pageEl;
  }
  if (!node) return null;
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return null;
    node = blockChildren(node)[+part];
    if (!node) return null;
  }
  return node;
}

/** Human description of a block: `kind — "first 60 chars"`. */
export function descOf(el) {
  const kind = kindOf(el) || 'block';
  const txt = (el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 60);
  return txt ? `${kind} — "${txt}"` : kind;
}

/** The reviewer's current text selection inside the iframe, for `quote`. */
export function selectionQuote(win) {
  try {
    return String(win?.getSelection?.() ?? '').trim() || null;
  } catch {
    return null;
  }
}

function objectAnchor(doc, comment) {
  if (!comment.object_kind || !comment.object_id) return null;
  for (const el of doc.querySelectorAll(SEL.editButton)) {
    const object = editButtonOf(el);
    if (object?.kind === comment.object_kind && object.target === comment.object_id) return el;
  }
  return null;
}

/** Object addresses exposed by the rendered page's edit-object anchors. */
export function objectAddresses(doc) {
  if (!doc) return [];
  return [...doc.querySelectorAll(SEL.editButton)]
    .map((el) => editButtonOf(el))
    .filter((object) => object?.kind && object.target)
    .map((object) => ({ kind: object.kind, id: object.target }));
}

/** Re-place comment pins: clear previous marks, then mark every comment
    whose block locator or object address resolves. `onPin(comment)` handles
    clicks. */
export function placePins(doc, comments, onPin) {
  const page = pageInfo(doc);
  if (!page) return;
  for (const el of doc.querySelectorAll('[data-wcl-comment-id]')) {
    el.removeAttribute('data-wcl-comment-id');
    el.removeAttribute('data-wcl-comment');
    el.querySelectorAll(':scope > .wcl-pin').forEach((p) => p.remove());
  }
  for (const c of comments) {
    const el = c.loc ? elByLoc(page.el, c.loc) : objectAnchor(doc, c);
    if (!el) continue;
    el.setAttribute('data-wcl-comment-id', c.id);
    el.setAttribute('data-wcl-comment', c.body);
    const pin = doc.createElement('div');
    pin.className = 'wcl-pin';
    pin.textContent = '✓';
    pin.title = c.body;
    pin.onclick = (ev) => {
      ev.stopPropagation();
      ev.preventDefault();
      onPin?.(c);
    };
    el.appendChild(pin);
  }
}

/** Enter block-pick mode inside the iframe: crosshair + hover highlight;
    a click picks the block (suppressing navigation), Esc cancels. Returns
    a cancel() that also serves as the cleanup. */
export function beginPick(doc, { onPick, onCancel }) {
  let hot = null;
  const clearHot = () => {
    if (hot) {
      hot.classList.remove('wcl-hot');
      hot = null;
    }
  };
  const move = (e) => {
    const el = e.target.closest?.(SEL.block) ?? null;
    if (el !== hot) {
      clearHot();
      hot = el;
      if (hot) hot.classList.add('wcl-hot');
    }
  };
  const click = (e) => {
    const el = e.target.closest?.(SEL.block);
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    cancel(true);
    onPick?.(el);
  };
  const key = (e) => {
    if (e.key === 'Escape') cancel(false);
  };
  function cancel(picked) {
    clearHot();
    doc.body?.classList.remove('wcl-picking');
    doc.removeEventListener('mousemove', move);
    doc.removeEventListener('click', click, true);
    doc.removeEventListener('keydown', key);
    if (!picked) onCancel?.();
  }
  doc.body?.classList.add('wcl-picking');
  doc.addEventListener('mousemove', move);
  doc.addEventListener('click', click, true);
  doc.addEventListener('keydown', key);
  return () => cancel(true);
}

/** Idempotently wire the page's edit_object "Edit this …" buttons
    ([data-wcl-edit-kind], emitted by the edit-mode preview build): a
    capture-phase click hands {kind, target} to `onEdit`. `enabled()`
    gates it off (letting the click fall through) while block-pick mode
    owns the iframe's clicks. */
export function installEditButtons(doc, onEdit, enabled = () => true) {
  if (!doc || doc.__wclEditButtonsWired) return;
  doc.__wclEditButtonsWired = true;
  doc.addEventListener(
    'click',
    (e) => {
      const btn = editButtonOf(e.target);
      if (!btn || !enabled()) return;
      e.preventDefault();
      e.stopPropagation();
      onEdit({ kind: btn.kind, target: btn.target });
    },
    true,
  );
}

/** Scroll a comment's block into view and flash it. */
export function jumpTo(doc, comment) {
  const page = pageInfo(doc);
  if (!page) return false;
  const el = comment.loc ? elByLoc(page.el, comment.loc) : null;
  if (!el) return false;
  el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  el.classList.add('wcl-flash');
  setTimeout(() => el.classList.remove('wcl-flash'), 1600);
  return true;
}
