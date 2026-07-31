/* Plain-DOM Design-mode layer for the preview iframe (same origin).
   Hover/selection chrome and the contenteditable source-swap sessions live
   in-frame (they track content geometry natively through scroll/reflow);
   all Forge chrome (toolbar, modals) stays in the parent, positioned from
   rects (see DesignView). Blocks are addressed through preview/anchors.js,
   which owns the edit-mode stamp format — this layer names no attributes. */

import {
  ATTR,
  SEL,
  anchorChainAt,
  anchorElAt,
  anchorOf,
  editButtonOf,
  fieldBindingOf,
  isShape,
  outerAnchorEl,
  shapeBox,
  stashShapeBox,
} from './anchors';

const CSS_ID = 'wcl-design-css';
/* The iframe holds a plain wdoc build with no Forge tokens — literal colors:
   blue for hover/selection, violet for field bindings, amber while editing. */
const FRAME_CSS = `
[${ATTR.span}], ${SEL.field} { cursor: default; }
.wcl-wys-hot { outline: 1px solid rgba(59,130,246,.55) !important; outline-offset: 2px; }
.wcl-wys-selected { outline: 2px solid #3b82f6 !important; outline-offset: 2px; }
.wcl-wys-selected.wcl-wys-shared { outline-color: #8b5cf6 !important; }
.wcl-wys-editing {
  outline: 2px solid #d97706 !important; outline-offset: 2px;
  background: rgba(217,119,6,.07);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
  font-size: 0.92em !important; font-style: normal !important; font-weight: 400 !important;
  white-space: pre-wrap;
}
.wcl-wys-editing * { font: inherit !important; }
.wcl-wys-saving { opacity: .55; pointer-events: none; }
.wcl-wys-locked { outline: 2px dashed #6b7280 !important; outline-offset: 2px; }
.wcl-wys-sel-box {
  fill: none; stroke: #3b82f6; stroke-width: 1.5; stroke-dasharray: 4 3;
  vector-effect: non-scaling-stroke; pointer-events: none;
}
.wcl-wys-sel-box.wcl-wys-shared { stroke: #8b5cf6; }
`;

/** Idempotently install the Design-mode styles into the iframe head. */
export function injectDesignCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = FRAME_CSS;
  doc.head.appendChild(style);
}

/** Install the Design-mode interaction layer. `handlers`:
    - onSelect(anchor|null) — click selected/deselected a block
    - onEditIntent(anchor, regionEl) — click inside the already-selected
      block's text (or an edit_field region): start a text session
    - onFieldIntent(binding) — click on an edit_field-bound region
    - enabled() — gate (false while committing/rebuilding)
    Returns a teardown. Idempotent per document. */
export function installDesign(doc, handlers) {
  if (!doc || doc.__wclDesignWired) return () => {};
  doc.__wclDesignWired = true;
  injectDesignCss(doc);

  let hot = null;
  const clearHot = () => {
    hot?.classList.remove('wcl-wys-hot');
    hot = null;
  };
  const move = (e) => {
    if (!handlers.enabled() || e.target.closest?.('.wcl-vis-gutter')) {
      clearHot();
      return;
    }
    const el = anchorElAt(e.target) ?? fieldBindingOf(e.target)?.el ?? null;
    if (el !== hot) {
      clearHot();
      hot = el;
      hot?.classList.add('wcl-wys-hot');
    }
  };
  const click = (e) => {
    if (!handlers.enabled()) return;
    // Let the existing edit_object buttons keep their own capture handler.
    if (editButtonOf(e.target)) return;
    // The merged view's visibility chips own their clicks (visgutter.js).
    if (e.target.closest?.('.wcl-vis-gutter')) return;
    // An editing session owns its element's clicks.
    if (e.target.closest?.('.wcl-wys-editing')) return;
    const binding = fieldBindingOf(e.target);
    const chain = anchorChainAt(e.target);
    const el = chain[0] ?? null;
    if (!el && !binding) {
      handlers.onSelect?.(null);
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (binding && (!el || binding.el.contains(el) || el.contains(binding.el))) {
      handlers.onFieldIntent?.(binding, e);
      return;
    }
    // A fresh click selects the NEAREST anchor — a diagram shape as much as
    // a table or an `li` (a shape click shows its properties panel at once;
    // the diagram itself is selected by clicking its background, and Esc
    // pops outward through the chain). A click inside the current selection
    // drills one level toward the target; a click on the already-innermost
    // anchor fires the edit intent.
    const selected = doc.querySelector('.wcl-wys-selected');
    const idx = chain.indexOf(selected);
    if (idx === 0) {
      const anchor = anchorOf(doc, el);
      if (anchor) handlers.onEditIntent?.(anchor, e.target, e);
      return;
    }
    const next = idx > 0 ? chain[idx - 1] : chain[0];
    const anchor = next ? anchorOf(doc, next) : null;
    if (anchor) handlers.onSelect?.(anchor);
  };
  const onKeyDown = (e) => {
    if (e.key !== 'Escape' || !handlers.enabled()) return;
    // An active text session owns Escape (it cancels the session).
    if (doc.querySelector('.wcl-wys-editing')) return;
    const selected = doc.querySelector('.wcl-wys-selected');
    if (!selected) return;
    e.preventDefault();
    // Pop the selection one anchor level up; at the top, deselect.
    const parent = outerAnchorEl(selected);
    handlers.onSelect?.(parent ? anchorOf(doc, parent) : null);
  };
  doc.addEventListener('mousemove', move);
  doc.addEventListener('click', click, true);
  doc.addEventListener('keydown', onKeyDown, true);
  return () => {
    clearHot();
    doc.removeEventListener('mousemove', move);
    doc.removeEventListener('click', click, true);
    doc.removeEventListener('keydown', onKeyDown, true);
    delete doc.__wclDesignWired;
  };
}

/** Mark `el` as the selected block (clearing any previous selection).
    SVG shape anchors additionally get an injected bbox rect — CSS outlines
    don't render on SVG <g> elements. */
export function markSelected(doc, el, shared) {
  for (const s of doc.querySelectorAll('.wcl-wys-selected')) {
    s.classList.remove('wcl-wys-selected', 'wcl-wys-shared');
  }
  for (const r of doc.querySelectorAll('.wcl-wys-sel-box')) r.remove();
  if (el) {
    el.classList.add('wcl-wys-selected');
    if (shared) el.classList.add('wcl-wys-shared');
    if (isShape(el)) addShapeSelBox(doc, el, shared);
  }
}

/** Append a dashed selection rect inside the shape's own <g> (so it rides
    the g's transform, including a live drag preview). Sized from the
    anchor module's geometry store — the measurement taken before any
    chrome went in, so re-selecting a shape can't grow its outline. Skipped
    when the shape can't be measured (no SVG renderer, detached node). */
function addShapeSelBox(doc, el, shared) {
  const box = shapeBox(el);
  if (!box) return;
  stashShapeBox(el, box);
  const pad = 3;
  const rect = doc.createElementNS('http://www.w3.org/2000/svg', 'rect');
  rect.setAttribute('class', `wcl-wys-sel-box${shared ? ' wcl-wys-shared' : ''}`);
  rect.setAttribute('x', box.x - pad);
  rect.setAttribute('y', box.y - pad);
  rect.setAttribute('width', box.width + 2 * pad);
  rect.setAttribute('height', box.height + 2 * pad);
  el.appendChild(rect);
}

// ---------------------------------------------------------------------------
// Source-swap text session
// ---------------------------------------------------------------------------

/** Begin a source-swap editing session on `regionEl`: its rendered children
    are replaced by the raw markup `initial` in a plaintext contenteditable;
    surrounding blocks stay rendered.

    `opts`:
    - onCommit(text)          — Ctrl+Enter or blur with changes
    - onEnter(before, after)  — Enter split-at-caret (p / li); falls back to
                                onCommit when absent
    - onCancel()              — Esc (the DOM is restored first)
    - caretAt                 — best-effort caret placement: a client point
                                {x, y} from the triggering click, else end

    Returns { finish(saved), textNow() } — finish restores/keeps state
    without firing callbacks (the orchestrator drives what happens next). */
export function beginTextSession(doc, regionEl, initial, opts) {
  const original = [...regionEl.childNodes];
  const prevEditable = regionEl.getAttribute('contenteditable');
  regionEl.textContent = initial;
  // plaintext-only keeps paste/typing as text (Chromium/WebKit); fall back
  // to plain contenteditable + input filtering elsewhere.
  regionEl.setAttribute('contenteditable', 'plaintext-only');
  if (!regionEl.isContentEditable) regionEl.setAttribute('contenteditable', 'true');
  regionEl.classList.add('wcl-wys-editing');
  regionEl.focus();
  placeCaret(doc, regionEl, opts?.caretAt);

  let done = false;
  const textNow = () => regionEl.textContent ?? '';

  const finish = (saved) => {
    if (done) return;
    done = true;
    regionEl.removeEventListener('keydown', onKey);
    regionEl.removeEventListener('blur', onBlur);
    regionEl.classList.remove('wcl-wys-editing');
    if (prevEditable === null) regionEl.removeAttribute('contenteditable');
    else regionEl.setAttribute('contenteditable', prevEditable);
    if (!saved) {
      regionEl.textContent = '';
      regionEl.append(...original);
    }
  };

  const onKey = (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
      opts.onCancel?.();
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      const text = textNow();
      finish(true);
      opts.onCommit?.(text);
    } else if (e.key === 'Enter' && opts.onEnter) {
      e.preventDefault();
      const [before, after] = splitAtCaret(doc, regionEl);
      finish(true);
      opts.onEnter(before, after);
    }
  };
  const onBlur = () => {
    // Blur commits when dirty, cancels when untouched. Deferred a tick so a
    // click that finishes the session elsewhere doesn't double-fire.
    setTimeout(() => {
      if (done) return;
      const text = textNow();
      if (text === initial) {
        finish(false);
        opts.onCancel?.();
      } else {
        finish(true);
        opts.onCommit?.(text);
      }
    }, 0);
  };
  regionEl.addEventListener('keydown', onKey);
  regionEl.addEventListener('blur', onBlur);
  return { finish, textNow };
}

/** Place the caret near a client point inside `el`, else at the end. */
function placeCaret(doc, el, point) {
  const sel = doc.defaultView?.getSelection?.();
  if (!sel) return;
  let range = null;
  if (point && doc.caretRangeFromPoint) {
    const r = doc.caretRangeFromPoint(point.x, point.y);
    if (r && el.contains(r.startContainer)) range = r;
  }
  if (!range) {
    range = doc.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
  }
  sel.removeAllRanges();
  sel.addRange(range);
}

/** The session text split at the caret (falling back to everything /
    nothing when the selection is unavailable). */
function splitAtCaret(doc, el) {
  const text = el.textContent ?? '';
  const sel = doc.defaultView?.getSelection?.();
  if (!sel?.rangeCount || !el.contains(sel.anchorNode)) return [text, ''];
  const range = sel.getRangeAt(0).cloneRange();
  range.selectNodeContents(el);
  range.setEnd(sel.getRangeAt(0).startContainer, sel.getRangeAt(0).startOffset);
  const before = range.toString();
  return [before, text.slice(before.length)];
}

/** Wrap the current selection inside an active session with `open`/`close`
    markers (pure string surgery — no execCommand). Marks the whole text
    when there is no selection inside the element. */
export function wrapSelection(doc, el, open, close) {
  const sel = doc.defaultView?.getSelection?.();
  const text = el.textContent ?? '';
  if (!sel?.rangeCount || !el.contains(sel.anchorNode) || sel.isCollapsed) {
    el.textContent = `${open}${text}${close}`;
    return;
  }
  const range = sel.getRangeAt(0).cloneRange();
  range.selectNodeContents(el);
  range.setEnd(sel.getRangeAt(0).startContainer, sel.getRangeAt(0).startOffset);
  const start = range.toString().length;
  const len = String(sel).length;
  el.textContent = text.slice(0, start) + open + text.slice(start, start + len) + close + text.slice(start + len);
}

/** WCL double-quoted string literal for `text` (fragment building). */
export function wclString(text) {
  return `"${String(text).replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\t/g, '\\t')}"`;
}
