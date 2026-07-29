/* Unit tests for the diagram-shape interaction layer: the pure parts
   (transform parsing, client→user mapping, resize-delta math, the layout
   drag gate) and the generalized move gesture (drag any shape; release
   resolves positional vs structural vs snap-back). */

import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  clientToUser,
  installShapeDrag,
  isDraggable,
  readTranslate,
  resizeDelta,
} from './diagram';

function setup(html) {
  document.body.innerHTML = html;
  return document;
}

describe('readTranslate', () => {
  it('parses space and comma separated translates', () => {
    setup('<g id="a" transform="translate(12 -3.5)"></g><g id="b" transform="translate(1,2)"></g>');
    expect(readTranslate(document.getElementById('a'))).toEqual({ x: 12, y: -3.5 });
    expect(readTranslate(document.getElementById('b'))).toEqual({ x: 1, y: 2 });
  });

  it('defaults to the origin without a transform', () => {
    setup('<g id="a"></g>');
    expect(readTranslate(document.getElementById('a'))).toEqual({ x: 0, y: 0 });
    expect(readTranslate(null)).toEqual({ x: 0, y: 0 });
  });
});

describe('clientToUser', () => {
  it('maps through the viewBox/rect scale fallback', () => {
    setup('<svg id="s" viewBox="10 20 200 100"></svg>');
    const svg = document.getElementById('s');
    // happy-dom has no layout: stub the client rect (2x horizontal scale,
    // 1x vertical) and leave getScreenCTM undefined.
    svg.getBoundingClientRect = () => ({ left: 50, top: 60, width: 100, height: 100 });
    svg.getScreenCTM = undefined;
    expect(clientToUser(svg, 50, 60)).toEqual({ x: 10, y: 20 });
    expect(clientToUser(svg, 100, 110)).toEqual({ x: 110, y: 70 });
  });

  it('passes through when no geometry is available', () => {
    setup('<svg id="s"></svg>');
    const svg = document.getElementById('s');
    svg.getScreenCTM = undefined;
    svg.getBoundingClientRect = () => ({ width: 0, height: 0 });
    expect(clientToUser(svg, 7, 9)).toEqual({ x: 7, y: 9 });
  });
});

describe('resizeDelta', () => {
  it('grows from the dragged corner, shifting x/y on top/left corners', () => {
    expect(resizeDelta('se', 10, 5)).toEqual({ dx: 0, dy: 0, dw: 10, dh: 5 });
    expect(resizeDelta('ne', 10, -5)).toEqual({ dx: 0, dy: -5, dw: 10, dh: 5 });
    expect(resizeDelta('sw', -10, 5)).toEqual({ dx: -10, dy: 0, dw: 10, dh: 5 });
    expect(resizeDelta('nw', -10, -5)).toEqual({ dx: -10, dy: -5, dw: 10, dh: 5 });
  });

  it('is delta-neutral: the opposite corner stays fixed', () => {
    // nw drag: bottom-right corner (x+w, y+h) must not move.
    const d = resizeDelta('nw', 4, 6);
    expect(d.dx + d.dw).toBe(0);
    expect(d.dy + d.dh).toBe(0);
  });
});

describe('isDraggable', () => {
  const shapeIn = (layout) => {
    setup(
      `<svg data-wcl-layout="${layout}"><g id="sh" data-wcl-shape></g></svg>`,
    );
    return document.getElementById('sh');
  };

  it('allows dragging under manual layouts only', () => {
    expect(isDraggable(shapeIn('free'))).toBe(true);
    expect(isDraggable(shapeIn('none'))).toBe(true);
    expect(isDraggable(shapeIn('layered'))).toBe(false);
    expect(isDraggable(shapeIn('force'))).toBe(false);
    expect(isDraggable(shapeIn('grid'))).toBe(false);
  });

  it('is false outside an annotated svg', () => {
    setup('<g id="sh" data-wcl-shape></g>');
    expect(isDraggable(document.getElementById('sh'))).toBe(false);
  });
});

describe('installShapeDrag — the generalized move gesture', () => {
  let teardown = null;
  afterEach(() => {
    teardown?.();
    teardown = null;
  });

  const ev = (type, x, y, target = document) =>
    target.dispatchEvent(
      new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y }),
    );

  /** A free-layout diagram with a container widget and a top-level leaf,
      the svg mapped 1:1 client→user (viewBox == stubbed client rect). */
  const wire = (handlers = {}) => {
    setup(`
      <svg id="svg" data-wcl-layout="free" viewBox="0 0 200 100">
        <g id="frame" data-wcl-shape data-wcl-kind="wf_browser"></g>
        <g id="label" data-wcl-shape data-wcl-kind="wf_label" transform="translate(10 10)"></g>
      </svg>`);
    const svg = document.getElementById('svg');
    svg.getScreenCTM = undefined;
    svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 });
    const h = {
      enabled: () => true,
      selectedShape: () => null,
      onMove: vi.fn(),
      onResize: vi.fn(),
      onRelocate: vi.fn(),
      acceptsChildren: (k) => k === 'wf_browser',
      ...handlers,
    };
    teardown = installShapeDrag(document, h);
    return h;
  };

  it('drags an UNSELECTED top-level shape positionally on its own diagram', () => {
    const h = wire();
    const label = document.getElementById('label');
    document.elementFromPoint = () => document.getElementById('svg');
    ev('pointerdown', 15, 15, label);
    ev('pointermove', 50, 40);
    ev('pointerup', 50, 40);
    expect(h.onMove).toHaveBeenCalledWith(label, { dx: 35, dy: 25 });
    expect(h.onRelocate).not.toHaveBeenCalled();
  });

  it('a sub-threshold press stays a click — nothing fires, nothing moves', () => {
    const h = wire();
    const label = document.getElementById('label');
    ev('pointerdown', 15, 15, label);
    ev('pointerup', 16, 15);
    expect(h.onMove).not.toHaveBeenCalled();
    expect(h.onRelocate).not.toHaveBeenCalled();
    expect(label.getAttribute('transform')).toBe('translate(10 10)');
  });

  it('a release over a container is a structural relocate, ghost snapped back', () => {
    const h = wire();
    const label = document.getElementById('label');
    let hitTransparent = null;
    document.elementFromPoint = () => {
      // Regression guard: the drop must resolve while the dragged ghost is
      // still pointer-events: none, or the hit is the ghost itself and the
      // drop degrades to a positional move.
      hitTransparent = label.style.pointerEvents;
      return document.getElementById('frame');
    };
    ev('pointerdown', 15, 15, label);
    ev('pointermove', 80, 60);
    ev('pointerup', 80, 60);
    expect(hitTransparent).toBe('none');
    expect(h.onMove).not.toHaveBeenCalled();
    expect(h.onRelocate).toHaveBeenCalledTimes(1);
    const [el, target, point] = h.onRelocate.mock.calls[0];
    expect(el).toBe(label);
    expect(target).toEqual({
      mode: 'inside',
      el: document.getElementById('frame'),
      slot: null,
      cellEl: null,
    });
    expect(point).toEqual({ x: 80, y: 60 });
    // The live ghost was restored before handing off.
    expect(label.getAttribute('transform')).toBe('translate(10 10)');
    expect(label.style.pointerEvents).toBe('');
  });

  it('a top-level drop onto a NESTED leaf is an ordering intent, not positional', () => {
    const h = wire();
    // Give the frame a nested leaf; drop the top-level label onto it.
    const frame = document.getElementById('frame');
    frame.innerHTML = '<g id="child" data-wcl-shape data-wcl-kind="wf_input"></g>';
    const label = document.getElementById('label');
    document.elementFromPoint = () => document.getElementById('child');
    ev('pointerdown', 15, 15, label);
    ev('pointermove', 80, 60);
    ev('pointerup', 80, 60);
    expect(h.onMove).not.toHaveBeenCalled();
    expect(h.onRelocate).toHaveBeenCalledTimes(1);
    expect(h.onRelocate.mock.calls[0][1]).toEqual({
      mode: 'after',
      el: document.getElementById('child'),
    });
  });

  it('a release with no valid target snaps back silently', () => {
    const h = wire();
    const label = document.getElementById('label');
    document.elementFromPoint = () => null;
    ev('pointerdown', 15, 15, label);
    ev('pointermove', 80, 60);
    expect(label.getAttribute('transform')).not.toBe('translate(10 10)');
    ev('pointerup', 80, 60);
    expect(h.onMove).not.toHaveBeenCalled();
    expect(h.onRelocate).not.toHaveBeenCalled();
    expect(label.getAttribute('transform')).toBe('translate(10 10)');
  });
});
