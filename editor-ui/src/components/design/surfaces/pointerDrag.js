/* A parent-document pointer drag: capture on the source element (REAL
   pointer ids — safe, unlike the synthetic ids inside the iframe the
   project memory warns about), a 4px threshold separating clicks from
   drags, a floating ghost chip following the cursor, Escape to cancel.

   Used by the widget palette and the structure tree instead of native
   HTML5 drag-and-drop: a parent-document HTML5 drag into the same-origin
   iframe proved unreliable in real browsers and impossible to drive from
   tests, while captured pointer events keep firing on the source element
   even while the cursor is over the iframe. */

const THRESHOLD = 4;

/**
 * Begin a drag from a `pointerdown` event. Callbacks:
 * - `chipText`        — the ghost chip's label
 * - `onMove(point)`   — cursor moved (page coords), past the threshold
 * - `onDrop(point)`   — released after a real drag
 * - `onClick()`       — released below the threshold (a plain click)
 * - `onCancel()`      — Escape / pointercancel; also fired so hosts can
 *                       clear any highlight they painted from `onMove`
 */
export function startPointerDrag(e, { chipText, onMove, onDrop, onClick, onCancel }) {
  if (e.button !== 0) return;
  const el = e.currentTarget;
  const start = { x: e.clientX, y: e.clientY };
  let chip = null;
  let moved = false;
  let done = false;

  const placeChip = (p) => {
    if (!chip) {
      chip = document.createElement('div');
      chip.className = 'ed-drag-chip';
      chip.textContent = chipText ?? '';
      document.body.appendChild(chip);
    }
    chip.style.left = `${p.x + 10}px`;
    chip.style.top = `${p.y + 8}px`;
  };

  const cleanup = () => {
    done = true;
    chip?.remove();
    chip = null;
    el.removeEventListener('pointermove', move);
    el.removeEventListener('pointerup', up);
    el.removeEventListener('pointercancel', cancel);
    window.removeEventListener('keydown', key, true);
    if (el.hasPointerCapture?.(e.pointerId)) el.releasePointerCapture(e.pointerId);
  };

  const move = (ev) => {
    const p = { x: ev.clientX, y: ev.clientY };
    if (!moved && Math.hypot(p.x - start.x, p.y - start.y) < THRESHOLD) return;
    moved = true;
    placeChip(p);
    onMove?.(p);
  };
  const up = (ev) => {
    if (done) return;
    const wasDrag = moved;
    cleanup();
    if (wasDrag) onDrop?.({ x: ev.clientX, y: ev.clientY });
    else onClick?.();
  };
  const cancel = () => {
    if (done) return;
    cleanup();
    onCancel?.();
  };
  const key = (ev) => {
    if (ev.key === 'Escape') cancel();
  };

  el.setPointerCapture?.(e.pointerId);
  el.addEventListener('pointermove', move);
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', cancel);
  window.addEventListener('keydown', key, true);
  // No text selection / focus flicker while dragging.
  e.preventDefault();
}
