/* Unit tests for the schema-driven form helpers every panel shares: field
   ordering, reading a cell, which control a (field, cell) pair wants, the
   ops a draft produces, the typed field-value → block-op mapping, list
   round-tripping, and the block snippet a new child block is seeded from.

   The control itself is deliberately untested: FieldControl is a flat
   switch over `controlFor` with no branching of its own, so a rendering
   harness would only re-test Forge. */

import { describe, expect, it } from 'vitest';

import {
  CUSTOM_OPTION,
  blockSnippet,
  cellText,
  controlFor,
  createFields,
  createValue,
  draftOps,
  fieldState,
  fieldText,
  formEditable,
  freshShapeId,
  listExpr,
  orderFields,
  shapeSnippet,
  slugify,
  suggestOptions,
  valueOp,
} from './schemaform';

const field = (name, type, extra = {}) => ({
  name,
  type,
  optional: type.endsWith('?'),
  inline_slot: null,
  symbols: null,
  default: null,
  ...extra,
});

/** Cells as the server serves them: positional labels, named fields. */
const cells = (fields = {}, labels = []) => ({ labels, fields });
const text = (t) => ({ state: 'text', text: t });

const SPAN = { start: 10, end: 40 };

describe('orderFields', () => {
  it('puts inline slots first, then geometry, then the rest by name', () => {
    const fields = [
      field('summary', 'utf8'),
      field('y', 'f64'),
      field('x', 'f64'),
      field('id', 'identifier', { inline_slot: 0 }),
      field('name', 'utf8'),
    ];
    expect(orderFields(fields).map((f) => f.name)).toEqual(['id', 'x', 'y', 'name', 'summary']);
  });
});

describe('valueOp', () => {
  it('writes strings as text and everything else as a typed expression', () => {
    expect(valueOp(SPAN, field('name', 'utf8'), 'Hi')).toEqual({
      op: 'set_field',
      span: SPAN,
      field: 'name',
      text: 'Hi',
    });
    expect(valueOp(SPAN, field('kind', 'K', { symbols: ['a'] }), 'a').expr).toBe(':a');
    expect(valueOp(SPAN, field('n', 'u32'), '3.7').expr).toBe('4');
    expect(valueOp(SPAN, field('w', 'f64'), '12').expr).toBe('12.0');
    expect(valueOp(SPAN, field('on', 'bool'), 'true').expr).toBe('true');
    // An identifier is a bare reference, never a quoted string.
    expect(valueOp(SPAN, field('container', 'identifier'), 'wcl_lang').expr).toBe('wcl_lang');
  });

  it('targets the label slot for inline fields', () => {
    const f = field('id', 'identifier', { inline_slot: 0 });
    expect(valueOp(SPAN, f, 'lexer')).toEqual({
      op: 'set_label',
      span: SPAN,
      slot: 0,
      expr: 'lexer',
    });
  });
});

describe('lists', () => {
  const tags = field('tags', 'list<utf8>');
  const repos = field('repos', 'list<identifier>');
  const listCells = cells({ tags: { state: 'list', items: [text('one'), text('two')] } });

  it('reads a list cell as one editable line', () => {
    expect(fieldText(tags, listCells)).toBe('one, two');
    expect(fieldText(repos, listCells)).toBe('');
  });

  it('quotes string elements and leaves identifiers bare', () => {
    expect(listExpr(tags, 'one, two')).toBe('["one", "two"]');
    expect(listExpr(repos, 'a, b')).toBe('[a, b]');
    expect(listExpr(tags, '  ')).toBe('[]');
  });

  it('escapes quotes in string elements', () => {
    expect(listExpr(tags, 'say "hi"')).toBe('["say \\"hi\\""]');
  });
});

describe('suggestOptions', () => {
  const schema = { suggestions: { kind: ['handler', 'module'] } };

  it('offers the values in use, plus a way to type a new one', () => {
    const opts = suggestOptions(field('kind', 'utf8?'), schema, 'module');
    expect(opts.map((o) => o.label)).toEqual(['(unset)', 'handler', 'module', '＋ Custom…']);
  });

  it('keeps a value that is not in the list — nothing is silently dropped', () => {
    const opts = suggestOptions(field('kind', 'utf8?'), schema, 'store');
    expect(opts.map((o) => o.value)).toEqual(['', 'store', 'handler', 'module', CUSTOM_OPTION]);
  });

  it('omits "(unset)" for a required field', () => {
    const opts = suggestOptions(field('kind', 'utf8'), schema, 'module');
    expect(opts[0].value).toBe('handler');
  });

  it('is null without suggestions, and for non-text fields', () => {
    expect(suggestOptions(field('name', 'utf8'), schema, 'x')).toBeNull();
    expect(suggestOptions(field('kind', 'identifier'), schema, 'x')).toBeNull();
    expect(suggestOptions(field('kind', 'utf8?'), {}, 'x')).toBeNull();
  });
});

describe('createValue', () => {
  it('keeps the WCL shape of each field type', () => {
    expect(createValue(field('container', 'identifier'), 'wcl_lang')).toEqual({ ident: 'wcl_lang' });
    expect(createValue(field('kind', 'K', { symbols: ['a'] }), 'a')).toEqual({ sym: 'a' });
    expect(createValue(field('on', 'bool'), 'true')).toBe(true);
    expect(createValue(field('n', 'u32'), '7')).toBe(7);
    expect(createValue(field('name', 'utf8'), 'Hi')).toBe('Hi');
    // A list takes the same comma line the edit forms take.
    expect(createValue(field('tags', 'list<utf8>'), 'a, b')).toEqual(['a', 'b']);
    expect(createValue(field('repos', 'list<identifier>'), 'one')).toEqual([{ ident: 'one' }]);
  });
});

describe('blockSnippet', () => {
  it('fills the inline id and every required field', () => {
    const entry = {
      kind: 'cli_flag',
      fields: [
        field('id', 'identifier', { inline_slot: 0 }),
        field('name', 'utf8'),
        field('value', 'utf8?'),
        field('repeatable', 'bool', { default: 'false' }),
      ],
    };
    expect(blockSnippet(entry, { id: 'f_out' })).toBe(
      'cli_flag f_out {\n  name = ""\n}',
    );
  });

  it('takes given values over the defaults', () => {
    const entry = {
      kind: 'step',
      fields: [
        field('id', 'identifier', { inline_slot: 0 }),
        field('n', 'u32'),
        field('shape', 'StepShape', { symbols: ['process', 'decision'] , optional: false }),
      ],
    };
    expect(blockSnippet(entry, { id: 's1', values: { n: '3', shape: 'decision' } })).toBe(
      'step s1 {\n  n = 3\n  shape = :decision\n}',
    );
  });

  it('emits a bare block when nothing is required', () => {
    expect(blockSnippet({ kind: 'note', fields: [] }, { id: 'n1' })).toBe('note {}');
  });
});

describe('fieldState / fieldText', () => {
  const block = cells(
    { kind: { state: 'symbol', text: 'module' }, hp: { state: 'number', text: '10' } },
    [{ state: 'identifier', text: 'lexer' }],
  );

  it('reads inline slots by position and fields by name', () => {
    expect(fieldText(field('id', 'identifier', { inline_slot: 0 }), block)).toBe('lexer');
    // A symbol's text is the bare member name — the colon is syntax.
    expect(fieldText(field('kind', 'K', { symbols: ['module'] }), block)).toBe('module');
    expect(fieldText(field('hp', 'u32'), block)).toBe('10');
    expect(fieldState(field('missing', 'utf8?'), block)).toBe('absent');
    expect(cellText(block, 'kind')).toBe('module');
  });

  it('sends computed values, but only those, to the source editor', () => {
    expect(formEditable('computed')).toBe(false);
    expect(formEditable('rows')).toBe(false);
    for (const state of ['text', 'identifier', 'symbol', 'bool', 'number', 'list', 'absent']) {
      expect(formEditable(state)).toBe(true);
    }
  });
});

describe('controlFor', () => {
  const cell = (state, t) => ({ state, text: t });

  it('picks a picker for symbols, for id references and for suggestions', () => {
    const kind = field('kind', 'K', { symbols: ['module', 'cli'] });
    expect(controlFor(kind, cell('symbol', 'module'))).toBe('symbol');
    const owner = field('container', 'identifier?');
    expect(controlFor(owner, cell('identifier', 'wcl_lang'), { ids: [{ value: 'a' }] })).toBe(
      'idref',
    );
    // Nothing to point at ⇒ type the reference.
    expect(controlFor(owner, cell('identifier', 'wcl_lang'))).toBe('text');
    const free = field('kind', 'utf8?');
    expect(controlFor(free, cell('text', 'store'), { suggestions: ['store'] })).toBe('suggest');
    // "Custom…" escapes the picker; a real new value is always possible.
    expect(controlFor(free, cell('text', 'store'), { suggestions: ['store'], custom: true })).toBe(
      'text',
    );
  });

  it('gives a bool a checkbox and a scalar an input, wherever it is opened', () => {
    expect(controlFor(field('on', 'bool'), cell('bool', 'true'))).toBe('bool');
    expect(controlFor(field('on', 'bool?'), undefined)).toBe('bool');
    // Numbers and identifiers are editable — they used to fall through to
    // a silently read-only control.
    expect(controlFor(field('hp', 'u32'), cell('number', '10'))).toBe('text');
    expect(controlFor(field('size', 'Bytes'), cell('number', '5MiB'))).toBe('text');
    expect(controlFor(field('repo', 'identifier?'), cell('identifier', 'r'))).toBe('text');
  });

  it('marks what no control can express', () => {
    expect(controlFor(field('title', 'utf8'), cell('computed'))).toBe('computed');
    expect(controlFor(field('rows', 'list<list<utf8>>'), { state: 'rows', rows: [] })).toBe(
      'computed',
    );
    // A list is one comma-separated line, in every panel.
    expect(controlFor(field('tags', 'list<utf8>'), { state: 'list', items: [] })).toBe('list');
    expect(controlFor(field('tags', 'list<utf8>'), undefined)).toBe('list');
  });
});

describe('draftOps', () => {
  const fields = [
    field('id', 'identifier', { inline_slot: 0 }),
    field('name', 'utf8', { optional: false }),
    field('summary', 'utf8?'),
    field('hp', 'u32?'),
    field('tags', 'list<utf8>?'),
  ];
  const block = cells(
    { name: text('Hero'), summary: text('a hero'), hp: { state: 'number', text: '10' } },
    [{ state: 'identifier', text: 'hero' }],
  );

  it('writes nothing for a draft that changes nothing', () => {
    expect(draftOps(fields, block, {}, SPAN)).toEqual([]);
    expect(draftOps(fields, block, { name: 'Hero', hp: '10', id: 'hero' }, SPAN)).toEqual([]);
  });

  it('writes the typed value of a changed field', () => {
    expect(draftOps(fields, block, { name: 'Villain', hp: '12' }, SPAN)).toEqual([
      { op: 'set_field', span: SPAN, field: 'hp', expr: '12' },
      { op: 'set_field', span: SPAN, field: 'name', text: 'Villain' },
    ]);
  });

  it('removes a cleared optional field and ignores a cleared required one', () => {
    expect(draftOps(fields, block, { summary: '', name: '' }, SPAN)).toEqual([
      { op: 'remove_field', span: SPAN, field: 'summary' },
    ]);
    // An inline label can't be absent, so clearing it writes nothing.
    expect(draftOps(fields, block, { id: '' }, SPAN)).toEqual([]);
    // Clearing something that was never set is not a removal.
    expect(draftOps(fields, block, { tags: '' }, SPAN)).toEqual([]);
  });

  it('targets a label slot by position and a field by name', () => {
    expect(draftOps(fields, block, { id: 'villain', tags: 'a, b' }, SPAN)).toEqual([
      { op: 'set_label', span: SPAN, slot: 0, expr: 'villain' },
      { op: 'set_field', span: SPAN, field: 'tags', expr: '["a", "b"]' },
    ]);
  });
});

describe('createFields', () => {
  it('types every answered field and leaves the empty ones out', () => {
    const fields = [
      field('id', 'identifier', { inline_slot: 0 }),
      field('name', 'utf8'),
      field('kind', 'K', { symbols: ['cli'] }),
      field('n', 'u32'),
      field('summary', 'utf8?'),
    ];
    expect(createFields(fields, { id: 'x', name: 'Hi', kind: 'cli', n: '3', summary: '' })).toEqual({
      name: 'Hi',
      kind: { sym: 'cli' },
      n: 3,
    });
  });
});

describe('slugify', () => {
  it('derives an identifier from a display name', () => {
    expect(slugify('WCL parse!')).toBe('wcl_parse');
    expect(slugify('2 fast')).toBe('_2_fast');
    expect(slugify(null)).toBe('');
  });
});

describe('shapeSnippet / freshShapeId', () => {
  const button = {
    kind: 'wf_button',
    fields: [
      field('label', 'utf8', { inline_slot: 0, optional: false }),
      field('x', 'f64?'),
      field('y', 'f64?'),
    ],
  };

  it('fills inline slots and staggers x/y only under a manual layout', () => {
    const manual = shapeSnippet(button, { uid: 'wf_button_1', manual: true, index: 0 });
    expect(manual).toContain('wf_button "Label"');
    expect(manual).toContain('x = 20.0');
    // Inside a container children stack — no coordinates.
    const stacked = shapeSnippet(button, { uid: 'wf_button_1', manual: false, index: 0 });
    expect(stacked).not.toContain('x =');
  });

  it('places at an explicit drop point instead of the stagger', () => {
    const s = shapeSnippet(button, { uid: 'wf_button_1', manual: true, index: 3, at: { x: 141.27, y: 88 } });
    expect(s).toContain('x = 141.3');
    expect(s).toContain('y = 88.0');
    // `at` places even when the layout gate alone wouldn't emit coordinates.
    const forced = shapeSnippet(button, { uid: 'wf_button_1', manual: false, at: { x: 5, y: 6 } });
    expect(forced).toContain('x = 5.0');
  });

  it('derives a fresh id past field ids and inline block labels', () => {
    const source = 'wf_button b { id = wf_button_1 }\nwf_input wf_button_2 {}';
    expect(freshShapeId('wf_button', source)).toBe('wf_button_3');
    expect(freshShapeId('wf_input', source)).toBe('wf_input_1');
  });
});
