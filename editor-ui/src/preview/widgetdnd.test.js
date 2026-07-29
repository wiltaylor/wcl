/* Unit tests for the widget drop resolution — the shared brain of palette
   drops and canvas widget moves. Pure element walks; the caller does the
   elementFromPoint. */

import { beforeEach, describe, expect, it } from 'vitest';

import {
  markDropTarget,
  placeSliceAt,
  relocateOps,
  resolveWidgetDrop,
  widgetTreeFrom,
} from './widgetdnd';

const CONTAINERS = new Set(['wf_browser', 'wf_panel', 'wf_grid']);
const accepts = (kind) => CONTAINERS.has(kind);

/* svg[free]
     └ g#frame (wf_browser, container)
         └ g#input (wf_input, leaf)
             └ rect#inputrect
     └ g#label (wf_label, leaf)
     └ g#grid (wf_grid, container)
         └ g[data-wf-guide] (the renderer's edit-mode guide chrome)
             └ rect#cell0 / rect#cell3 (data-wf-slot drop zones) + kind tag
         └ g#gbtn (wf_button, leaf)
   plus an off-canvas div. */
beforeEach(() => {
  document.body.innerHTML = `
    <svg id="svg" data-wcl-layout="free">
      <g id="frame" data-wcl-shape data-wcl-kind="wf_browser">
        <g id="input" data-wcl-shape data-wcl-kind="wf_input">
          <rect id="inputrect"></rect>
        </g>
      </g>
      <g id="label" data-wcl-shape data-wcl-kind="wf_label"></g>
      <g id="grid" data-wcl-shape data-wcl-kind="wf_grid">
        <g data-wf-guide="1">
          <rect id="cell0" data-wf-slot="0"></rect>
          <rect id="cell3" data-wf-slot="3"></rect>
          <text>grid ·2</text>
        </g>
        <g id="gbtn" data-wcl-shape data-wcl-kind="wf_button"><text>OK</text></g>
      </g>
    </svg>
    <div id="outside"></div>`;
});

const el = (id) => document.getElementById(id);

describe('resolveWidgetDrop', () => {
  it('drops after a leaf widget', () => {
    expect(resolveWidgetDrop(el('label'), accepts)).toEqual({ mode: 'after', el: el('label') });
  });

  it('drops inside a container widget', () => {
    expect(resolveWidgetDrop(el('frame'), accepts)).toEqual({
      mode: 'inside',
      el: el('frame'),
      slot: null,
      cellEl: null,
    });
  });

  it('resolves a layout-guide zone to its insertion slot', () => {
    expect(resolveWidgetDrop(el('cell3'), accepts)).toEqual({
      mode: 'inside',
      el: el('grid'),
      slot: 3,
      cellEl: el('cell3'),
    });
    // A hit on the container outside any slot zone appends (slot null).
    expect(resolveWidgetDrop(el('grid'), accepts)).toEqual({
      mode: 'inside',
      el: el('grid'),
      slot: null,
      cellEl: null,
    });
  });

  it('a nested leaf beats its container (hit through its own markup)', () => {
    expect(resolveWidgetDrop(el('inputrect'), accepts)).toEqual({
      mode: 'after',
      el: el('input'),
    });
  });

  it('the diagram background is a diagram drop', () => {
    expect(resolveWidgetDrop(el('svg'), accepts)).toEqual({ mode: 'diagram', el: el('svg') });
  });

  it('is null off the canvas and for null hits', () => {
    expect(resolveWidgetDrop(el('outside'), accepts)).toBeNull();
    expect(resolveWidgetDrop(null, accepts)).toBeNull();
  });

  it('rewrites or inserts x/y when placing a slice', () => {
    const moved = placeSliceAt('wf_button b {\n  x = 20.0\n  y = 20.0\n  text = "Go"\n}', {
      x: 141.27,
      y: 88,
    });
    expect(moved).toContain('x = 141.3');
    expect(moved).toContain('y = 88.0');
    expect(moved).toContain('text = "Go"');
    // Fields absent from the slice are inserted after the opening brace.
    const inserted = placeSliceAt('wf_button b {\n  text = "Go"\n}', { x: 5, y: 6 });
    expect(inserted).toMatch(/\{\n {2}x = 5\.0\n {2}y = 6\.0/);
    // A braceless canonical print grows a body.
    expect(placeSliceAt('wf_label "Hi"', { x: 1, y: 2 })).toContain('x = 1.0');
  });

  it('skips the moved subtree — resolution continues outward', () => {
    // Moving the input: a drop on its own markup resolves to the enclosing
    // container, never to itself.
    expect(resolveWidgetDrop(el('inputrect'), accepts, el('input'))).toEqual({
      mode: 'inside',
      el: el('frame'),
      slot: null,
      cellEl: null,
    });
    // Moving the frame: a drop on its nested input walks past the whole
    // subtree and lands on the diagram.
    expect(resolveWidgetDrop(el('inputrect'), accepts, el('frame'))).toEqual({
      mode: 'diagram',
      el: el('svg'),
    });
    // Moving the grid itself: its own slot zones are inside the excluded
    // subtree, so the drop walks out to the diagram — no self-slotting.
    expect(resolveWidgetDrop(el('cell0'), accepts, el('grid'))).toEqual({
      mode: 'diagram',
      el: el('svg'),
    });
    // Moving the grid's button: a drop on a slot zone still targets the
    // grid at that position (the zone isn't in the moved subtree).
    expect(resolveWidgetDrop(el('cell0'), accepts, el('gbtn'))).toEqual({
      mode: 'inside',
      el: el('grid'),
      slot: 0,
      cellEl: el('cell0'),
    });
  });
});

describe('markDropTarget', () => {
  it('keeps exactly one element highlighted', () => {
    markDropTarget(document, el('frame'));
    expect(el('frame').classList.contains('wcl-wys-drop')).toBe(true);
    markDropTarget(document, el('label'));
    expect(el('frame').classList.contains('wcl-wys-drop')).toBe(false);
    expect(el('label').classList.contains('wcl-wys-drop')).toBe(true);
    markDropTarget(document, null);
    expect(document.querySelectorAll('.wcl-wys-drop').length).toBe(0);
  });

  it('highlights the resolved slot zone alongside the container', () => {
    markDropTarget(document, el('grid'), el('cell3'));
    expect(el('grid').classList.contains('wcl-wys-drop')).toBe(true);
    expect(el('cell3').classList.contains('wcl-wys-drop-cell')).toBe(true);
    // Moving to a different cell swaps the mark; clearing clears both.
    markDropTarget(document, el('grid'), el('cell0'));
    expect(el('cell3').classList.contains('wcl-wys-drop-cell')).toBe(false);
    expect(el('cell0').classList.contains('wcl-wys-drop-cell')).toBe(true);
    markDropTarget(document, null);
    expect(document.querySelectorAll('.wcl-wys-drop, .wcl-wys-drop-cell').length).toBe(0);
  });
});

describe('relocateOps', () => {
  const spans = { targetSpan: { start: 5, end: 9 }, sourceSpan: { start: 20, end: 40 } };

  it('builds the insert+delete batch for each mode', () => {
    const after = relocateOps({ slice: 'wf_button b {}', mode: 'after', ...spans });
    expect(after).toEqual([
      { op: 'insert_after', span: spans.targetSpan, source: 'wf_button b {}' },
      { op: 'delete', span: spans.sourceSpan },
    ]);
    const inside = relocateOps({ slice: 'wf_button b {}', mode: 'inside', ...spans });
    expect(inside[0]).toEqual({
      op: 'insert_child',
      span: spans.targetSpan,
      index: 9999,
      source: 'wf_button b {}',
    });
  });

  it('inserts at the resolved slot instead of appending', () => {
    const [insert] = relocateOps({ slice: 'wf_button b {}', mode: 'inside', ...spans, slot: 2 });
    expect(insert.index).toBe(2);
    // Slot 0 is a real position, not a falsy append.
    const [first] = relocateOps({ slice: 'wf_button b {}', mode: 'inside', ...spans, slot: 0 });
    expect(first.index).toBe(0);
  });

  it('rewrites the slice position for a diagram drop with `at`', () => {
    const [insert] = relocateOps({
      slice: 'wf_button b {\n  x = 1.0\n  y = 2.0\n}',
      mode: 'diagram',
      ...spans,
      at: { x: 33, y: 44 },
    });
    expect(insert.source).toContain('x = 33.0');
    expect(insert.source).toContain('y = 44.0');
  });
});

describe('widgetTreeFrom', () => {
  it('walks the anchored structure with nesting and labels', () => {
    document.getElementById('label').innerHTML = '<text>Send</text>';
    const roots = widgetTreeFrom(document);
    expect(roots).toHaveLength(1);
    const [root] = roots;
    expect(root.kind).toBe('diagram');
    expect(root.children.map((c) => c.kind)).toEqual(['wf_browser', 'wf_label', 'wf_grid']);
    // The frame's nested input is a CHILD of the frame, not of the root.
    expect(root.children[0].children.map((c) => c.kind)).toEqual(['wf_input']);
    // Labels: own rendered text (not a descendant's), falling back to null.
    expect(root.children[1].label).toBe('Send');
    expect(root.children[0].label).toBeNull();
    // Live elements ride along for selection/drag.
    expect(root.children[0].el).toBe(document.getElementById('frame'));
  });

  it('ignores the layout-guide chrome — its tag never becomes a label', () => {
    const roots = widgetTreeFrom(document);
    const grid = roots[0].children[2];
    expect(grid.kind).toBe('wf_grid');
    // The guide's "grid ·2" tag is skipped; the button's text is its OWN.
    expect(grid.label).toBeNull();
    expect(grid.children.map((c) => c.kind)).toEqual(['wf_button']);
    expect(grid.children[0].label).toBe('OK');
  });
});
