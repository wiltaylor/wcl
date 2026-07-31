/* Unit tests for the anchor model — the client-side owner of the edit-mode
   stamp format. Everything here builds a document fragment carrying stamps,
   asks the module about it and asserts what it reports: no server, no real
   build, and no assertion on WHICH attribute was read. Runs under
   happy-dom, like the rest of the preview layer's tests. */

import { beforeEach, describe, expect, it } from 'vitest';

import {
  adjacentSameFileSibling,
  anchorChainAt,
  anchorEls,
  anchorElAt,
  anchorOf,
  anchorsIn,
  blockChildren,
  closestOfKind,
  containerOf,
  edgeOf,
  editButtonOf,
  elBySpan,
  fieldBindingOf,
  isChrome,
  isManualLayout,
  outerAnchorEl,
  pageInfo,
  parseSpanAttr,
  prevSiblingOfKind,
  restampExcept,
  restampPageSpan,
  restampSpan,
  sameFileSiblings,
  shapeBox,
  shapeChildren,
  slotOf,
  spanKey,
  spanOf,
  stashShapeBox,
  visOf,
} from './anchors';

/* A page as an edit-mode merged build stamps it: content blocks from the
   unit's own file, one template-chrome block, a repeated (shared) block,
   a nested list, a diagram with a nested shape, an edit_field binding and
   an edit_object button. */
const PAGE = `
<div data-wcl-page-file="unit.wcl" data-wcl-page-span="0:900" data-wcl-page-name="concept_x">
  <p id="a" data-wcl-block data-wcl-kind="p"
     data-wcl-span="10:20" data-wcl-file="unit.wcl">a</p>
  <p id="b" data-wcl-block data-wcl-kind="p" data-wcl-span="30:40" data-wcl-file="unit.wcl"
     data-wcl-except="deck training" data-wcl-vis="custom">b</p>
  <h1 id="tpl" data-wcl-block data-wcl-kind="h1"
      data-wcl-span="10:20" data-wcl-file="chrome-placeholder">chrome</h1>
  <div id="list" data-wcl-block data-wcl-kind="list"
       data-wcl-span="70:200" data-wcl-file="unit.wcl">
    <p id="li1" data-wcl-block data-wcl-kind="li"
       data-wcl-span="80:90" data-wcl-file="unit.wcl">one</p>
    <span id="sep">·</span>
    <p id="li2" data-wcl-block data-wcl-kind="li"
       data-wcl-span="100:110" data-wcl-file="unit.wcl">two</p>
  </div>
  <p id="dup1" data-wcl-block data-wcl-kind="p"
     data-wcl-span="300:310" data-wcl-file="unit.wcl">repeated</p>
  <p id="dup2" data-wcl-block data-wcl-kind="p"
     data-wcl-span="300:310" data-wcl-file="unit.wcl">repeated</p>
  <svg id="dia" data-wcl-block data-wcl-kind="diagram" data-wcl-layout="free"
       data-wcl-span="400:600" data-wcl-file="unit.wcl">
    <g id="frame" data-wcl-shape data-wcl-kind="wf_browser" data-wcl-shape-id="frame"
       data-wcl-span="420:560" data-wcl-file="unit.wcl">
      <g data-wf-guide="1"><rect id="cell2" data-wf-slot="2"></rect></g>
      <g id="btn" data-wcl-shape data-wcl-kind="wf_button"
         data-wcl-span="440:520" data-wcl-file="unit.wcl"><rect id="btnrect"></rect></g>
    </g>
    <path id="edge" data-wcl-edge="frame:btn"></path>
    <path id="edgehit" data-wcl-edge="frame:btn"><title id="edgetitle">x</title></path>
    <path id="badedge" data-wcl-edge="oops"></path>
  </svg>
  <h2 id="bound" data-wcl-field-name="title" data-wcl-field-kind="screen"
      data-wcl-field-target="login" data-wcl-field-plain>Login</h2>
  <button id="editbtn" data-wcl-edit-kind="screen" data-wcl-edit-target="login">Edit this</button>
  <p id="badspan" data-wcl-block data-wcl-span="oops" data-wcl-file="unit.wcl">malformed</p>
</div>`;

function setup() {
  document.body.innerHTML = PAGE;
  // happy-dom leaves `&lt;` undecoded in attribute values, so stamp the
  // system path (what the build emits for template chrome) directly.
  document.getElementById('tpl').setAttribute('data-wcl-file', '<wcl-system>/templates.wcl');
  return document;
}

const el = (id) => document.getElementById(id);

beforeEach(setup);

describe('span parsing', () => {
  it('parses well-formed spans and rejects malformed ones', () => {
    expect(parseSpanAttr('10:20')).toEqual({ start: 10, end: 20 });
    expect(parseSpanAttr(null)).toBeNull();
    expect(parseSpanAttr('')).toBeNull();
    expect(parseSpanAttr('10')).toBeNull();
    expect(parseSpanAttr('oops')).toBeNull();
    expect(parseSpanAttr('10:')).toBeNull();
    expect(parseSpanAttr('1:2:3')).toBeNull();
    // A malformed stamp reads as no stamp — never a half-parsed span.
    expect(spanOf(el('badspan'))).toBeNull();
    expect(anchorOf(document, el('badspan'))).toBeNull();
  });

  it('round-trips a span through its attribute form', () => {
    expect(spanKey({ start: 3, end: 9 })).toBe('3:9');
    expect(spanKey('3:9')).toBe('3:9');
    expect(spanOf(el('a'))).toEqual({ start: 10, end: 20 });
  });
});

describe('anchorOf', () => {
  it('reads every stamped family off one element', () => {
    expect(anchorOf(document, el('a'))).toMatchObject({
      el: el('a'),
      file: 'unit.wcl',
      span: { start: 10, end: 20 },
      kind: 'p',
      shape: false,
      shapeId: null,
      layout: null,
      except: [],
      vis: null,
      chrome: false,
      shared: false,
      box: null,
    });
  });

  it('reads the visibility stamps a merged build adds', () => {
    expect(anchorOf(document, el('b'))).toMatchObject({
      except: ['deck', 'training'],
      vis: 'custom',
    });
    // The same fact through the gutter's view of it.
    // The read half speaks the anchor's own vocabulary — no renaming at
    // the call site, so the two halves cannot drift apart.
    expect(visOf(el('b'))).toEqual({ except: ['deck', 'training'], vis: 'custom' });
    expect(visOf(el('a'))).toEqual({ except: [], vis: null });
  });

  it('reads the diagram family, layout inherited from the owning svg', () => {
    expect(anchorOf(document, el('btn'))).toMatchObject({
      kind: 'wf_button',
      shape: true,
      shapeId: null,
      layout: 'free',
    });
    expect(anchorOf(document, el('frame'))).toMatchObject({ shape: true, shapeId: 'frame' });
    // The diagram's own anchor IS the svg, so it exposes the layout too.
    expect(anchorOf(document, el('dia'))).toMatchObject({ kind: 'diagram', layout: 'free' });
  });

  it('derives the shared and geometry facts only when they are read', () => {
    let measured = 0;
    el('btn').getBBox = () => {
      measured += 1;
      return { x: 0, y: 0, width: 10, height: 10 };
    };
    const a = anchorOf(document, el('btn'));
    expect(measured).toBe(0); // building the anchor forces no layout
    expect(a.box).toEqual({ x: 0, y: 0, width: 10, height: 10 });
    expect(a.box.width).toBe(10);
    expect(measured).toBe(1); // and the answer is remembered
    expect(a.shared).toBe(false);
  });

  it('is null for unanchored elements', () => {
    expect(anchorOf(document, el('sep'))).toBeNull();
    expect(anchorOf(document, null)).toBeNull();
  });

  it('defaults the kind and finds the document itself when none is passed', () => {
    el('a').removeAttribute('data-wcl-kind');
    expect(anchorOf(document, el('a')).kind).toBe('block');
    expect(anchorOf(null, el('a')).file).toBe('unit.wcl');
  });
});

describe('the chrome rule', () => {
  it('marks blocks declared by a stdlib / registry source', () => {
    expect(isChrome(el('tpl'))).toBe(true);
    expect(isChrome(el('a'))).toBe(false);
    expect(isChrome(el('sep'))).toBe(false);
    expect(anchorOf(document, el('tpl')).chrome).toBe(true);
    expect(anchorOf(document, el('a')).chrome).toBe(false);
  });
});

describe('the shared-instance rule', () => {
  it('is false for a unique anchor and true when a duplicate exists', () => {
    expect(anchorOf(document, el('a')).shared).toBe(false);
    expect(anchorOf(document, el('dup1')).shared).toBe(true);
    expect(anchorOf(document, el('dup2')).shared).toBe(true);
    // A span collision across FILES is not a shared instance.
    expect(anchorOf(document, el('tpl')).shared).toBe(false);
  });
});

describe('reading a file\'s anchors', () => {
  it('collects every anchor of one file, and the elements at one span', () => {
    const files = new Set(anchorsIn(document, 'unit.wcl').map((a) => a.file));
    expect([...files]).toEqual(['unit.wcl']);
    expect(anchorsIn(document, 'unit.wcl').map((a) => a.el.id)).toContain('dup2');
    // The malformed stamp is not an anchor.
    expect(anchorsIn(document, 'unit.wcl').map((a) => a.el.id)).not.toContain('badspan');
    expect(anchorEls(document, 'unit.wcl', { start: 300, end: 310 }).map((e) => e.id)).toEqual([
      'dup1',
      'dup2',
    ]);
    expect(elBySpan(document, 'unit.wcl', { start: 10, end: 20 })).toBe(el('a'));
    expect(elBySpan(document, 'unit.wcl', { start: 1, end: 2 })).toBeNull();
  });
});

describe('resolving anchors at a point', () => {
  it('finds the innermost anchor, skipping template chrome', () => {
    expect(anchorElAt(el('btnrect'))).toBe(el('btn'));
    expect(anchorElAt(el('tpl'))).toBeNull();
    expect(anchorElAt(el('sep'))).toBe(el('list'));
    expect(anchorElAt(null)).toBeNull();
  });

  it('skips a malformed stamp rather than resolving to an element anchorOf rejects', () => {
    // Otherwise a click lands on an element the anchor reader refuses, and
    // silently does nothing.
    expect(anchorElAt(el('badspan'))).toBeNull();
    expect(anchorChainAt(el('badspan'))).toEqual([]);
    // An outer anchor still wins when there is one.
    el('list').appendChild(el('badspan'));
    expect(anchorElAt(el('badspan'))).toBe(el('list'));
  });

  it('collects the nesting chain innermost → outermost', () => {
    expect(anchorChainAt(el('btnrect')).map((e) => e.id)).toEqual(['btn', 'frame', 'dia']);
    expect(anchorChainAt(el('li1')).map((e) => e.id)).toEqual(['li1', 'list']);
    expect(anchorChainAt(el('a')).map((e) => e.id)).toEqual(['a']);
  });

  it('steps one anchor outward, ending at the top', () => {
    expect(outerAnchorEl(el('btn'))).toBe(el('frame'));
    expect(outerAnchorEl(el('frame'))).toBe(el('dia'));
    expect(outerAnchorEl(el('dia'))).toBeNull();
  });
});

describe('the page wrapper and the block tree', () => {
  it('reads the page stamps', () => {
    expect(pageInfo(document)).toMatchObject({
      el: document.querySelector('[data-wcl-page-file]'),
      name: 'concept_x',
      file: 'unit.wcl',
      span: { start: 0, end: 900 },
    });
    document.body.innerHTML = '<p>not a wdoc page</p>';
    expect(pageInfo(document)).toBeNull();
  });

  it('walks direct block children only', () => {
    const page = pageInfo(document).el;
    expect(blockChildren(page).map((e) => e.id)).toEqual([
      'a',
      'b',
      'tpl',
      'list',
      'dup1',
      'dup2',
      'dia',
      'badspan',
    ]);
    expect(blockChildren(el('list')).map((e) => e.id)).toEqual(['li1', 'li2']);
  });

  it('walks direct shape children only', () => {
    expect(shapeChildren(el('dia')).map((e) => e.id)).toEqual(['frame']);
    expect(shapeChildren(el('frame')).map((e) => e.id)).toEqual(['btn']);
  });

  it('names the block an element sits in, the page root at top level', () => {
    const page = pageInfo(document).el;
    expect(containerOf(page, el('li1'))).toBe(el('list'));
    expect(containerOf(page, el('sep'))).toBe(el('list'));
    expect(containerOf(page, el('a'))).toBe(page);
    // The container's block children are the sibling run `el` belongs to.
    expect(blockChildren(containerOf(page, el('li1')))).toContain(el('li1'));
  });
});

describe('the manual-layout predicate', () => {
  it('is true for the layouts that honor per-shape x/y, false without a diagram', () => {
    expect(isManualLayout('free')).toBe(true);
    expect(isManualLayout('none')).toBe(true);
    expect(isManualLayout('layered')).toBe(false);
    // A layout of null means "no owning diagram" — nothing to place into.
    expect(isManualLayout(null)).toBe(false);
    expect(isManualLayout(undefined)).toBe(false);
    // Read off an anchor, which is how every caller asks.
    expect(isManualLayout(anchorOf(document, el('btn')).layout)).toBe(true);
    expect(isManualLayout(anchorOf(document, el('a')).layout)).toBe(false);
  });
});

describe('the sibling walk', () => {
  it('is scoped to the container and the block\'s own file', () => {
    // Down from b: the template h1 is skipped → the list.
    expect(adjacentSameFileSibling(document, el('b'), 'down')?.id).toBe('list');
    expect(adjacentSameFileSibling(document, el('b'), 'up')?.id).toBe('a');
    // A nested container scopes its own siblings.
    expect(adjacentSameFileSibling(document, el('li1'), 'down')?.id).toBe('li2');
    expect(adjacentSameFileSibling(document, el('li1'), 'up')).toBeNull();
    // Top edge.
    expect(adjacentSameFileSibling(document, el('a'), 'up')).toBeNull();
  });

  it('lists the ordered same-file siblings the reorder drop line uses', () => {
    expect(sameFileSiblings(document, el('a')).map((e) => e.id)).toEqual([
      'a',
      'b',
      'list',
      'dup1',
      'dup2',
      'dia',
    ]);
    expect(sameFileSiblings(document, el('li2')).map((e) => e.id)).toEqual(['li1', 'li2']);
    // The two walks agree about what a neighbour is.
    const sibs = sameFileSiblings(document, el('b'));
    expect(sibs[sibs.indexOf(el('b')) + 1]).toBe(adjacentSameFileSibling(document, el('b'), 'down'));
  });

  it('finds the previous sibling of a kind, and the enclosing one', () => {
    expect(prevSiblingOfKind(el('li2'), 'li')).toBe(el('li1'));
    expect(prevSiblingOfKind(el('li1'), 'li')).toBeNull();
    expect(closestOfKind(el('li1').parentElement, 'li')).toBeNull();
    expect(closestOfKind(el('li1'), 'li')).toBe(el('li1'));
  });
});

describe('bindings stamped by the build', () => {
  it('reads an edit_field binding from anywhere inside it', () => {
    expect(fieldBindingOf(el('bound'))).toEqual({
      el: el('bound'),
      kind: 'screen',
      target: 'login',
      field: 'title',
      plain: true,
    });
    expect(fieldBindingOf(el('a'))).toBeNull();
  });

  it('reads an edit_object button', () => {
    expect(editButtonOf(el('editbtn'))).toEqual({
      el: el('editbtn'),
      kind: 'screen',
      target: 'login',
    });
    expect(editButtonOf(el('a'))).toBeNull();
  });

  it('reads a layout-guide drop slot', () => {
    expect(slotOf(el('cell2'))).toBe(2);
    expect(slotOf(el('btn'))).toBeNull();
  });

  it('reads the connection an edge path draws, from anywhere inside it', () => {
    expect(edgeOf(el('edge'))).toEqual({ from: 'frame', to: 'btn' });
    expect(edgeOf(el('edgetitle'))).toEqual({ from: 'frame', to: 'btn' });
    // A malformed stamp reads as no stamp, like a malformed span.
    expect(edgeOf(el('badedge'))).toBeNull();
    expect(edgeOf(el('btn'))).toBeNull();
    expect(edgeOf(null)).toBeNull();
  });
});

describe('re-stamping after an in-place commit', () => {
  it('rewrites a block span and the page span', () => {
    restampSpan(el('a'), { start: 12, end: 22 });
    expect(anchorOf(document, el('a')).span).toEqual({ start: 12, end: 22 });
    // The already-formatted attribute form is accepted too.
    restampSpan(el('a'), '14:24');
    expect(spanOf(el('a'))).toEqual({ start: 14, end: 24 });
    restampPageSpan(pageInfo(document).el, { start: 0, end: 910 });
    expect(pageInfo(document).span).toEqual({ start: 0, end: 910 });
    // The re-stamped element is found at its NEW span, not its old one.
    expect(elBySpan(document, 'unit.wcl', { start: 14, end: 24 })).toBe(el('a'));
    expect(elBySpan(document, 'unit.wcl', { start: 10, end: 20 })).toBeNull();
  });

  it('round-trips the visibility stamp through visOf', () => {
    restampExcept(el('a'), ['deck', 'training']);
    expect(visOf(el('a')).except).toEqual(['deck', 'training']);
    expect(anchorOf(document, el('a')).except).toEqual(['deck', 'training']);
    restampExcept(el('a'), []);
    expect(visOf(el('a')).except).toEqual([]);
    expect(anchorOf(document, el('a')).except).toEqual([]);
  });
});

describe('the shape geometry store', () => {
  const box = (x, y, width, height) => ({ x, y, width, height });

  it('prefers a stashed measurement to a live one', () => {
    const shape = el('btn');
    shape.getBBox = () => box(0, 0, 100, 40);
    expect(stashShapeBox(shape)).toEqual(box(0, 0, 100, 40));
    shape.getBBox = () => box(0, 0, 200, 200); // "live", chrome included
    expect(shapeBox(shape)).toEqual(box(0, 0, 100, 40));
    // And that is the geometry the anchor carries.
    expect(anchorOf(document, shape).box).toEqual(box(0, 0, 100, 40));
  });

  it('re-measures a clean shape, and stands on the stash when chrome is inside', () => {
    const shape = el('btn');
    shape.getBBox = () => box(0, 0, 100, 40);
    stashShapeBox(shape);
    // The shape grew from an in-place commit — its element survived, so
    // nothing evicted the entry. Clean, therefore re-measured.
    shape.getBBox = () => box(0, 0, 160, 40);
    expect(stashShapeBox(shape)).toEqual(box(0, 0, 160, 40));
    expect(shapeBox(shape)).toEqual(box(0, 0, 160, 40));
    // With chrome inside, a live measurement would swallow it: the stash
    // stands, which is why re-selecting a shape can't grow its outline.
    shape.getBBox = () => box(-3, -3, 206, 86);
    expect(stashShapeBox(shape, { chromeInside: true })).toEqual(box(0, 0, 160, 40));
    expect(shapeBox(shape)).toEqual(box(0, 0, 160, 40));
  });

  it('falls back to a live measurement when nothing was stashed', () => {
    const shape = el('btn');
    shape.getBBox = () => box(1, 2, 30, 40);
    expect(shapeBox(shape)).toEqual(box(1, 2, 30, 40));
  });

  it('reports nothing when the shape cannot be measured', () => {
    expect(shapeBox(null)).toBeNull();
    el('btn').getBBox = () => {
      throw new Error('detached');
    };
    expect(shapeBox(el('btn'))).toBeNull();
    delete el('btn').getBBox;
    expect(shapeBox(el('btn'))?.width).toBe(0); // happy-dom's stub geometry
    // Non-shape anchors never carry geometry.
    expect(anchorOf(document, el('a')).box).toBeNull();
  });

  it('keys entries by element, so a rebuilt shape measures afresh', () => {
    el('btn').getBBox = () => box(0, 0, 100, 40);
    stashShapeBox(el('btn'));
    expect(shapeBox(el('btn'))).toEqual(box(0, 0, 100, 40));
    setup(); // a rebuild replaces the elements
    const rebuilt = el('btn');
    rebuilt.getBBox = () => box(0, 0, 300, 90);
    expect(shapeBox(rebuilt)).toEqual(box(0, 0, 300, 90));
  });
});
