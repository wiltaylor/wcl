/* Unit tests for the diagram-shape interaction layer's pure parts:
   transform parsing, client→user mapping (viewBox fallback), the corner
   resize-delta math, and the layout-based drag gate. */

import { describe, expect, it } from 'vitest';

import { clientToUser, isDraggable, readTranslate, resizeDelta } from './diagram';

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
