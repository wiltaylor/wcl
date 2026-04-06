/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  TERNARY: 1,
  OR: 2,
  AND: 3,
  EQUALITY: 4,
  COMPARISON: 5,
  ADDITIVE: 6,
  MULTIPLICATIVE: 7,
  UNARY: 8,
  POSTFIX: 9,
  CALL: 10,
};

export default grammar({
  name: "wcl",

  extras: ($) => [/\s/, $.line_comment, $.block_comment, $.doc_comment],

  externals: ($) => [$.heredoc_start, $.heredoc_body, $.heredoc_end],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$._primary_expression, $.lambda_parameters],
    [$._primary_expression, $.selector],
    [$._primary_expression, $.selector, $.lambda_parameters],
    [$.selector],
    [$.pipeline],
    [$.filter],
  ],

  rules: {
    // Top-level
    document: ($) => repeat($._doc_item),

    _doc_item: ($) =>
      choice(
        $.import_declaration,
        $.export_declaration,
        $.namespace_declaration,
        $.use_declaration,
        $._body_item,
      ),

    _body_item: ($) =>
      choice(
        $.attribute,
        $.block,
        $.table,
        $.let_binding,
        $.macro_definition,
        $.macro_call,
        $.for_loop,
        $.conditional,
        $.validation,
        $.schema,
        $.struct_definition,
        $.decorator_schema,
        $.declare_statement,
        $.symbol_set_declaration,
      ),

    // Imports
    import_declaration: ($) =>
      seq(
        "import",
        optional("?"),
        choice($.string_literal, $.library_import),
        optional($.lazy_modifier),
      ),

    lazy_modifier: ($) => seq("lazy", "(", $.namespace_path, ")"),

    library_import: ($) => seq("<", /[^>]+/, ">"),

    // Exports
    export_declaration: ($) =>
      choice(
        seq("export", "let", $.identifier, "=", $.expression),
        seq("export", $.identifier),
      ),

    // Namespaces
    namespace_path: ($) => seq($.identifier, repeat(seq("::", $.identifier))),

    namespace_declaration: ($) =>
      choice(
        seq("namespace", $.namespace_path, "{", repeat($._doc_item), "}"),
        seq("namespace", $.namespace_path),
      ),

    // Use declarations — path is inlined to avoid conflict with namespace_path's :: repeat
    use_declaration: ($) =>
      seq(
        "use",
        $.identifier,
        repeat(seq("::", $.identifier)),
        "::",
        choice($.use_target, seq("{", commaSep1($.use_target), "}")),
      ),

    use_target: ($) =>
      seq($.identifier, optional(seq("->", $.identifier))),

    // Attributes
    attribute: ($) =>
      seq(repeat($.decorator), $.identifier, "=", $.expression),

    // Blocks
    block: ($) =>
      seq(
        repeat($.decorator),
        optional("partial"),
        $.identifier,
        optional($.identifier_literal),
        repeat($.inline_arg),
        $.block_body,
      ),

    inline_arg: ($) =>
      choice(
        $.integer_literal,
        $.float_literal,
        $.date_literal,
        $.duration_literal,
        $.string_literal,
        $.boolean_literal,
        $.null_literal,
        $.symbol_literal,
        $.identifier,
        $.list_literal,
      ),

    block_body: ($) => seq("{", repeat($._body_item), "}"),

    // Let bindings
    let_binding: ($) =>
      seq(optional("partial"), "let", $.identifier, "=", $.expression),

    // Control flow
    for_loop: ($) =>
      seq(
        "for",
        $.identifier,
        optional(seq(",", $.identifier)),
        "in",
        $.expression,
        "{",
        repeat($._body_item),
        "}",
      ),

    conditional: ($) =>
      seq(
        "if",
        $.expression,
        "{",
        repeat($._body_item),
        "}",
        optional($.else_branch),
      ),

    else_branch: ($) =>
      seq(
        "else",
        choice($.conditional, seq("{", repeat($._body_item), "}")),
      ),

    // Tables
    table: ($) =>
      seq(
        repeat($.decorator),
        optional("partial"),
        "table",
        optional(choice($.identifier_literal, $.identifier)),
        optional(seq(":", $.identifier)),
        choice(
          seq("{", repeat($.column_declaration), repeat($.table_row), "}"),
          seq("=", $.expression),
        ),
      ),

    column_declaration: ($) =>
      seq(repeat($.decorator), $.identifier, ":", $.type_expression),

    table_row: ($) =>
      seq("|", $.expression, repeat(seq("|", $.expression)), "|"),

    // Schemas
    schema: ($) =>
      seq(
        repeat($.decorator),
        "schema",
        $.string_literal,
        "{",
        repeat(choice($.schema_field, $.schema_variant)),
        "}",
      ),

    schema_variant: ($) =>
      seq(
        repeat($.decorator),
        "variant",
        $.string_literal,
        "{",
        repeat($.schema_field),
        "}",
      ),

    schema_field: ($) =>
      prec.right(
        seq(
          repeat($.decorator),
          $.identifier,
          ":",
          $.type_expression,
          repeat($.decorator),
        ),
      ),

    // Struct definitions
    struct_definition: ($) =>
      seq(
        repeat($.decorator),
        "struct",
        $.string_literal,
        "{",
        repeat(choice($.schema_field, $.schema_variant)),
        "}",
      ),

    // Decorator schemas
    decorator_schema: ($) =>
      seq(
        repeat($.decorator),
        "decorator_schema",
        $.string_literal,
        "{",
        $.decorator_schema_body,
        "}",
      ),

    decorator_schema_body: ($) =>
      seq(
        "target",
        "=",
        "[",
        commaSep1($.target_type),
        "]",
        repeat($.schema_field),
      ),

    target_type: ($) => choice("block", "attribute", "table", "schema", "let"),

    // Decorators
    decorator: ($) =>
      seq(
        "@",
        $.identifier,
        optional(seq("(", optional($.decorator_arguments), ")")),
      ),

    decorator_arguments: ($) => commaSep1($._decorator_arg),

    _decorator_arg: ($) => choice($.named_argument, $.expression),

    named_argument: ($) => seq($.identifier, "=", $.expression),

    // Macros
    macro_definition: ($) =>
      seq(
        repeat($.decorator),
        "macro",
        choice($.function_macro, $.attribute_macro),
      ),

    function_macro: ($) =>
      seq(
        $.identifier,
        "(",
        optional($.parameter_list),
        ")",
        "{",
        repeat($._body_item),
        "}",
      ),

    attribute_macro: ($) =>
      seq(
        "@",
        $.identifier,
        "(",
        optional($.parameter_list),
        ")",
        "{",
        repeat($.transform_directive),
        "}",
      ),

    parameter_list: ($) => commaSep1($.parameter),

    parameter: ($) =>
      seq(
        $.identifier,
        optional(seq(":", $.type_expression)),
        optional(seq("=", $.expression)),
      ),

    macro_call: ($) =>
      seq($.identifier, "(", optional($.argument_list), ")"),

    argument_list: ($) => commaSep1(choice($.named_argument, $.expression)),

    // Transform directives (attribute macros)
    transform_directive: ($) =>
      choice(
        $.inject_block,
        $.set_block,
        $.remove_block,
        $.when_block,
      ),

    inject_block: ($) => seq("inject", "{", repeat($._body_item), "}"),

    set_block: ($) => seq("set", "{", repeat($.attribute), "}"),

    remove_block: ($) => seq("remove", "[", commaSep1($.identifier), "]"),

    when_block: ($) =>
      seq("when", $.expression, "{", repeat($.transform_directive), "}"),

    // Validation
    validation: ($) =>
      seq(
        repeat($.decorator),
        "validation",
        $.string_literal,
        "{",
        repeat($.let_binding),
        "check",
        "=",
        $.expression,
        "message",
        "=",
        $.expression,
        "}",
      ),

    // Declare statements (library function stubs)
    declare_statement: ($) =>
      seq(
        "declare",
        $.identifier,
        "(",
        optional($.parameter_list),
        ")",
        optional(seq("->", $.type_expression)),
      ),

    // Type expressions
    type_expression: ($) =>
      choice(
        $.builtin_type,
        $.list_type,
        $.map_type,
        $.set_type,
        $.ref_type,
        $.union_type,
      ),

    builtin_type: ($) =>
      choice("string", "bool", "null", "identifier", "any", "symbol",
             "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64",
             "i128", "u128", "f32", "f64", "date", "duration"),

    list_type: ($) => seq("list", "(", $.type_expression, ")"),
    map_type: ($) =>
      seq("map", "(", $.type_expression, ",", $.type_expression, ")"),
    set_type: ($) => seq("set", "(", $.type_expression, ")"),
    ref_type: ($) => seq("ref", "(", $.string_literal, ")"),
    union_type: ($) => seq("union", "(", commaSep1($.type_expression), ")"),

    // Expressions
    expression: ($) => $._ternary_expression,

    _ternary_expression: ($) =>
      choice($.ternary_expression, $._or_expression),

    ternary_expression: ($) =>
      prec.right(
        PREC.TERNARY,
        seq($._or_expression, "?", $.expression, ":", $.expression),
      ),

    _or_expression: ($) => choice($.or_expression, $._and_expression),

    or_expression: ($) =>
      prec.left(PREC.OR, seq($._or_expression, "||", $._and_expression)),

    _and_expression: ($) =>
      choice($.and_expression, $._equality_expression),

    and_expression: ($) =>
      prec.left(
        PREC.AND,
        seq($._and_expression, "&&", $._equality_expression),
      ),

    _equality_expression: ($) =>
      choice($.equality_expression, $._comparison_expression),

    equality_expression: ($) =>
      prec.left(
        PREC.EQUALITY,
        seq(
          $._equality_expression,
          choice("==", "!="),
          $._comparison_expression,
        ),
      ),

    _comparison_expression: ($) =>
      choice($.comparison_expression, $._additive_expression),

    comparison_expression: ($) =>
      prec.left(
        PREC.COMPARISON,
        seq(
          $._comparison_expression,
          choice("<", ">", "<=", ">=", "=~"),
          $._additive_expression,
        ),
      ),

    _additive_expression: ($) =>
      choice($.additive_expression, $._multiplicative_expression),

    additive_expression: ($) =>
      prec.left(
        PREC.ADDITIVE,
        seq(
          $._additive_expression,
          choice("+", "-"),
          $._multiplicative_expression,
        ),
      ),

    _multiplicative_expression: ($) =>
      choice($.multiplicative_expression, $._unary_expression),

    multiplicative_expression: ($) =>
      prec.left(
        PREC.MULTIPLICATIVE,
        seq(
          $._multiplicative_expression,
          choice("*", "/", "%"),
          $._unary_expression,
        ),
      ),

    _unary_expression: ($) =>
      choice($.unary_expression, $._postfix_expression),

    unary_expression: ($) =>
      prec(PREC.UNARY, seq(choice("!", "-"), $._unary_expression)),

    _postfix_expression: ($) =>
      choice(
        $.member_expression,
        $.index_expression,
        $.call_expression,
        $._primary_expression,
      ),

    member_expression: ($) =>
      prec.left(PREC.POSTFIX, seq($._postfix_expression, ".", $.identifier)),

    index_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq($._postfix_expression, "[", $.expression, "]"),
      ),

    call_expression: ($) =>
      prec.left(
        PREC.CALL,
        seq($._postfix_expression, "(", optional($.argument_list), ")"),
      ),

    _primary_expression: ($) =>
      choice(
        $.integer_literal,
        $.float_literal,
        $.date_literal,
        $.duration_literal,
        $.string_literal,
        $.heredoc_literal,
        $.boolean_literal,
        $.null_literal,
        $.symbol_literal,
        $.identifier_literal,
        $.identifier,
        $.list_literal,
        $.map_literal,
        $.parenthesized_expression,
        $.pipeline,
        $.import_table_expression,
        $.import_raw_expression,
        $.ref_expression,
        $.lambda_expression,
      ),

    // Query pipelines are first-class expressions
    pipeline: ($) =>
      seq($.selector, repeat(seq("|", $.filter))),

    selector: ($) =>
      choice(
        seq(
          optional(".."),
          $.identifier,
          optional(seq("#", $.identifier_literal)),
          repeat(seq(".", $.identifier)),
        ),
        ".",
        "*",
      ),

    filter: ($) =>
      choice(
        seq(
          ".",
          $.identifier,
          optional(
            seq(
              choice("==", "!=", "<", ">", "<=", ">=", "=~"),
              $.expression,
            ),
          ),
        ),
        seq("has", "(", choice(seq(".", $.identifier), seq("@", $.identifier)), ")"),
        seq(
          "@",
          $.identifier,
          ".",
          $.identifier,
          choice("==", "!=", "<", ">", "<=", ">="),
          $.expression,
        ),
      ),

    // Import utility expressions
    import_table_arg: ($) =>
      choice(
        $.string_literal,
        seq($.identifier, "=", $.expression),
      ),

    import_table_expression: ($) =>
      seq(
        "import_table",
        "(",
        $.string_literal,
        repeat(seq(",", $.import_table_arg)),
        ")",
      ),

    import_raw_expression: ($) =>
      seq("import_raw", "(", $.string_literal, ")"),

    // Ref expression — accepts bare identifiers or string paths for qualified/relative refs
    ref_expression: ($) =>
      seq("ref", "(", choice($.identifier_literal, field("ident", $.identifier), $.string_literal), ")"),

    // Literals
    integer_literal: ($) =>
      token(
        choice(
          /0[xX][0-9a-fA-F][0-9a-fA-F_]*/,
          /0[oO][0-7][0-7_]*/,
          /0[bB][01][01_]*/,
          /[0-9][0-9_]*/,
        ),
      ),

    float_literal: ($) =>
      token(/[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?/),

    date_literal: ($) => token(seq("d", token.immediate(/"[^"]*"/))),

    duration_literal: ($) => token(seq("dur", token.immediate(/"[^"]*"/))),

    string_literal: ($) =>
      seq('"', repeat(choice($.interpolation, $.escape_sequence, $.string_content)), '"'),

    string_content: ($) => token.immediate(prec(-1, /[^"\\$]+|\$[^{]/)),

    interpolation: ($) =>
      seq(token.immediate("${"), $.expression, "}"),

    escape_sequence: ($) =>
      token.immediate(
        /\\["\\/nrt]|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}/,
      ),

    // Heredoc: uses an external scanner (src/scanner.c) to correctly
    // match opening and closing delimiters.
    heredoc_literal: ($) =>
      seq($.heredoc_start, optional($.heredoc_body), $.heredoc_end),

    symbol_literal: ($) => token(seq(":", /[a-zA-Z_][a-zA-Z0-9_]*/)),

    // Symbol sets
    symbol_set_declaration: ($) =>
      seq(
        "symbol_set",
        field("name", $.identifier),
        "{",
        repeat($.symbol_set_member),
        "}",
      ),

    symbol_set_member: ($) =>
      seq($.symbol_literal, optional(seq("=", $.string_literal))),

    boolean_literal: ($) => choice("true", "false"),

    null_literal: ($) => "null",

    identifier_literal: ($) => /[a-zA-Z_][a-zA-Z0-9_]*-[a-zA-Z0-9_-]*/,

    // Collections
    list_literal: ($) =>
      seq("[", optional(commaSep1($.expression)), optional(","), "]"),

    map_literal: ($) => seq("{", optional(repeat1($.map_entry)), "}"),

    map_entry: ($) =>
      seq(
        choice($.identifier, $.string_literal),
        choice("=", ":"),
        $.expression,
        optional(","),
      ),

    // Lambda
    lambda_expression: ($) =>
      prec.right(
        seq($.lambda_parameters, "=>", choice($.block_expression, $.expression)),
      ),

    block_expression: ($) =>
      seq("{", repeat($.let_binding), $.expression, "}"),

    lambda_parameters: ($) =>
      choice($.identifier, seq("(", optional(commaSep1($.identifier)), ")")),

    // Grouping
    parenthesized_expression: ($) => seq("(", $.expression, ")"),

    // Identifiers
    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    // Comments
    doc_comment: ($) => token(seq("///", /.*/)),
    line_comment: ($) => token(seq("//", /.*/)),
    block_comment: ($) => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/")),
  },
});

/**
 * Comma-separated list with at least one element.
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)), optional(","));
}
