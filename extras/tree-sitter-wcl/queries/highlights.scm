; Keywords
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
] @keyword

[
  "import"
  "export"
  "inject"
  "set"
  "remove"
  "check"
  "message"
  "target"
] @keyword.import

; Operators
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

; Punctuation
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

; Literals
(integer_literal) @number
(float_literal) @number.float
(string_literal) @string
(string_content) @string
(escape_sequence) @string.escape
(interpolation
  "${" @punctuation.special
  "}" @punctuation.special)
(boolean_literal) @boolean
(null_literal) @constant.builtin

; Types
(builtin_type) @type.builtin
[
  "list"
  "map"
  "set"
  "union"
  "ref"
] @type.builtin

; Identifiers
(identifier_literal) @string.special

; Decorators
(decorator
  "@" @attribute
  (identifier) @attribute)

; Functions
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

; Blocks
(block
  (identifier) @type)

; Schema names
(schema
  (string_literal) @type)
(decorator_schema
  (string_literal) @type)

; Let bindings
(let_binding
  (identifier) @variable)

; Attributes
(attribute
  (identifier) @property)

; Parameters
(parameter
  (identifier) @variable.parameter)

; Lambda
(lambda_parameters
  (identifier) @variable.parameter)

; For loop variable
(for_loop
  (identifier) @variable)

; Column declarations
(column_declaration
  (identifier) @property)

; Schema fields
(schema_field
  (identifier) @property)

; Named arguments
(named_argument
  (identifier) @property)

; Target types
(target_type) @type.builtin

; Comments
(doc_comment) @comment.documentation
(line_comment) @comment
(block_comment) @comment
