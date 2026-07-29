/* Unit tests for the schema-driven form helpers shared by the diagram
   ShapePanel, the Systems dock and the details modal: field ordering, the
   typed field-value → block-op mapping, list round-tripping, and the block
   snippet a new child block is seeded from. */

import { describe, expect, it } from 'vitest';

import {
  CUSTOM_OPTION,
  blockSnippet,
  createValue,
  fieldState,
  fieldText,
  formEditable,
  freshShapeId,
  listExpr,
  listText,
  orderFields,
  shapeSnippet,
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
  const cells = { tags: { state: 'list', items: [{ text: 'one' }, { text: 'two' }] } };

  it('reads a list cell as one editable line', () => {
    expect(listText(tags, cells)).toBe('one, two');
    expect(listText(repos, cells)).toBe('');
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
  const cells = {
    '@0': { state: 'literal', text: 'lexer', slot: 0 },
    kind: { state: 'literal', text: ':module' },
  };
  it('reads inline slots by position and fields by name', () => {
    expect(fieldText(field('id', 'identifier', { inline_slot: 0 }), cells)).toBe('lexer');
    // A symbol cell arrives as `:name`; the form works in bare names.
    expect(fieldText(field('kind', 'K', { symbols: ['module'] }), cells)).toBe('module');
    expect(fieldState(field('missing', 'utf8?'), cells)).toBe('absent');
  });

  it('sends computed values to the source editor', () => {
    expect(formEditable('computed')).toBe(false);
    expect(formEditable('absent')).toBe(true);
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
