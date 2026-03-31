; WCL highlight queries for tree-sitter
; Compatible with Neovim, Helix, Zed, GitHub, and any tree-sitter-highlight consumer.
;
; This is the canonical copy. Editor-specific query files (locals.scm,
; textobjects.scm, injections.scm) live in extras/highlight-queries/.

; ── Keywords ──────────────────────────────────────────────────────────────

[
  "if"
  "else"
  "for"
  "in"
  "when"
] @keyword.control

[
  "let"
  "partial"
  "macro"
  "schema"
  "table"
  "validation"
  "decorator_schema"
  "declare"
  "variant"
  "symbol_set"
] @keyword

[
  "import"
  "export"
] @keyword.import

[
  "inject"
  "set"
  "remove"
  "check"
  "message"
  "target"
] @keyword

; ── Operators ─────────────────────────────────────────────────────────────

[
  "="
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "=~"
  "&&"
  "||"
  "!"
  "?"
  ":"
  "=>"
  "->"
  "|"
] @operator

; ── Punctuation ───────────────────────────────────────────────────────────

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
] @punctuation.delimiter

"@" @punctuation.special

; ── Literals ──────────────────────────────────────────────────────────────

(integer_literal) @number
(float_literal) @number.float

(string_literal) @string
(string_content) @string
(escape_sequence) @string.escape
(interpolation
  "${" @punctuation.special
  "}" @punctuation.special)
(heredoc_literal) @string

(date_literal) @string.special
(duration_literal) @string.special

(symbol_literal) @constant

(boolean_literal) @constant.builtin
(null_literal) @constant.builtin

; ── Types ─────────────────────────────────────────────────────────────────

(builtin_type) @type.builtin

[
  "list"
  "map"
  "set"
  "union"
  "ref"
] @type.builtin

; ── Identifiers ───────────────────────────────────────────────────────────

(identifier_literal) @string.special

; ── Decorators ────────────────────────────────────────────────────────────

(decorator
  "@" @attribute
  (identifier) @attribute)

; ── Functions ─────────────────────────────────────────────────────────────

(call_expression
  (identifier) @function.call)
(macro_call
  (identifier) @function.macro)
(function_macro
  (identifier) @function.macro)
(attribute_macro
  (identifier) @function.macro)
(declare_statement
  (identifier) @function)

; ── Blocks ────────────────────────────────────────────────────────────────

(block
  (identifier) @type)

; ── Schema names ──────────────────────────────────────────────────────────

(schema
  (string_literal) @type)
(decorator_schema
  (string_literal) @type)

; ── Variables ─────────────────────────────────────────────────────────────

(let_binding
  (identifier) @variable)

; ── Attributes (fields) ──────────────────────────────────────────────────

(attribute
  (identifier) @property)

; ── Parameters ────────────────────────────────────────────────────────────

(parameter
  (identifier) @variable.parameter)

(lambda_parameters
  (identifier) @variable.parameter)

; ── For loop variable ─────────────────────────────────────────────────────

(for_loop
  (identifier) @variable)

; ── Column declarations ──────────────────────────────────────────────────

(column_declaration
  (identifier) @property)

; ── Schema fields ────────────────────────────────────────────────────────

(schema_field
  (identifier) @property)

; ── Named arguments ──────────────────────────────────────────────────────

(named_argument
  (identifier) @property)

; ── Query expressions ────────────────────────────────────────────────────

[
  "query"
  "has"
  "import_table"
  "import_raw"
] @function.builtin

; ── Introspection builtins ──────────────────────────────────────────
((call_expression
  (identifier) @function.builtin)
  (#any-of? @function.builtin "is_imported" "has_schema"))

(selector
  (identifier) @type)
(filter
  (identifier) @property)

; ── Target types ─────────────────────────────────────────────────────────

(target_type) @type.builtin

; ── Comments ─────────────────────────────────────────────────────────────

(doc_comment) @comment.documentation
(line_comment) @comment
(block_comment) @comment
