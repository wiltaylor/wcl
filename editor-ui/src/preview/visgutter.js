/* Per-block gutter for the edit-mode preview iframes (the design canvas and
   every content-modal tab). Each top-level content block gets a small
   floating column on its left:

   - a drag handle — drag vertically to re-order the block among its
     same-file siblings (committed as a batch of `move` ops);
   - a profile button — pops up the visibility editor (which profiles the
     block shows in). On merged builds the button doubles as the indicator:
     amber when the block is hidden in some view, dashed when its
     visibility is custom (`@only` / other axes) — both read off the
     anchor, so the indicator can't disagree with what was built.

   Plain-DOM, same-origin, like frame.js. The wysiwyg click/move handlers
   ignore `.wcl-vis-gutter` hits so the controls own their events. */

import { anchorOf, blockChildren, pageInfo, sameFileSiblings } from './anchors';

const CSS_ID = 'wcl-vis-css';
/* The iframe holds a plain wdoc build with no Forge tokens — literal
   colors: blue for "shown in this view", muted gray for hidden, dashed
   amber for custom visibility. */
const VIS_CSS = `
.wcl-vis-host { position: relative; }
.wcl-vis-ghost { opacity: .45; }
.wcl-vis-gutter {
  position: absolute; left: -26px; top: 0; z-index: 2147483000;
  display: flex; flex-direction: column; gap: 2px; align-items: center;
  opacity: .35; transition: opacity .12s ease;
}
.wcl-vis-host:hover > .wcl-vis-gutter, .wcl-vis-gutter.is-dragging { opacity: 1; }
.wcl-vis-handle {
  width: 16px; height: 16px; border: 0; padding: 0; border-radius: 3px;
  background: transparent; color: #6b7280; cursor: grab;
  font: 700 11px/16px system-ui, sans-serif; text-align: center;
  touch-action: none;
}
.wcl-vis-handle:active { cursor: grabbing; }
.wcl-vis-profile {
  width: 16px; height: 16px; padding: 0; border-radius: 3px;
  border: 1px solid #6b7280; background: #f9fafb; color: #374151;
  font: 400 10px/14px system-ui, sans-serif; text-align: center;
  cursor: pointer; box-sizing: border-box;
}
.wcl-vis-profile.is-partial { border-color: #d97706; background: #fef3c7; color: #b45309; }
.wcl-vis-profile.is-custom { border-style: dashed; border-color: #d97706; color: #b45309; }
.wcl-vis-dropline {
  position: absolute; left: 0; right: 0; height: 0; z-index: 2147483001;
  border-top: 2px solid #2563eb; pointer-events: none;
}
`;

/** Idempotently install the gutter styles into the iframe head. */
export function injectVisCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = VIS_CSS;
  doc.head.appendChild(style);
}

/** How many positions (and in which direction) `from` must move among the
    ordered same-file block list `sameFile` to land before/after `to`. */
export function moveSteps(sameFile, fromIdx, dropIdx) {
  // dropIdx = the insertion slot (0 … len) in the same-file list.
  const target = dropIdx > fromIdx ? dropIdx - 1 : dropIdx;
  return target - fromIdx; // negative = up
}

/** Place the per-block gutters: clear previous ones, then decorate every
    top-level content block of the page (nested blocks keep the block
    toolbar). Options:
    - onProfile({file, span}) — the profile button: pop up the visibility
      editor for this block;
    - onReorder({file, span, steps, el, sameFile, dropIdx}) — a completed
      handle drag; steps < 0 moves up, > 0 down, among the block's
      same-file siblings (el/sameFile/dropIdx let the caller mirror the
      move in the DOM without a rebuild);
    - merged: the merged build — the profile button doubles as the
      indicator (amber = hidden in some view, dashed = custom visibility,
      read from the build's stamps);
    - currentSite: the merged build's underlying view — its hidden blocks
      get the ghost treatment;
    - enabled() — gate while committing/rebuilding. */
export function placeVisGutters(
  doc,
  { onProfile, onReorder, enabled = () => true, currentSite, merged = false } = {},
) {
  injectVisCss(doc);
  const page = pageInfo(doc);
  if (!page) return;
  for (const g of doc.querySelectorAll('.wcl-vis-gutter, .wcl-vis-dropline')) g.remove();
  for (const h of doc.querySelectorAll('.wcl-vis-host')) h.classList.remove('wcl-vis-host');
  for (const h of doc.querySelectorAll('.wcl-vis-ghost')) h.classList.remove('wcl-vis-ghost');
  const anchors = blockChildren(page.el)
    .map((el) => anchorOf(doc, el))
    .filter((a) => a && !a.chrome);
  for (const { el, file, span, except: exceptSites, vis } of anchors) {
    const custom = vis === 'custom';
    el.classList.add('wcl-vis-host');
    if (currentSite && exceptSites.includes(currentSite)) el.classList.add('wcl-vis-ghost');
    const gutter = doc.createElement('div');
    gutter.className = 'wcl-vis-gutter';

    if (onReorder) {
      const handle = doc.createElement('button');
      handle.type = 'button';
      handle.className = 'wcl-vis-handle';
      handle.textContent = '⋮⋮';
      handle.title = 'Drag to re-order';
      wireDrag(doc, handle, el, { onReorder, enabled });
      gutter.appendChild(handle);
    }

    if (onProfile) {
      const btn = doc.createElement('button');
      btn.type = 'button';
      btn.className = 'wcl-vis-profile';
      btn.textContent = '◐';
      if (merged && custom) {
        btn.classList.add('is-custom');
        btn.title = 'Custom visibility (@only / other axes) — opens the editor';
      } else if (merged && exceptSites.length) {
        btn.classList.add('is-partial');
        btn.title = `Hidden in: ${exceptSites.join(', ')} — click to change`;
      } else {
        btn.title = 'Set which profiles this block shows in';
      }
      btn.onclick = (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        if (!enabled()) return;
        onProfile({ file, span });
      };
      gutter.appendChild(btn);
    }

    el.appendChild(gutter);
  }
}

/** Handle-drag → insertion index among the dragged block's SAME-FILE
    siblings → `onReorder({file, span, steps})`. The drop line previews the
    slot; cross-file drops are refused (a move op batches on one file). */
function wireDrag(doc, handle, el, { onReorder, enabled }) {
  handle.addEventListener('pointerdown', (down) => {
    if (!enabled()) return;
    down.preventDefault();
    down.stopPropagation();
    const { file, span } = anchorOf(doc, el);
    const sameFile = sameFileSiblings(doc, el);
    const fromIdx = sameFile.indexOf(el);
    if (fromIdx < 0 || sameFile.length < 2) return;
    // No pointer capture — move/up listen on the document, and capturing a
    // synthetic pointer id throws.
    const gutter = handle.parentElement;
    gutter.classList.add('is-dragging');
    const line = doc.createElement('div');
    line.className = 'wcl-vis-dropline';
    let dropIdx = null;

    const slotAt = (clientY) => {
      // Insertion slot: before the first same-file block whose midpoint is
      // below the pointer; after the last otherwise.
      for (let i = 0; i < sameFile.length; i += 1) {
        const r = sameFile[i].getBoundingClientRect();
        if (clientY < r.top + r.height / 2) return i;
      }
      return sameFile.length;
    };
    const move = (ev) => {
      dropIdx = slotAt(ev.clientY);
      const before = sameFile[dropIdx];
      const anchorEl = before ?? sameFile[sameFile.length - 1];
      const r = anchorEl.getBoundingClientRect();
      const y = (before ? r.top : r.bottom) + (doc.defaultView?.scrollY ?? 0);
      line.style.top = `${y - 1}px`;
      if (!line.parentElement) doc.body.appendChild(line);
    };
    const finish = (commit) => {
      doc.removeEventListener('pointermove', move);
      doc.removeEventListener('pointerup', up);
      doc.removeEventListener('keydown', key, true);
      gutter.classList.remove('is-dragging');
      line.remove();
      if (!commit || dropIdx == null) return;
      const steps = moveSteps(sameFile, fromIdx, dropIdx);
      if (steps !== 0) onReorder({ file, span, steps, el, sameFile, dropIdx });
    };
    const up = () => finish(true);
    const key = (ev) => {
      if (ev.key === 'Escape') {
        ev.stopPropagation();
        finish(false);
      }
    };
    doc.addEventListener('pointermove', move);
    doc.addEventListener('pointerup', up);
    doc.addEventListener('keydown', key, true);
  });
}
