/* Plain-DOM helpers the comment UI runs against the preview iframe's
   document (same origin). The locator pair locOf/elByLoc is a verbatim
   port of the old `wdoc serve --comment` client: a block's children are
   the nearest [data-wcl-block] descendants not separated by another
   block, and a locator is the /-joined child-index path from the page
   root ([data-wcl-page-file] wrapper) down to the element. Positional,
   not fuzzy — a stale locator resolves to null and its pin is dropped
   (the record itself survives in the sidecar). */

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

/** The anchored page in `doc`, or null (non-wdoc content, not yet loaded). */
export function pageInfo(doc) {
  const el = doc?.querySelector?.('[data-wcl-page-file]');
  if (!el) return null;
  return {
    el,
    name: el.getAttribute('data-wcl-page-name'),
    file: el.getAttribute('data-wcl-page-file'),
  };
}

/** Idempotently install the highlight/pin styles into the iframe head. */
export function injectCss(doc) {
  if (!doc?.head || doc.getElementById(CSS_ID)) return;
  const style = doc.createElement('style');
  style.id = CSS_ID;
  style.textContent = FRAME_CSS;
  doc.head.appendChild(style);
}

/** Nearest [data-wcl-block] descendants of `node` not separated from it
    by another block — the block tree's direct children. */
export function blockChildren(node) {
  return [...node.querySelectorAll('[data-wcl-block]')].filter((b) => {
    let p = b.parentElement;
    while (p && p !== node) {
      if (p.hasAttribute('data-wcl-block')) return false;
      p = p.parentElement;
    }
    return p === node;
  });
}

/** Child-index path from the page root to `el`, e.g. "0/2/1". */
export function locOf(pageEl, el) {
  const path = [];
  let cur = el;
  while (cur && cur !== pageEl) {
    let p = cur.parentElement;
    let pb = null;
    while (p && p !== pageEl) {
      if (p.hasAttribute('data-wcl-block')) {
        pb = p;
        break;
      }
      p = p.parentElement;
    }
    path.unshift(blockChildren(pb || pageEl).indexOf(cur));
    cur = pb;
  }
  return path.join('/');
}

/** Inverse of locOf; null when the path no longer resolves. */
export function elByLoc(pageEl, loc) {
  if (loc === '' || loc == null || !pageEl) return null;
  let node = pageEl;
  for (const part of String(loc).split('/')) {
    node = blockChildren(node)[+part];
    if (!node) return null;
  }
  return node;
}

/** Human description of a block: `kind — "first 60 chars"`. */
export function descOf(el) {
  const kind = el.getAttribute('data-wcl-kind') || 'block';
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

/** Re-place comment pins: clear previous marks, then mark every block
    comment whose locator still resolves. `onPin(comment)` handles clicks. */
export function placePins(doc, comments, onPin) {
  const page = pageInfo(doc);
  if (!page) return;
  for (const el of doc.querySelectorAll('[data-wcl-comment-id]')) {
    el.removeAttribute('data-wcl-comment-id');
    el.removeAttribute('data-wcl-comment');
    el.querySelectorAll(':scope > .wcl-pin').forEach((p) => p.remove());
  }
  for (const c of comments) {
    if (!c.loc) continue;
    const el = elByLoc(page.el, c.loc);
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
    const el = e.target.closest?.('[data-wcl-block]') ?? null;
    if (el !== hot) {
      clearHot();
      hot = el;
      if (hot) hot.classList.add('wcl-hot');
    }
  };
  const click = (e) => {
    const el = e.target.closest?.('[data-wcl-block]');
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
