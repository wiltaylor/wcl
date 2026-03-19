# EBNF Grammar

This is the complete EBNF grammar for WCL.

Notation:
- `=` — rule definition
- `|` — alternation
- `{ ... }` — zero or more repetitions
- `[ ... ]` — zero or one (optional)
- `( ... )` — grouping
- `"..."` — literal terminal
- `UPPER` — named terminal

```ebnf
(* ===== Top-level ===== *)
document        = trivia { doc_item } trivia ;
doc_item        = import_decl | export_decl | body_item ;
body            = { body_item } ;
body_item       = attribute
                | block
                | table
                | let_binding
                | macro_def
                | macro_call
                | for_loop
                | conditional
                | validation
                | schema
                | decorator_schema
                | comment ;

(* ===== Import Directives (top-level only) ===== *)
import_decl     = "import" STRING_LIT ;

(* ===== Export Declarations (top-level only) ===== *)
export_decl     = "export" "let" IDENT "=" expression
                | "export" IDENT ;

(* ===== Attributes ===== *)
attribute       = { decorator } IDENT "=" expression ;

(* ===== Blocks ===== *)
block           = { decorator } [ "partial" ] IDENT [ IDENTIFIER_LIT ]
                  { STRING_LIT } "{" body "}" ;

(* ===== Let Bindings ===== *)
let_binding     = "let" IDENT "=" expression ;

(* ===== Control Flow ===== *)
for_loop        = "for" IDENT [ "," IDENT ] "in" expression "{" body "}" ;
conditional     = "if" expression "{" body "}" [ else_branch ] ;
else_branch     = "else" ( conditional | "{" body "}" ) ;

(* ===== Tables ===== *)
table           = { decorator } [ "partial" ] "table" [ IDENTIFIER_LIT ]
                  { STRING_LIT } "{" table_body "}" ;
table_body      = { column_decl } { table_row } ;
column_decl     = { decorator } IDENT ":" type_expr ;
table_row       = "|" expression { "|" expression } "|" ;

(* ===== Schemas ===== *)
schema          = { decorator } "schema" STRING_LIT "{" { schema_field } "}" ;
schema_field    = { decorator } IDENT "=" type_expr { decorator } ;

(* ===== Decorator Schemas ===== *)
decorator_schema = { decorator } "decorator_schema" STRING_LIT
                   "{" decorator_schema_body "}" ;
decorator_schema_body = "target" "=" "[" target_list "]" { schema_field } ;
target_list     = target { "," target } ;
target          = "block" | "attribute" | "table" | "schema" ;

(* ===== Decorators ===== *)
decorator       = "@" IDENT [ "(" decorator_args ")" ] ;
decorator_args  = positional_args [ "," named_args ]
                | named_args ;
positional_args = expression { "," expression } ;
named_args      = named_arg { "," named_arg } ;
named_arg       = IDENT "=" expression ;

(* ===== Macros ===== *)
macro_def       = { decorator } "macro" ( func_macro_def | attr_macro_def ) ;
func_macro_def  = IDENT "(" param_list ")" "{" body "}" ;
attr_macro_def  = "@" IDENT "(" param_list ")" "{" transform_body "}" ;
param_list      = [ param { "," param } ] ;
param           = IDENT [ ":" type_expr ] [ "=" expression ] ;
macro_call      = IDENT "(" [ arg_list ] ")" ;
arg_list        = expression { "," expression } [ "," named_args ] ;

(* ===== Transform Body (attribute macros) ===== *)
transform_body  = { transform_directive } ;
transform_directive = inject_block | set_block | remove_block | when_block ;
inject_block    = "inject" "{" body "}" ;
set_block       = "set" "{" { attribute } "}" ;
remove_block    = "remove" "[" ident_list "]" ;
ident_list      = IDENT { "," IDENT } [ "," ] ;
when_block      = "when" expression "{" { transform_directive } "}" ;

(* ===== Validation ===== *)
validation      = { decorator } "validation" STRING_LIT "{"
                  { let_binding } "check" "=" expression
                  "message" "=" expression "}" ;

(* ===== Types ===== *)
type_expr       = "string" | "int" | "float" | "bool" | "null"
                | "identifier" | "any"
                | "list" "(" type_expr ")"
                | "map" "(" type_expr "," type_expr ")"
                | "set" "(" type_expr ")"
                | "ref" "(" STRING_LIT ")"
                | "union" "(" type_expr { "," type_expr } ")" ;

(* ===== Expressions ===== *)
expression      = ternary_expr ;
ternary_expr    = or_expr [ "?" expression ":" expression ] ;
or_expr         = and_expr { "||" and_expr } ;
and_expr        = equality_expr { "&&" equality_expr } ;
equality_expr   = comparison_expr { ( "==" | "!=" ) comparison_expr } ;
comparison_expr = additive_expr { ( "<" | ">" | "<=" | ">=" | "=~" )
                  additive_expr } ;
additive_expr   = multiplicative_expr { ( "+" | "-" ) multiplicative_expr } ;
multiplicative_expr = unary_expr { ( "*" | "/" | "%" ) unary_expr } ;
unary_expr      = ( "!" | "-" ) unary_expr | postfix_expr ;
postfix_expr    = primary_expr { ( "." IDENT | "[" expression "]"
                | "(" [ arg_list ] ")" ) } ;
primary_expr    = INT_LIT | FLOAT_LIT | STRING_LIT | BOOL_LIT | NULL_LIT
                | IDENTIFIER_LIT | IDENT
                | list_literal | map_literal
                | "(" expression ")"
                | query_expr
                | import_util_expr
                | ref_expr
                | lambda_expr ;

(* ===== Special Expressions ===== *)
query_expr      = "query" "(" pipeline ")" ;
pipeline        = selector { "|" filter } ;
selector        = [ ".." ] IDENT [ "#" IDENTIFIER_LIT ]
                  { "." ( IDENT | STRING_LIT ) }
                | "." | "*" ;
filter          = "." IDENT [ ( "==" | "!=" | "<" | ">" | "<=" | ">="
                  | "=~" ) expression ]
                | "has" "(" ( "." IDENT | "@" IDENT ) ")"
                | "@" IDENT "." IDENT ( "==" | "!=" | "<" | ">" | "<="
                  | ">=" ) expression ;

import_util_expr = "import_table" "(" STRING_LIT [ "," STRING_LIT ] ")"
                 | "import_raw" "(" STRING_LIT ")" ;

ref_expr        = "ref" "(" IDENTIFIER_LIT ")" ;

lambda_expr     = lambda_params "=>" ( expression | block_expr ) ;
lambda_params   = IDENT
                | "(" [ IDENT { "," IDENT } ] ")" ;
block_expr      = "{" { let_binding } expression "}" ;

(* ===== Collections ===== *)
list_literal    = "[" [ expression { "," expression } [ "," ] ] "]" ;
map_literal     = "{" [ map_entry { map_entry } ] "}" ;
map_entry       = ( IDENT | STRING_LIT ) "=" expression ;

(* ===== Trivia ===== *)
trivia          = { whitespace | comment } ;
comment         = line_comment | block_comment | doc_comment ;
line_comment    = "//" { any_char } newline ;
block_comment   = "/*" { any_char | block_comment } "*/" ;
doc_comment     = "///" { any_char } newline ;

(* ===== Terminals ===== *)
IDENT           = ( letter | "_" ) { letter | digit | "_" } ;
IDENTIFIER_LIT  = ( letter | "_" ) { letter | digit | "_" | "-" } ;
STRING_LIT      = '"' { string_char | escape_seq | interpolation } '"'
                | heredoc ;
INT_LIT         = digit { digit | "_" }
                | "0x" hex_digit { hex_digit | "_" }
                | "0o" oct_digit { oct_digit | "_" }
                | "0b" bin_digit { bin_digit | "_" } ;
FLOAT_LIT       = digit { digit } "." digit { digit }
                  [ ( "e" | "E" ) [ "+" | "-" ] digit { digit } ] ;
BOOL_LIT        = "true" | "false" ;
NULL_LIT        = "null" ;
interpolation   = "${" expression "}" ;
escape_seq      = "\\" ( '"' | "\\" | "n" | "r" | "t"
                | "u" hex4 | "U" hex8 ) ;
heredoc         = "<<" [ "-" ] marker newline { any_char } newline marker
                | "<<'" marker "'" newline { any_char } newline marker ;
```
