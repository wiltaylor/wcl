import { describe, expect, it } from 'vitest';

import { numExpr, numField, shapeOps } from './shapeops';

/** A palette diagram_kinds entry. */
const kindDef = (kind, fields) => ({
  kind,
  fields: fields.map((f) => (typeof f === 'string' ? { name: f, type: 'f64' } : f)),
});

/** A /api/block/source payload with number-literal fields. */
const source = (fields) => ({
  ok: true,
  fields: Object.fromEntries(
    Object.entries(fields).map(([name, v]) => [
      name,
      typeof v === 'object' ? v : { state: 'number', text: String(v) },
    ]),
  ),
});

const computed = { state: 'computed', text: 'a + b' };
const SPAN = { start: 10, end: 40 };

describe('shapeOps — move', () => {
  const rect = kindDef('rect', ['x', 'y']);

  it('shifts x/y by the delta', () => {
    const res = shapeOps({
      gesture: 'move',
      kind: 'rect',
      kindDef: rect,
      span: SPAN,
      source: source({ x: 10, y: 20 }),
      delta: { dx: 5, dy: -4 },
    });
    expect(res.ok).toBe(true);
    expect(res.ops).toEqual([
      { op: 'set_field', span: SPAN, field: 'x', expr: '15.0' },
      { op: 'set_field', span: SPAN, field: 'y', expr: '16.0' },
    ]);
  });

  it('treats an absent position as the origin', () => {
    const res = shapeOps({
      gesture: 'move',
      kind: 'rect',
      kindDef: rect,
      span: SPAN,
      source: source({}),
      delta: { dx: 12.5, dy: 0 },
    });
    expect(res.ops[0].expr).toBe('12.5');
    expect(res.ops[1].expr).toBe('0.0');
  });

  it('refuses a kind with no x/y', () => {
    const res = shapeOps({
      gesture: 'move',
      kind: 'label',
      kindDef: kindDef('label', ['text']),
      span: SPAN,
      source: source({}),
      delta: { dx: 1, dy: 1 },
    });
    expect(res.ok).toBe(false);
    expect(res.reason).toBe('no-position-field');
    expect(res.message).toContain('label');
  });

  it('refuses a computed position', () => {
    const res = shapeOps({
      gesture: 'move',
      kind: 'rect',
      kindDef: rect,
      span: SPAN,
      source: source({ x: computed, y: 3 }),
      delta: { dx: 1, dy: 1 },
    });
    expect(res.ok).toBe(false);
    expect(res.reason).toBe('computed-position');
  });
});

describe('shapeOps — resize', () => {
  const box = kindDef('rect', ['x', 'y', 'width', 'height']);
  const resize = (src, delta, bbox = null) =>
    shapeOps({
      gesture: 'resize',
      kind: 'rect',
      kindDef: box,
      span: SPAN,
      source: src,
      delta: { dx: 0, dy: 0, dw: 0, dh: 0, ...delta },
      box: bbox,
    });

  it('grows width/height from the written values', () => {
    const res = resize(source({ width: 100, height: 50 }), { dw: 20, dh: -10 });
    expect(res.ops).toEqual([
      { op: 'set_field', span: SPAN, field: 'width', expr: '120.0' },
      { op: 'set_field', span: SPAN, field: 'height', expr: '40.0' },
    ]);
  });

  it('starts an unwritten size from the rendered bbox', () => {
    const res = resize(source({}), { dw: 10, dh: 10 }, { width: 80, height: 30 });
    expect(res.ops[0].expr).toBe('90.0');
    expect(res.ops[1].expr).toBe('40.0');
  });

  it('never shrinks below the minimum', () => {
    const res = resize(source({ width: 20, height: 20 }), { dw: -100, dh: -100 });
    expect(res.ops[0].expr).toBe('8.0');
    expect(res.ops[1].expr).toBe('8.0');
  });

  it('moves x/y for a top-left grab', () => {
    const res = resize(source({ x: 5, y: 5, width: 40, height: 40 }), {
      dx: -5,
      dy: -5,
      dw: 5,
      dh: 5,
    });
    expect(res.ops.map((o) => [o.field, o.expr])).toEqual([
      ['width', '45.0'],
      ['height', '45.0'],
      ['x', '0.0'],
      ['y', '0.0'],
    ]);
  });

  it('rounds to integers for integer-typed fields', () => {
    const res = shapeOps({
      gesture: 'resize',
      kind: 'grid',
      kindDef: kindDef('grid', [
        { name: 'width', type: 'i32' },
        { name: 'height', type: 'i32' },
      ]),
      span: SPAN,
      source: source({ width: 10, height: 10 }),
      delta: { dx: 0, dy: 0, dw: 4.4, dh: 0 },
      box: null,
    });
    expect(res.ops[0].expr).toBe('14');
  });

  it('refuses a computed size — even with a rendered bbox to fall back on', () => {
    const res = resize(source({ width: computed, height: 20 }), { dw: 5 }, { width: 99, height: 99 });
    expect(res.ok).toBe(false);
    expect(res.reason).toBe('computed-size');
    expect(res.message).toContain('width');
  });

  it('refuses a kind with no width/height', () => {
    const res = shapeOps({
      gesture: 'resize',
      kind: 'line',
      kindDef: kindDef('line', ['x', 'y']),
      span: SPAN,
      source: source({}),
      delta: { dx: 0, dy: 0, dw: 1, dh: 1 },
    });
    expect(res.reason).toBe('no-size-field');
  });
});

describe('shapeOps — connect', () => {
  const connect = (from, to, owner = { span: SPAN, shared: false }) =>
    shapeOps({ gesture: 'connect', from, to, owner });

  it('writes a connection on the owning diagram', () => {
    const res = connect('a', 'b');
    expect(res.ops).toEqual([{ op: 'connect_add', span: SPAN, from: 'a', to: 'b' }]);
  });

  it('refuses when either shape has no id', () => {
    expect(connect('a', null).reason).toBe('missing-id');
    expect(connect('', 'b').reason).toBe('missing-id');
  });

  it('refuses a self-connection', () => {
    expect(connect('a', 'a').reason).toBe('self-connection');
  });

  it('refuses when no diagram owns the shapes', () => {
    expect(connect('a', 'b', null).reason).toBe('no-diagram');
  });

  it('refuses a generated diagram', () => {
    expect(connect('a', 'b', { span: SPAN, shared: true }).reason).toBe('generated');
  });
});

describe('shapeOps — relocate', () => {
  const src = { file: 'a.wcl', span: { start: 5, end: 15 }, shared: false };
  const relocate = (over) =>
    shapeOps({
      gesture: 'relocate',
      slice: 'wf_button "Go" {\n  x = 1.0\n  y = 2.0\n}',
      mode: 'inside',
      source: src,
      target: { file: 'a.wcl', span: { start: 40, end: 90 }, shared: false, layout: 'free' },
      ...over,
    });

  it('inserts inside a container and deletes the original', () => {
    const res = relocate({});
    expect(res.ops.map((o) => o.op)).toEqual(['insert_child', 'delete']);
    expect(res.ops[0].span).toEqual({ start: 40, end: 90 });
    expect(res.ops[1].span).toEqual(src.span);
  });

  it('inserts after a leaf sibling', () => {
    expect(relocate({ mode: 'after' }).ops[0].op).toBe('insert_after');
  });

  it('places the slice at the drop point on a manual-layout diagram', () => {
    const res = relocate({ mode: 'diagram', at: { x: 30, y: 40 } });
    expect(res.ops[0].source).toContain('x = 30.0');
    expect(res.ops[0].source).toContain('y = 40.0');
  });

  it('drops the drop point when the target layout places shapes itself', () => {
    const res = relocate({
      mode: 'diagram',
      at: { x: 30, y: 40 },
      target: { file: 'a.wcl', span: { start: 40, end: 90 }, shared: false, layout: 'layered' },
    });
    expect(res.ops[0].source).toContain('x = 1.0');
  });

  it('honours a resolved layout-guide slot', () => {
    expect(relocate({ slot: 2 }).ops[0].index).toBe(2);
  });

  it('refuses nesting into a kind the schema says takes no children', () => {
    const res = relocate({
      target: {
        kind: 'wf_button',
        file: 'a.wcl',
        span: { start: 40, end: 90 },
        shared: false,
        acceptsChildren: false,
      },
    });
    expect(res.reason).toBe('target-rejects-children');
    expect(res.message).toContain('wf_button');
  });

  it('refuses a move across files', () => {
    const res = relocate({
      target: { file: 'b.wcl', span: { start: 1, end: 2 }, shared: false, layout: 'free' },
    });
    expect(res.reason).toBe('cross-file');
  });

  it('refuses generated content on either end', () => {
    expect(relocate({ source: { ...src, shared: true } }).reason).toBe('generated');
  });
});

describe('shapeOps — convert to manual', () => {
  const rect = kindDef('rect', ['x', 'y']);

  it('materializes every child position and switches the layout', () => {
    const res = shapeOps({
      gesture: 'convert',
      span: SPAN,
      children: [
        { kindDef: rect, span: { start: 20, end: 25 }, at: { x: 12, y: 8 } },
        { kindDef: rect, span: { start: 26, end: 31 }, at: { x: 40.25, y: 0 } },
      ],
    });
    expect(res.skipped).toBe(0);
    expect(res.ops[0]).toEqual({ op: 'set_field', span: SPAN, field: 'layout', expr: ':free' });
    expect(res.ops.slice(1).map((o) => [o.span.start, o.field, o.expr])).toEqual([
      [20, 'x', '12.0'],
      [20, 'y', '8.0'],
      [26, 'x', '40.3'],
      [26, 'y', '0.0'],
    ]);
  });

  it('counts children whose kind cannot carry a position', () => {
    const res = shapeOps({
      gesture: 'convert',
      span: SPAN,
      children: [{ kindDef: kindDef('label', ['text']), span: SPAN, at: { x: 1, y: 1 } }],
    });
    expect(res.skipped).toBe(1);
    expect(res.ops).toHaveLength(1);
  });
});

describe('shapeOps — entry point', () => {
  it('refuses an unknown gesture rather than throwing', () => {
    expect(shapeOps({ gesture: 'wiggle' }).reason).toBe('unknown-gesture');
    expect(shapeOps(undefined).ok).toBe(false);
  });
});

describe('value helpers', () => {
  it('reads absent / literal / computed fields', () => {
    expect(numField(source({}), 'x')).toBe(0);
    expect(numField(source({ x: 4 }), 'x')).toBe(4);
    expect(Number.isNaN(numField(source({ x: computed }), 'x'))).toBe(true);
  });

  it('formats by the declared type', () => {
    expect(numExpr(kindDef('k', [{ name: 'n', type: 'u32' }]), 'n', 4.6)).toBe('5');
    expect(numExpr(kindDef('k', [{ name: 'n', type: 'f64' }]), 'n', 4)).toBe('4.0');
  });
});
