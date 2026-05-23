/**
 * Tree-sitter grammar for WCL.
 *
 * Coverage target: enough syntax for editor highlighting and outlines
 * (decorators, type / interface / union / fn declarations, blocks,
 * field assignments, literals, identifiers, operators, strings with
 * `$"${...}"` interpolation slots). Detailed type-system constructs
 * (`tensor<T, [...]>`, complex variant bodies, `match` arms) are
 * parsed as best-effort expression / type fragments — sufficient for
 * highlighting but not a re-implementation of the Rust parser.
 */

const PREC = {
  unary: 10,
  mul: 7,
  add: 6,
  cmp: 5,
  eq: 4,
  and: 3,
  or: 2,
  assign: 1,
};

const NUM_SUFFIXES = [
  'i8', 'i16', 'i32', 'i64', 'i128', 'isize',
  'u8', 'u16', 'u32', 'u64', 'u128', 'usize',
  'f32', 'f64',
];

module.exports = grammar({
  name: 'wcl',

  extras: $ => [/\s/, $.line_comment],

  word: $ => $.identifier,

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
      $.block,
      $.field,
    ),

    line_comment: _ => token(choice(
      seq('//', /[^\n]*/),
      seq('#',  /[^\n]*/),
    )),

    // ── Declarations ──────────────────────────────────────────────

    namespace_decl: $ => seq('namespace', $.dotted_name),

    use_decl: $ => seq(
      'use',
      $.dotted_name,
      optional(choice(
        seq('as', $.identifier),
        seq('.', '{', commaSep1($.use_item), '}'),
      )),
    ),
    use_item: $ => seq($.identifier, optional(seq('as', $.identifier))),

    import_decl: $ => seq('import', $.string),

    type_decl: $ => seq(
      repeat($.decorator),
      'type',
      field('name', $.dotted_name),
      optional(seq('extends', commaSep1($.dotted_name))),
      $.field_block,
    ),

    interface_decl: $ => seq(
      repeat($.decorator),
      'interface',
      field('name', $.dotted_name),
      optional(seq('extends', commaSep1($.dotted_name))),
      $.field_block,
    ),

    union_decl: $ => seq(
      repeat($.decorator),
      'union',
      field('name', $.dotted_name),
      optional(seq('extends', commaSep1($.dotted_name))),
      '{',
      repeat($.union_variant),
      '}',
    ),
    union_variant: $ => seq(
      repeat($.decorator),
      field('name', $.identifier),
      optional(choice(
        seq('{', repeat($.type_field), '}'),
        seq('&', $.dotted_name),
        $.type_ref,
        'none',
      )),
    ),

    symbol_set_decl: $ => seq(
      repeat($.decorator),
      'symbol_set',
      field('name', $.dotted_name),
      '{',
      repeat(seq(repeat($.decorator), $.identifier)),
      '}',
    ),

    connection_decl: $ => seq(
      'connection',
      field('name', $.dotted_name),
      ':',
      $.type_ref,
      '->',
      $.type_ref,
      ':',
      $.dotted_name,
    ),

    field_block: $ => seq('{', repeat($.type_field), '}'),
    type_field: $ => seq(
      repeat($.decorator),
      field('name', $.identifier),
      ':',
      $.type_ref,
      optional('?'),
    ),

    // ── Decorators ────────────────────────────────────────────────

    decorator: $ => seq(
      '@',
      field('name', $.dotted_name),
      optional(seq('(', commaSep($._expr_or_named), ')')),
    ),
    _expr_or_named: $ => choice($.named_arg, $._expr),
    named_arg: $ => seq($.identifier, '=', $._expr),

    // ── Blocks / fields ───────────────────────────────────────────

    block: $ => seq(
      repeat($.decorator),
      field('kind', $.identifier),
      repeat($._block_label),
      '{',
      repeat($._item),
      '}',
    ),
    _block_label: $ => choice($.identifier, $.string, $.number, $.symbol),

    field: $ => seq(
      repeat($.decorator),
      field('name', $.identifier),
      '=',
      $._expr,
    ),

    // ── Types ─────────────────────────────────────────────────────

    type_ref: $ => choice(
      $.dotted_name,
      seq('&', $.dotted_name),
      seq('list', '<', $.type_ref, '>'),
      seq('[', $.type_ref, ']'),
      seq(
        'fn',
        '(',
        commaSep($.type_ref),
        ')',
        '->',
        $.type_ref,
      ),
    ),

    // ── Expressions ───────────────────────────────────────────────

    _expr: $ => choice(
      $.number,
      $.string,
      $.interpolated_string,
      $.bool,
      $.none,
      $.symbol,
      $.identifier,
      $.list_lit,
      $.member,
      $.call,
      $.unary,
      $.binary,
      $.parens,
      $.if_expr,
      $.match_expr,
      $.block_expr,
      $.function_lit,
    ),

    parens:    $ => seq('(', $._expr, ')'),
    list_lit:  $ => seq('[', commaSep($._expr), optional(','), ']'),

    member: $ => prec.left(11, seq(
      $._expr, '.', $.identifier,
    )),

    call: $ => prec.left(11, seq(
      field('callee', $._expr),
      '(',
      commaSep($._expr),
      ')',
    )),

    unary: $ => prec(PREC.unary, choice(
      seq('-', $._expr),
      seq('!', $._expr),
    )),

    binary: $ => choice(
      ...[
        ['*', PREC.mul], ['/', PREC.mul], ['%', PREC.mul],
        ['+', PREC.add], ['-', PREC.add],
        ['<', PREC.cmp], ['<=', PREC.cmp], ['>', PREC.cmp], ['>=', PREC.cmp],
        ['==', PREC.eq], ['!=', PREC.eq],
        ['&&', PREC.and],
        ['||', PREC.or],
      ].map(([op, p]) => prec.left(p, seq($._expr, op, $._expr))),
    ),

    if_expr: $ => seq(
      'if', $._expr, $.block_expr,
      'else', choice($.block_expr, $.if_expr),
    ),

    match_expr: $ => seq(
      'match', $._expr, '{',
      repeat(seq($._expr, '=>', $._expr, optional(','))),
      '}',
    ),

    block_expr: $ => seq(
      '{',
      repeat(seq($.let_binding, ';')),
      $._expr,
      '}',
    ),
    let_binding: $ => seq('let', $.identifier, '=', $._expr),

    function_lit: $ => seq(
      'fn',
      '(',
      commaSep($.fn_param),
      ')',
      '->',
      $.type_ref,
      $.block_expr,
    ),
    fn_param: $ => seq($.identifier, ':', $.type_ref),

    // ── Literals ──────────────────────────────────────────────────

    number: _ => token(seq(
      optional('-'),
      choice(
        seq('0x', /[0-9a-fA-F_]+/),
        seq('0b', /[01_]+/),
        seq('0o', /[0-7_]+/),
        /[0-9][0-9_]*(\.[0-9_]+)?([eE][+-]?[0-9_]+)?/,
      ),
      optional(choice(...NUM_SUFFIXES)),
    )),

    string: _ => token(seq(
      optional(choice('ascii', 'utf16', 'utf32', 'utf8')),
      '"',
      repeat(choice(/[^"\\]/, /\\./)),
      '"',
    )),

    interpolated_string: _ => token(seq(
      '$',
      optional(choice('ascii', 'utf16', 'utf32', 'utf8')),
      '"',
      repeat(choice(/[^"\\$]/, /\\./, /\$\{[^}]*\}/)),
      '"',
    )),

    bool:   _ => choice('true', 'false'),
    none:   _ => 'none',
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
