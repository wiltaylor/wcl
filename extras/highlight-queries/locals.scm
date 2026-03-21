; Local variable scoping for WCL
; Used by tree-sitter-highlight and Neovim for scope-aware highlighting.

; Scope boundaries
(block_body) @local.scope
(for_loop) @local.scope
(conditional) @local.scope
(function_macro) @local.scope
(attribute_macro) @local.scope
(lambda_expression) @local.scope

; Definitions
(let_binding
  (identifier) @local.definition)

(parameter
  (identifier) @local.definition)

(for_loop
  (identifier) @local.definition)

(lambda_parameters
  (identifier) @local.definition)

; References
(identifier) @local.reference
