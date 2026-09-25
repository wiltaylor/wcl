; Highlight queries for WCL. Capture names follow the common
; nvim-treesitter / Helix / Zed vocabulary.

(comment) @comment

; ── Keywords ────────────────────────────────────────────────────

[
  "type"
  "interface"
  "union"
  "symbol_set"
  "connection"
  "slot"
  "extends"
] @keyword.type

[
  "namespace"
  "use"
  "import"
  "as"
] @keyword.import

[
  "let"
  "fn"
] @keyword

[
  "if"
  "else"
  "match"
] @keyword.conditional

[
  "try"
  "catch"
] @keyword.exception

[
  (parent)
  (self)
] @variable.builtin

; ── Literals ────────────────────────────────────────────────────

(number) @number
(boolean) @boolean
(none) @constant.builtin
(symbol) @string.special.symbol

(string) @string
(heredoc) @string
(interpolated_string "$\"" @string)
(interpolated_string "\"" @string)
(string_content) @string
(escape_sequence) @string.escape
(interpolation
  "${" @punctuation.special
  "}" @punctuation.special)
(system_import_path) @string.special.path

; ── Declarations ────────────────────────────────────────────────

(type_decl name: (dotted_name (identifier) @type.definition))
(interface_decl name: (dotted_name (identifier) @type.definition))
(union_decl name: (dotted_name (identifier) @type.definition))
(symbol_set_decl name: (dotted_name (identifier) @type.definition))
(connection_decl name: (dotted_name (identifier) @type.definition))
(namespace_decl name: (dotted_name (identifier) @module))
(use_decl path: (dotted_name (identifier) @module))
(extends_clause (dotted_name (identifier) @type))

(union_variant name: (identifier) @constructor)
(symbol_entry name: (identifier) @constant)
(type_field name: (identifier) @property)
(record_field name: (identifier) @property)
(record_pattern_field name: (identifier) @property)
(named_argument name: (identifier) @variable.parameter)
(parameter name: (identifier) @variable.parameter)

(fn_item name: (identifier) @function)
(let_item name: (identifier) @variable)
(let_binding name: (identifier) @variable)
(slot_decl name: (identifier) @variable)

; ── Types ───────────────────────────────────────────────────────

(named_type name: (dotted_name (identifier) @type))
(connection_decl kinds: (dotted_name (identifier) @type))

; ── Blocks and fields ───────────────────────────────────────────

(block kind: (identifier) @tag)
(qualified_kind
  namespace: (dotted_name (identifier) @module)
  name: (identifier) @tag)
(block label: (identifier) @label)
(compound_identifier) @label

(field name: (identifier) @property)
(table name: (identifier) @property)
(connection source: (identifier) @variable)
(connection destination: (identifier) @variable)

; ── Decorators ──────────────────────────────────────────────────

(decorator "@" @attribute)
(decorator name: (dotted_name (identifier) @attribute))

; ── Expressions ─────────────────────────────────────────────────

(call_expression function: (identifier) @function.call)
(call_expression
  function: (member_expression property: (identifier) @function.method.call))
(member_expression property: (identifier) @property)
(variant_expression variant: (identifier) @constructor)
(variant_pattern variant: (identifier) @constructor)
(wildcard_pattern) @variable.builtin
(rest_pattern) @operator

; ── Operators and punctuation ───────────────────────────────────

[
  "??" "||" "&&" "==" "!=" "<" "<=" ">" ">="
  "+" "-" "*" "/" "%" "!" "=" "=>" "->" "&" "|" "::"
] @operator

(optional_marker) @operator
(repeat_marker) @operator

[ "(" ")" "[" "]" "{" "}" ] @punctuation.bracket
[ "," "." ":" ";" ] @punctuation.delimiter
