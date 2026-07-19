/* Plain-DOM Design-mode layer for the preview iframe (same origin).
   Hover/selection chrome and the contenteditable source-swap sessions live
   in-frame (they track content geometry natively through scroll/reflow);
   all Forge chrome (toolbar, modals) stays in the parent, positioned from
   rects (see DesignView). Blocks are addressed by the edit-mode anchors the
   build stamps: data-wcl-kind / data-wcl-span ("start:end" byte offsets) /
   data-wcl-file, plus the edit_field bindings data-wcl-field-*. */

const CSS_ID = 'wcl-design-css';
/* The iframe holds a plain wdoc build with no Forge tokens — literal colors:
   blue for hover/selection, violet for field bindings, amber while editing. */
const FRAME_CSS = `
[data-wcl-span], [data-wcl-field-name] { cursor: default; }
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
`;

/** Idempotently install the Design-mode styles into the iframe head. */
export function injectDesignCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = FRAME_CSS;
  doc.head.appendChild(style);
}

/** The anchor info of a block element: file + span + kind, plus whether the
    same (file, span) renders more than once in the page (repeater/template
    output — an edit affects every instance). */
export function anchorOf(doc, el) {
  const span = el.getAttribute('data-wcl-span');
  const file = el.getAttribute('data-wcl-file');
  if (!span || !file) return null;
  const [start, end] = span.split(':').map(Number);
  const dup = doc.querySelectorAll(
    `[data-wcl-span="${CSS.escape(span)}"][data-wcl-file="${CSS.escape(file)}"]`,
  );
  return {
    el,
    file,
    span: { start, end },
    kind: el.getAttribute('data-wcl-kind') || 'block',
    shared: dup.length > 1,
  };
}

/** The `edit_field` binding on (or above) `el`, or null. */
export function fieldBindingOf(el) {
  const bound = el.closest?.('[data-wcl-field-name]');
  if (!bound) return null;
  return {
    el: bound,
    kind: bound.getAttribute('data-wcl-field-kind'),
    target: bound.getAttribute('data-wcl-field-target') || null,
    field: bound.getAttribute('data-wcl-field-name'),
    plain: bound.hasAttribute('data-wcl-field-plain'),
  };
}

/** Template chrome (sidebar, rails, layout parts) comes from stdlib /
    registry sources (`<wcl-system>/…` paths) — not editable content. */
function isChrome(el) {
  const file = el?.getAttribute?.('data-wcl-file') ?? '';
  return file.startsWith('<');
}

/** The nearest selectable content-block element for an event target —
    skipping template chrome so its links/toggles keep working. */
export function blockAt(target) {
  let el = target?.closest?.('[data-wcl-span][data-wcl-file]') ?? null;
  while (el && isChrome(el)) {
    el = el.parentElement?.closest?.('[data-wcl-span][data-wcl-file]') ?? null;
  }
  return el;
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
    if (!handlers.enabled()) {
      clearHot();
      return;
    }
    const el = blockAt(e.target) ?? fieldBindingOf(e.target)?.el ?? null;
    if (el !== hot) {
      clearHot();
      hot = el;
      hot?.classList.add('wcl-wys-hot');
    }
  };
  const click = (e) => {
    if (!handlers.enabled()) return;
    // Let the existing edit_object buttons keep their own capture handler.
    if (e.target.closest?.('[data-wcl-edit-kind]')) return;
    // An editing session owns its element's clicks.
    if (e.target.closest?.('.wcl-wys-editing')) return;
    const binding = fieldBindingOf(e.target);
    const el = blockAt(e.target);
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
    const anchor = anchorOf(doc, el);
    if (!anchor) return;
    const selected = doc.querySelector('.wcl-wys-selected');
    if (selected === el) handlers.onEditIntent?.(anchor, e.target, e);
    else handlers.onSelect?.(anchor);
  };
  doc.addEventListener('mousemove', move);
  doc.addEventListener('click', click, true);
  return () => {
    clearHot();
    doc.removeEventListener('mousemove', move);
    doc.removeEventListener('click', click, true);
    delete doc.__wclDesignWired;
  };
}

/** Mark `el` as the selected block (clearing any previous selection). */
export function markSelected(doc, el, shared) {
  for (const s of doc.querySelectorAll('.wcl-wys-selected')) {
    s.classList.remove('wcl-wys-selected', 'wcl-wys-shared');
  }
  if (el) {
    el.classList.add('wcl-wys-selected');
    if (shared) el.classList.add('wcl-wys-shared');
  }
}

/** Find a block element by its source binding (after a rebuild refreshed
    the anchors). */
export function elBySpan(doc, file, span) {
  return doc?.querySelector?.(
    `[data-wcl-span="${span.start}:${span.end}"][data-wcl-file="${CSS.escape(file)}"]`,
  );
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
