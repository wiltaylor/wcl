/* Unit tests for the Design-mode iframe layer: the source-swap text
   session (commit / cancel / Enter-split semantics), the marker-wrapping
   string surgery, and the WCL string escaper. Runs under happy-dom. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  anchorChainAt,
  beginTextSession,
  blockAt,
  installDesign,
  wclString,
  wrapSelection,
} from './wysiwyg';
import { cellDisplay, cellRaw, splitPipeRow } from './table';

function setup(html) {
  document.body.innerHTML = html;
  return document;
}

const flush = () => new Promise((r) => setTimeout(r, 5));

describe('beginTextSession', () => {
  let el;
  beforeEach(() => {
    setup('<p id="t"><span class="bold">Bold</span> and plain</p>');
    el = document.getElementById('t');
  });

  it('swaps to raw markup and commits on Ctrl+Enter', () => {
    const onCommit = vi.fn();
    beginTextSession(document, el, '**Bold** and plain', { onCommit });
    // Rendered children replaced by the raw markup text.
    expect(el.textContent).toBe('**Bold** and plain');
    expect(el.querySelector('.bold')).toBeNull();
    expect(el.classList.contains('wcl-wys-editing')).toBe(true);

    el.textContent = '**Bolder** and plain';
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true }));
    expect(onCommit).toHaveBeenCalledWith('**Bolder** and plain');
    expect(el.classList.contains('wcl-wys-editing')).toBe(false);
  });

  it('restores the rendered DOM on Escape', () => {
    const onCancel = vi.fn();
    beginTextSession(document, el, '**Bold** and plain', { onCancel });
    el.textContent = 'changed';
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(onCancel).toHaveBeenCalled();
    // Original rendered children are back.
    expect(el.querySelector('.bold')?.textContent).toBe('Bold');
    expect(el.getAttribute('contenteditable')).toBeNull();
  });

  it('commits on blur only when dirty', async () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    beginTextSession(document, el, 'text', { onCommit, onCancel });
    el.dispatchEvent(new Event('blur'));
    await flush();
    expect(onCommit).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalled();

    setup('<p id="t2">x</p>');
    const el2 = document.getElementById('t2');
    beginTextSession(document, el2, 'text', { onCommit, onCancel });
    el2.textContent = 'text edited';
    el2.dispatchEvent(new Event('blur'));
    await flush();
    expect(onCommit).toHaveBeenCalledWith('text edited');
  });

  it('routes Enter to the split handler when provided', () => {
    const onEnter = vi.fn();
    const onCommit = vi.fn();
    beginTextSession(document, el, 'before after', { onEnter, onCommit });
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(onEnter).toHaveBeenCalled();
    // Whatever the caret position, the two halves recompose the text.
    const [before, after] = onEnter.mock.calls[0];
    expect(before + after).toBe('before after');
    expect(onCommit).not.toHaveBeenCalled();
  });
});

describe('wrapSelection', () => {
  it('wraps the whole text when nothing is selected', () => {
    setup('<p id="t">plain</p>');
    const el = document.getElementById('t');
    window.getSelection()?.removeAllRanges();
    wrapSelection(document, el, '**', '**');
    expect(el.textContent).toBe('**plain**');
  });
});

describe('blockAt', () => {
  it('skips template chrome and finds content blocks', () => {
    setup(
      '<div id="chromeblock" data-wcl-span="0:9">' +
        '<a id="chrome">nav</a></div>' +
        '<p data-wcl-span="10:30" data-wcl-file="data/x.wcl"><em id="content">t</em></p>',
    );
    // Set via the DOM: innerHTML entity handling for `<` in attribute
    // values differs between happy-dom and browsers.
    document
      .getElementById('chromeblock')
      .setAttribute('data-wcl-file', '<wcl-system>/wdoc/templates.wcl');
    expect(blockAt(document.getElementById('chrome'))).toBeNull();
    expect(blockAt(document.getElementById('content'))?.getAttribute('data-wcl-file')).toBe(
      'data/x.wcl',
    );
  });
});

/* A nested-anchor page: a diagram <svg> anchor containing shape anchors
   (as the edit-mode build stamps them). happy-dom renders SVG as generic
   elements, which is fine — the chain/selection logic is pure DOM. */
const NESTED = `
  <div data-wcl-span="0:5" data-wcl-file="main.wcl" data-wcl-kind="p" id="para">text</div>
  <svg data-wcl-span="10:99" data-wcl-file="main.wcl" data-wcl-kind="diagram" data-wcl-layout="free" id="dia">
    <g data-wcl-shape data-wcl-span="20:40" data-wcl-file="main.wcl" data-wcl-kind="rect" id="shape">
      <rect id="inner" />
    </g>
  </svg>`;

describe('anchorChainAt', () => {
  it('collects anchors innermost to outermost', () => {
    setup(NESTED);
    const chain = anchorChainAt(document.getElementById('inner'));
    expect(chain.map((el) => el.id)).toEqual(['shape', 'dia']);
    expect(anchorChainAt(document.getElementById('para')).map((el) => el.id)).toEqual(['para']);
  });
});

describe('installDesign drill-in selection', () => {
  const wire = () => {
    const onSelect = vi.fn();
    const onEditIntent = vi.fn();
    const teardown = installDesign(document, {
      enabled: () => true,
      onSelect: (a) => {
        onSelect(a);
        // Mirror EditSurface: selection state lives on the element.
        for (const s of document.querySelectorAll('.wcl-wys-selected')) {
          s.classList.remove('wcl-wys-selected');
        }
        a?.el.classList.add('wcl-wys-selected');
      },
      onEditIntent,
    });
    return { onSelect, onEditIntent, teardown };
  };
  const click = (el) => el.dispatchEvent(new MouseEvent('click', { bubbles: true }));

  it('selects the shape directly on first click; diagram via background', () => {
    setup(NESTED);
    const { onSelect, onEditIntent, teardown } = wire();
    const inner = document.getElementById('inner');
    const dia = document.getElementById('dia');

    // Nearest-first: a shape click shows its selection (and panel) at once.
    click(inner);
    expect(onSelect).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: 'rect', shape: true, layout: 'free' }),
    );
    expect(onEditIntent).not.toHaveBeenCalled();

    click(inner);
    expect(onEditIntent).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'rect' }),
      inner,
      expect.anything(),
    );

    // Clicking the diagram background selects the diagram itself.
    click(dia);
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ kind: 'diagram' }));
    teardown();
  });

  it('nested HTML anchors select the innermost directly (nearest behavior)', () => {
    setup(
      '<div data-wcl-span="0:50" data-wcl-file="main.wcl" data-wcl-kind="demo" id="demo">' +
        '<table data-wcl-span="5:45" data-wcl-file="main.wcl" data-wcl-kind="table" id="tbl">' +
        '<tbody><tr><td id="cell">x</td></tr></tbody></table></div>',
    );
    const { onSelect, onEditIntent, teardown } = wire();
    const cell = document.getElementById('cell');

    // One click: the table, not the wrapping demo block.
    click(cell);
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ kind: 'table' }));
    // Second click: straight to the editor.
    click(cell);
    expect(onEditIntent).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'table' }),
      cell,
      expect.anything(),
    );
    // Esc still pops out to the container.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ kind: 'demo' }));
    teardown();
  });

  it('single-anchor blocks keep select-then-edit behavior', () => {
    setup(NESTED);
    const { onSelect, onEditIntent, teardown } = wire();
    const para = document.getElementById('para');
    click(para);
    expect(onSelect).toHaveBeenLastCalledWith(
      expect.objectContaining({ kind: 'p', shape: false }),
    );
    click(para);
    expect(onEditIntent).toHaveBeenCalled();
    teardown();
  });

  it('Escape pops the selection one level, then deselects', () => {
    setup(NESTED);
    const { onSelect, teardown } = wire();
    const inner = document.getElementById('inner');
    click(inner); // shape selected directly
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ kind: 'diagram' }));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(onSelect).toHaveBeenLastCalledWith(null);
    teardown();
  });

  it('lets the merged view visibility chips own their clicks', () => {
    setup(
      '<div data-wcl-span="0:5" data-wcl-file="main.wcl" data-wcl-kind="p" id="para">text' +
        '<div class="wcl-vis-gutter"><button class="wcl-vis-chip" id="chip">B</button></div></div>',
    );
    const { onSelect, onEditIntent, teardown } = wire();
    click(document.getElementById('chip'));
    expect(onSelect).not.toHaveBeenCalled();
    expect(onEditIntent).not.toHaveBeenCalled();
    // A click on the block itself still selects.
    click(document.getElementById('para'));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ kind: 'p' }));
    teardown();
  });
});

describe('wclString', () => {
  it('escapes quotes, backslashes and newlines', () => {
    expect(wclString('a "b" \\ c\nd\te')).toBe('"a \\"b\\" \\\\ c\\nd\\te"');
  });
});

describe('pipe tables', () => {
  it('splits rows respecting quoted pipes', () => {
    expect(splitPipeRow('| "Name" | "A | B" | 42 |')).toEqual(['"Name"', '"A | B"', '42']);
  });
  it('round-trips display text', () => {
    expect(cellDisplay('"A \\"quoted\\" cell"')).toBe('A "quoted" cell');
    expect(cellDisplay('someExpr(1)')).toBe('someExpr(1)');
    expect(cellRaw('A "quoted" cell')).toBe('"A \\"quoted\\" cell"');
  });
});
