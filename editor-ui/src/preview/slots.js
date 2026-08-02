/* Design-mode chrome for layout slots.

   The renderer emits one display:contents wrapper for every resolved slot,
   including page-owned holes with no content. This module turns those empty
   wrappers into visible, accessible insertion targets and builds the one
   page-addressed operation that fills them. Layout-owned fallback wrappers
   deliberately carry no data-wcl-page-file, so they never enter this path. */

import { ATTR, SEL, pageSlotOf, pageSpanOf } from './anchors';

const CSS_ID = 'wcl-slot-css';
const EMPTY_CLASS = 'wcl-wys-empty-slot';

const SLOT_CSS = `
.${EMPTY_CLASS} {
  width: 100%; min-height: 52px; box-sizing: border-box; margin: 8px 0;
  display: grid; place-items: center; padding: 12px 18px;
  border: 1px dashed rgba(37,99,235,.72); border-radius: 4px;
  background:
    repeating-linear-gradient(-45deg, rgba(37,99,235,.045) 0 7px, transparent 7px 14px),
    rgba(239,246,255,.55);
  color: #1d4ed8; cursor: pointer;
  font: 650 11px/1.2 ui-monospace, SFMono-Regular, Menlo, monospace;
  letter-spacing: .08em; text-transform: uppercase;
  transition: border-color .12s ease, background-color .12s ease, transform .12s ease;
}
.${EMPTY_CLASS}:hover {
  border-style: solid; background-color: rgba(219,234,254,.72); transform: translateY(-1px);
}
.${EMPTY_CLASS}.is-drop-target {
  border: 2px solid #2563eb; background-color: rgba(191,219,254,.86);
  box-shadow: 0 0 0 3px rgba(37,99,235,.16); transform: translateY(-1px);
}
.${EMPTY_CLASS}:focus-visible { outline: 2px solid #2563eb; outline-offset: 2px; }
@media (prefers-reduced-motion: reduce) { .${EMPTY_CLASS} { transition: none; } }
`;

function injectSlotCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = SLOT_CSS;
  doc.head.appendChild(style);
}

/** The page-owned target represented by a rendered slot wrapper. It has the
    same anchor-shaped fields the insertion modal already consumes, plus the
    slot name that changes the operation from sibling insertion to slot fill. */
export function slotTargetOf(el) {
  const slot = pageSlotOf(el);
  const file = el?.getAttribute?.(ATTR.pageFile);
  const span = pageSpanOf(el);
  if (!slot || !file || !span) return null;
  return { kind: 'slot', slot, file, span, el };
}

/** Build the public editor operation for inserting content into a resolved
    page slot. The server owns the WCL shape (`hero { ... }` versus loose
    reserved `content`) so the client never performs source surgery. */
export function slotInsertOp(target, source) {
  return {
    op: 'insert_slot',
    span: target.span,
    slot: target.slot,
    source,
  };
}

/** Build the atomic operation batch used by a gutter drop. The source slot
    is deliberately irrelevant: page content can move between any authored
    slots, while the server preserves (or creates) their structural wrappers. */
export function slotMoveOps(source, target, canonicalSource) {
  return [slotInsertOp(target, canonicalSource), { op: 'delete', span: source.span }];
}

/** The empty-slot target under a client point, or null. Kept in this layer
    so the gutter drag never learns the class name of chrome it does not own. */
export function emptySlotTargetAt(doc, x, y) {
  const hit = doc?.elementFromPoint?.(x, y);
  const button = hit?.closest?.(`.${EMPTY_CLASS}`) ?? null;
  const target = button && slotTargetOf(button.parentElement);
  return target ? { ...target, button } : null;
}

/** Keep exactly one empty slot painted as the current drop target. */
export function markEmptySlotDrop(doc, target) {
  for (const button of doc?.querySelectorAll?.(`.${EMPTY_CLASS}.is-drop-target`) ?? []) {
    if (button !== target?.button) button.classList.remove('is-drop-target');
  }
  target?.button?.classList.add('is-drop-target');
}

/** Replace every empty page-owned slot's invisible wrapper with a visible
    add button. Returns the number of holes decorated. Safe to call after
    every local edit: earlier chrome is removed before emptiness is tested. */
export function placeEmptySlots(doc, { onInsert } = {}) {
  if (!doc) return 0;
  injectSlotCss(doc);
  for (const old of doc.querySelectorAll(`.${EMPTY_CLASS}`)) old.remove();

  let count = 0;
  for (const root of doc.querySelectorAll(`${SEL.page}[${ATTR.pageSlot}]`)) {
    // Any rendered block — or non-anchor output — means the slot is already
    // occupied. Whitespace from the wrapper's pretty-printed HTML does not.
    if (root.querySelector(SEL.anchor) || root.children.length || root.textContent?.trim()) continue;
    const target = slotTargetOf(root);
    if (!target) continue;

    const button = doc.createElement('button');
    button.type = 'button';
    button.className = EMPTY_CLASS;
    button.textContent = `＋ Add content · ${target.slot} slot`;
    button.title = `Insert a block into the ${target.slot} slot`;
    button.onclick = (event) => {
      event.preventDefault();
      event.stopPropagation();
      onInsert?.(target);
    };
    root.appendChild(button);
    count += 1;
  }
  return count;
}
