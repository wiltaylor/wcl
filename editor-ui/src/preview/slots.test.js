/* Empty layout slots are a first-class Design-mode editing surface. These
   tests exercise the public DOM decoration and operation-builder seams: the
   rendered page wrappers go in, visible add targets and a page-addressed op
   come out. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { placeEmptySlots, slotInsertOp, slotMoveOps } from './slots';

beforeEach(() => {
  document.head.innerHTML = '';
  document.body.innerHTML = `
    <div data-wcl-page-file="page.wcl" data-wcl-page-span="10:200"
         data-wcl-page-name="index" data-wcl-slot="content">
      <p data-wcl-file="page.wcl" data-wcl-span="40:60" data-wcl-kind="p">Body</p>
    </div>
    <div id="hero" data-wcl-page-file="page.wcl" data-wcl-page-span="10:200"
         data-wcl-page-name="index" data-wcl-slot="hero"></div>
    <div id="fallback" data-wcl-file="layout.wcl" data-wcl-span="80:120"
         data-wcl-page-name="index" data-wcl-slot="footer"></div>`;
});

describe('placeEmptySlots', () => {
  it('makes only unfilled page-owned slots visible and fillable', () => {
    const onInsert = vi.fn();
    expect(placeEmptySlots(document, { onInsert })).toBe(1);

    const target = document.querySelector('.wcl-wys-empty-slot');
    expect(target?.textContent).toContain('hero');
    expect(target?.tagName).toBe('BUTTON');
    expect(document.querySelector('[data-wcl-slot="content"] .wcl-wys-empty-slot')).toBeNull();
    expect(document.querySelector('#fallback .wcl-wys-empty-slot')).toBeNull();

    target.click();
    expect(onInsert).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'slot',
        slot: 'hero',
        file: 'page.wcl',
        span: { start: 10, end: 200 },
        el: document.getElementById('hero'),
      }),
    );
  });

  it('is idempotent when the surface redecorates', () => {
    placeEmptySlots(document);
    placeEmptySlots(document);
    expect(document.querySelectorAll('.wcl-wys-empty-slot')).toHaveLength(1);
  });
});

describe('slotInsertOp', () => {
  it('addresses the page and names the destination slot', () => {
    const target = {
      kind: 'slot',
      slot: 'hero',
      file: 'page.wcl',
      span: { start: 10, end: 200 },
    };
    expect(slotInsertOp(target, 'h1 "Hello"')).toEqual({
      op: 'insert_slot',
      span: { start: 10, end: 200 },
      slot: 'hero',
      source: 'h1 "Hello"',
    });
  });

  it('builds one atomic move from a named source slot', () => {
    const target = {
      kind: 'slot',
      slot: 'sidebar',
      file: 'page.wcl',
      span: { start: 10, end: 200 },
    };
    const source = {
      slot: 'hero',
      file: 'page.wcl',
      span: { start: 40, end: 60 },
    };
    expect(slotMoveOps(source, target, 'h1 "Hello"')).toEqual([
      {
        op: 'insert_slot',
        span: { start: 10, end: 200 },
        slot: 'sidebar',
        source: 'h1 "Hello"',
      },
      { op: 'delete', span: { start: 40, end: 60 } },
    ]);
  });
});
