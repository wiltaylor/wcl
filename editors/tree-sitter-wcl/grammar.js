/**
 * Tree-sitter grammar for WCL.
 *
 * Tracks the Rust parser in `crates/wcl_lang/src/parser` closely enough
 * for highlighting, folding and outlines: every item form, every
 * expression form, patterns, type references, strings of every flavour
 * (plain, encoded, interpolated, heredoc) and numeric literals with type
 * or unit suffixes. Two constructs the Rust lexer decides by looking at
 * raw bytes live in `src/scanner.c`: heredoc bodies (the closer is a tag
 * chosen by the author) and the end of a body-less block, which the
 * Rust parser detects by a line break before the next token.
 *
 * Where the Rust parser treats a word as a keyword only in context
 * (`type`, `use`, `slot`, `try`, `parent`, …), the grammar does the same:
 * tree-sitter only lexes a keyword where the parse state accepts it, and
 * the item-start keywords are aliased back to identifiers so `type = 1`
 * still parses as a field.
 */

const PREC = {
  coalesce: 1,
  or: 2,
  and: 3,
  eq: 4,
  cmp: 5,
  add: 6,
  mul: 7,
  unary: 8,
  call: 9,
  member: 10,
};

const ENCODINGS = ['ascii', 'utf8', 'utf16', 'utf32'];

// Words that open an item form at the top of a file or block. Anywhere
// the Rust parser would read them as an ordinary name (`type = "x"`,
// `slot "a" {}`), they are aliased back to `identifier`.
const ITEM_KEYWORDS = [
  'type', 'interface', 'union', 'symbol_set', 'namespace', 'use',
  'import', 'let', 'fn', 'connection', 'slot',
];

module.exports = grammar({
  name: 'wcl',

  externals: $ => [
    $._block_end,
    $.heredoc,
    $._error_sentinel,
  ],

  extras: $ => [/\s/, $.comment],

  word: $ => $.identifier,

  supertypes: $ => [$._expression, $._pattern],

  conflicts: $ => [
    // `use a.b` — the `.` may extend the path or open a `.{…}` list.
    [$.dotted_name],
    // `slot name: T` is a slot declaration; `slot name {}` is a block.
    [$._name, $.slot_decl],
    [$._name, $.let_item],
    [$._name, $.fn_item],
    [$._name, $.connection_decl],
    [$._name, $.type_decl],
    [$._name, $.interface_decl],
    [$._name, $.union_decl],
    [$._name, $.symbol_set_decl],
    [$._name, $.namespace_decl],
    [$._name, $.use_decl],
    [$._name, $.import_decl],
  ],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.namespace_decl,
      $.use_decl,
      $.import_decl,
      $.type_decl,
      $.interface_decl,
      $.union_decl,
      $.symbol_set_decl,
      $.connection_decl,
      $.connection,
      $.let_item,
      $.fn_item,
      $.slot_decl,
      $.table,
      $.block,
      $.field,
    ),

    comment: _ => token(choice(
      seq('//', /[^\n]*/),
      seq('#', /[^\n]*/),
    )),

    _name: $ => choice(
      $.identifier,
      alias(choice(...ITEM_KEYWORDS), $.identifier),
    ),

    _decorators: $ => repeat1($.decorator),

    // ── Declarations ──────────────────────────────────────────────

    namespace_decl: $ => seq('namespace', field('name', $.dotted_name)),

    use_decl: $ => seq(
      'use',
      field('path', $.dotted_name),
      optional(choice(
        seq('as', field('alias', $.identifier)),
        seq('.', $.use_list),
      )),
    ),
    use_list: $ => seq('{', commaSep($.use_item), optional(','), '}'),
    use_item: $ => seq(
      field('name', $.identifier),
      optional(seq('as', field('alias', $.identifier))),
    ),

    import_decl: $ => seq(
      'import',
      field('path', choice($.string, $.system_import_path)),
    ),
    system_import_path: _ => token(seq('<', /[^>\n]+/, '>')),

    type_decl: $ => seq(
      optional($._decorators),
      'type',
      field('name', $.dotted_name),
      choice(
        seq(optional($.extends_clause), $.field_block),
        seq('=', field('alias', $._type)),
      ),
    ),

    interface_decl: $ => seq(
      optional($._decorators),
      'interface',
      field('name', $.dotted_name),
      optional($.extends_clause),
      $.field_block,
    ),

    extends_clause: $ => seq('extends', commaSep1($.dotted_name)),

    field_block: $ => seq('{', repeat($.type_field), '}'),

    type_field: $ => seq(
      optional($._decorators),
      field('name', $.identifier),
      choice(
        seq(':', field('type', $._type), optional($.optional_marker)),
        seq('=', field('default', $._expression)),
      ),
    ),
    optional_marker: _ => '?',

    union_decl: $ => seq(
      optional($._decorators),
      'union',
      field('name', $.dotted_name),
      optional($.extends_clause),
      '{',
      repeat($.union_variant),
      '}',
    ),
    union_variant: $ => seq(
      optional($._decorators),
      field('name', $.identifier),
      field('body', choice(
        $.field_block,
        $.none,
        $._type,
      )),
    ),

    symbol_set_decl: $ => seq(
      optional($._decorators),
      'symbol_set',
      field('name', $.dotted_name),
      '{',
      repeat($.symbol_entry),
      '}',
    ),
    symbol_entry: $ => seq(optional($._decorators), field('name', $.identifier)),

    connection_decl: $ => seq(
      optional($._decorators),
      'connection',
      field('name', $.dotted_name),
      ':',
      field('source', $._type),
      '->',
      field('destination', $._type),
      ':',
      field('kinds', $.dotted_name),
    ),

    // `a -> b`, `a -> b :kind`, `a -> b : kind`.
    connection: $ => seq(
      field('source', $._name),
      '->',
      field('destination', $.identifier),
      optional(field('kind', choice(
        $.symbol,
        seq(':', alias($.identifier, $.symbol)),
      ))),
    ),

    let_item: $ => seq(
      'let',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    fn_item: $ => prec.right(seq(
      optional($._decorators),
      'fn',
      field('name', $.identifier),
      field('parameters', $.parameters),
      '->',
      field('return_type', $._type),
      field('body', $._expression),
    )),

    // `slot name: T`, `slot name: T?`, `slot name: T* = default`.
    slot_decl: $ => seq(
      optional($._decorators),
      'slot',
      field('name', $.identifier),
      ':',
      field('type', $._type),
      optional(choice($.optional_marker, $.repeat_marker)),
      optional(seq('=', field('default', $._expression))),
    ),
    repeat_marker: _ => '*',

    // ── Decorators ────────────────────────────────────────────────

    decorator: $ => seq(
      '@',
      field('name', $.dotted_name),
      optional(field('arguments', $.decorator_arguments)),
    ),
    decorator_arguments: $ => seq(
      '(',
      commaSep(choice($.named_argument, $._expression)),
      optional(','),
      ')',
    ),
    named_argument: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    // ── Tables ────────────────────────────────────────────────────

    table: $ => prec.right(seq(
      field('name', $._name),
      ':',
      repeat($.table_row),
    )),
    // `| a | b |` — the trailing pipe is optional. A cell never starts
    // with an identifier, which is what lets a row end at a line break.
    table_row: $ => prec.right(seq(
      '|',
      repeat(seq($._table_cell, '|')),
      optional($._table_cell),
    )),
    _table_cell: $ => choice(
      $.number,
      $.string,
      $.interpolated_string,
      $.heredoc,
      $.boolean,
      $.none,
      $.symbol,
      $.list,
      $.parenthesized_expression,
      $.unary_expression,
    ),

    // ── Blocks and fields ─────────────────────────────────────────

    block: $ => seq(
      optional($._decorators),
      field('kind', choice($._name, $.qualified_kind)),
      optional($.optional_marker),
      repeat(field('label', $._label)),
      choice(field('body', $.block_body), $._block_end),
    ),
    qualified_kind: $ => seq(
      field('namespace', $.dotted_name),
      '::',
      field('name', $.identifier),
    ),
    block_body: $ => seq('{', repeat($._item), '}'),

    _label: $ => choice(
      $.identifier,
      $.compound_identifier,
      $.string,
      $.interpolated_string,
      $.heredoc,
      $.number,
      $.boolean,
      $.symbol,
      $.none,
    ),
    // A bare label may run across `-` / `/` with no spaces:
    // `page getting-started/install`.
    compound_identifier: _ => token(
      /[a-zA-Z_][a-zA-Z0-9_]*([-\/][a-zA-Z0-9_]+)+/,
    ),

    field: $ => seq(
      optional($._decorators),
      field('name', choice($._name, $.string)),
      '=',
      field('value', $._expression),
    ),

    // ── Types ─────────────────────────────────────────────────────

    _type: $ => choice(
      $.reference_type,
      $._type_atom,
    ),
    reference_type: $ => seq('&', $._type_atom),
    _type_atom: $ => choice(
      $.function_type,
      $.named_type,
    ),
    named_type: $ => prec.right(seq(
      field('name', $.dotted_name),
      optional(field('arguments', $.type_arguments)),
    )),
    type_arguments: $ => seq(
      '<',
      commaSep1(choice($._type, $.tensor_dimensions)),
      optional(','),
      '>',
    ),
    tensor_dimensions: $ => seq(
      '[',
      commaSep1(choice($.number, $.identifier)),
      optional(','),
      ']',
    ),
    function_type: $ => prec.right(seq(
      'fn',
      '(',
      commaSep($._type),
      optional(','),
      ')',
      '->',
      field('return_type', $._type),
    )),

    // ── Expressions ───────────────────────────────────────────────

    _expression: $ => choice(
      $.number,
      $.string,
      $.interpolated_string,
      $.heredoc,
      $.boolean,
      $.none,
      $.symbol,
      $.identifier,
      $.parent,
      $.self,
      $.list,
      $.record,
      $.block_expression,
      $.parenthesized_expression,
      $.member_expression,
      $.call_expression,
      $.variant_expression,
      $.unary_expression,
      $.binary_expression,
      $.if_expression,
      $.match_expression,
      $.try_expression,
      $.function_expression,
    ),

    parent: _ => 'parent',
    self: _ => 'self',

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    list: $ => seq('[', commaSep($._expression), optional(','), ']'),

    record: $ => seq(
      '{',
      commaSep1($.record_field),
      optional(','),
      '}',
    ),
    record_field: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $._expression),
    ),

    block_expression: $ => seq(
      '{',
      repeat($.let_binding),
      $._expression,
      '}',
    ),
    let_binding: $ => seq(
      'let',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
      ';',
    ),

    // `steps.1` addresses a block by a numeric label.
    member_expression: $ => prec(PREC.member, seq(
      field('object', $._expression),
      '.',
      field('property', choice($.identifier, alias($.number, $.identifier))),
    )),

    call_expression: $ => prec(PREC.call, seq(
      field('function', $._expression),
      field('arguments', $.arguments),
    )),
    arguments: $ => seq('(', commaSep($._expression), optional(','), ')'),

    // `Shape::Circle`, `Shape::Circle(1.0)`, `Shape::Rect { w: 1, h: 2 }`.
    variant_expression: $ => prec.right(PREC.member, seq(
      field('type', $._expression),
      '::',
      field('variant', $.identifier),
      optional(field('arguments', choice(
        seq('(', $._expression, ')'),
        $.record,
      ))),
    )),

    unary_expression: $ => prec(PREC.unary, seq(
      field('operator', choice('-', '!')),
      field('operand', $._expression),
    )),

    binary_expression: $ => choice(
      ...[
        ['??', PREC.coalesce],
        ['||', PREC.or],
        ['&&', PREC.and],
        ['==', PREC.eq], ['!=', PREC.eq],
        ['<', PREC.cmp], ['<=', PREC.cmp], ['>', PREC.cmp], ['>=', PREC.cmp],
        ['+', PREC.add], ['-', PREC.add],
        ['*', PREC.mul], ['/', PREC.mul], ['%', PREC.mul],
      ].map(([op, p]) => prec.left(p, seq(
        field('left', $._expression),
        field('operator', op),
        field('right', $._expression),
      ))),
    ),

    if_expression: $ => prec.right(seq(
      'if',
      field('condition', choice($._expression, $.let_condition)),
      field('consequence', $.block_expression),
      optional(seq(
        'else',
        field('alternative', choice($.block_expression, $.if_expression)),
      )),
    )),
    let_condition: $ => seq(
      'let',
      field('pattern', $._pattern),
      '=',
      field('value', $._expression),
    ),

    match_expression: $ => seq(
      'match',
      field('value', $._expression),
      '{',
      repeat(seq($.match_arm, optional(','))),
      '}',
    ),
    match_arm: $ => prec.right(seq(
      field('pattern', $._pattern),
      repeat(seq('|', field('pattern', $._pattern))),
      optional(seq('if', field('guard', $._expression))),
      '=>',
      field('value', $._expression),
    )),

    // `try body catch e => handler` / `try { … } catch e { … }`.
    try_expression: $ => prec.right(seq(
      'try',
      field('body', $._expression),
      'catch',
      field('binding', $.identifier),
      choice(
        seq('=>', field('handler', $._expression)),
        field('handler', $.block_expression),
      ),
    )),

    function_expression: $ => prec.right(seq(
      'fn',
      field('parameters', $.parameters),
      '->',
      field('return_type', $._type),
      field('body', $._expression),
    )),
    parameters: $ => seq('(', commaSep($.parameter), optional(','), ')'),
    parameter: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $._type),
    ),

    // ── Patterns ──────────────────────────────────────────────────

    _pattern: $ => choice(
      $.wildcard_pattern,
      $.identifier,
      $.at_pattern,
      $.variant_pattern,
      $.number,
      $.string,
      $.boolean,
      $.none,
      $.symbol,
    ),
    wildcard_pattern: _ => '_',
    at_pattern: $ => seq(
      field('name', $.identifier),
      '@',
      field('pattern', $._pattern),
    ),
    variant_pattern: $ => prec.right(choice(
      seq(
        field('type', $.dotted_name),
        '::',
        field('variant', $.identifier),
        optional(field('arguments', $._variant_pattern_arguments)),
      ),
      seq(
        field('variant', $.identifier),
        field('arguments', $._variant_pattern_arguments),
      ),
    )),
    _variant_pattern_arguments: $ => choice(
      seq('(', $._pattern, ')'),
      $.record_pattern,
    ),
    record_pattern: $ => seq(
      '{',
      commaSep(choice($.record_pattern_field, $.rest_pattern)),
      optional(','),
      '}',
    ),
    record_pattern_field: $ => seq(
      field('name', $.identifier),
      optional(seq(':', field('pattern', $._pattern))),
    ),
    rest_pattern: _ => '..',

    // ── Literals ──────────────────────────────────────────────────

    // Digits with `_` separators in any base, an optional fraction and
    // exponent, then an optional suffix: a numeric type (`8080u16`) or a
    // literal unit (`5MiB`, `30s`). The lexer resolves which at
    // evaluation time; the suffix is part of the token either way.
    number: _ => token(seq(
      choice(
        /0[xX][0-9a-fA-F_]+/,
        /0[bB][01_]+/,
        /0[oO][0-7_]+/,
        /[0-9][0-9_]*(\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?)?/,
      ),
      optional(/[a-zA-Z][a-zA-Z0-9]*/),
    )),

    string: _ => token(seq(
      optional(choice(...ENCODINGS)),
      '"',
      repeat(choice(/[^"\\\n]/, /\\./)),
      '"',
    )),

    interpolated_string: $ => seq(
      alias(token(seq('$', optional(choice(...ENCODINGS)), '"')), '$"'),
      repeat(choice(
        $.string_content,
        $.escape_sequence,
        $.interpolation,
      )),
      token.immediate('"'),
    ),
    string_content: _ => token.immediate(prec(1, choice(/[^"\\$\n]+/, '$'))),
    escape_sequence: _ => token.immediate(prec(1, /\\./)),
    interpolation: $ => seq(
      token.immediate(prec(2, '${')),
      $._expression,
      '}',
    ),

    boolean: _ => choice('true', 'false'),
    none: _ => 'none',
    symbol: _ => token(seq(':', /[a-zA-Z_][a-zA-Z0-9_]*/)),

    // ── Identifiers ───────────────────────────────────────────────

    identifier: _ => /[a-zA-Z_][a-zA-Z0-9_]*/,
    dotted_name: $ => seq($.identifier, repeat(seq('.', $.identifier))),
  },
});

function commaSep(rule) {
  return optional(commaSep1(rule));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
