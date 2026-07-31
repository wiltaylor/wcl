/* Unit tests for the in-place-commit DOM helpers: anchor patching from a
   span_map and the optimistic move + revert. The stamp readers/writers
   they build on (sibling resolution, span re-stamping, the visibility
   round-trip) are tested in anchors.test.js. Runs under happy-dom. */

import { describe, expect, it } from 'vitest';

import { mappedSpan, moveDomBlock, patchAnchors } from './localops';

const PAGE = `
<div data-wcl-page-file="unit.wcl" data-wcl-page-span="0:500" data-wcl-page-name="x">
  <p id="a" data-wcl-block data-wcl-span="10:20" data-wcl-file="unit.wcl">a</p>
  <p id="b" data-wcl-block data-wcl-span="30:40" data-wcl-file="unit.wcl">b</p>
  <h1 id="tpl" data-wcl-block data-wcl-span="10:20" data-wcl-file="template.wcl">tpl</h1>
  <p id="c" data-wcl-block data-wcl-span="50:60" data-wcl-file="unit.wcl">c</p>
  <div id="list" data-wcl-block data-wcl-span="70:200" data-wcl-file="unit.wcl">
    <p id="li1" data-wcl-block data-wcl-span="80:90" data-wcl-file="unit.wcl">one</p>
    <p id="li2" data-wcl-block data-wcl-span="100:110" data-wcl-file="unit.wcl">two</p>
  </div>
</div>`;

function setup() {
  document.body.innerHTML = PAGE;
  return document;
}

const MAP = [
  { from: { start: 10, end: 20 }, to: { start: 12, end: 22 } },
  { from: { start: 30, end: 40 }, to: { start: 32, end: 42 } },
  { from: { start: 0, end: 500 }, to: { start: 0, end: 510 } },
];

describe('patchAnchors', () => {
  it('rewrites matching spans for the file only', () => {
    setup();
    const n = patchAnchors(document, 'unit.wcl', MAP);
    expect(n).toBe(3); // a, b, page wrapper
    expect(document.getElementById('a').getAttribute('data-wcl-span')).toBe('12:22');
    expect(document.getElementById('b').getAttribute('data-wcl-span')).toBe('32:42');
    // Unmapped span untouched; other files untouched even on span collision.
    expect(document.getElementById('c').getAttribute('data-wcl-span')).toBe('50:60');
    expect(document.getElementById('tpl').getAttribute('data-wcl-span')).toBe('10:20');
    // The page wrapper's own span patched too.
    expect(
      document.querySelector('[data-wcl-page-file]').getAttribute('data-wcl-page-span'),
    ).toBe('0:510');
  });
});

describe('mappedSpan', () => {
  it('maps a known span and misses unknown ones', () => {
    expect(mappedSpan(MAP, { start: 10, end: 20 })).toEqual({ start: 12, end: 22 });
    expect(mappedSpan(MAP, { start: 1, end: 2 })).toBeNull();
  });
});

describe('moveDomBlock', () => {
  it('moves before a reference and reverts exactly', () => {
    setup();
    const a = document.getElementById('a');
    const c = document.getElementById('c');
    const revert = moveDomBlock(a, c);
    let order = [...document.querySelectorAll('p[id]')].map((e) => e.id);
    expect(order.slice(0, 3)).toEqual(['b', 'a', 'c']);
    revert();
    order = [...document.querySelectorAll('p[id]')].map((e) => e.id);
    expect(order.slice(0, 3)).toEqual(['a', 'b', 'c']);
  });

  it('moves to the end when the reference is null', () => {
    setup();
    const a = document.getElementById('a');
    const revert = moveDomBlock(a, null);
    expect(a.parentElement.lastElementChild).toBe(a);
    revert();
    expect(document.querySelector('[data-wcl-page-file]').children[0]).toBe(a);
  });
});
