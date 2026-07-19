/* Unit tests for the Design-mode iframe layer: the source-swap text
   session (commit / cancel / Enter-split semantics), the marker-wrapping
   string surgery, and the WCL string escaper. Runs under happy-dom. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { beginTextSession, blockAt, wclString, wrapSelection } from './wysiwyg';
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
