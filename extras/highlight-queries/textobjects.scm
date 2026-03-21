; Text objects for WCL
; Used by Neovim (nvim-treesitter-textobjects) and Helix for structural
; selection (e.g. "around function", "inner block").

; Blocks
(block) @class.outer
(block_body) @class.inner

; Functions / macros
(function_macro) @function.outer
(function_macro
  "{" . (_)* @function.inner . "}")

(attribute_macro) @function.outer

(macro_call) @call.outer
(call_expression) @call.outer

; Parameters
(parameter) @parameter.outer
(parameter_list) @parameter.inner

; Comments
(line_comment) @comment.outer
(block_comment) @comment.outer
(doc_comment) @comment.outer

; Conditionals
(conditional) @conditional.outer
(for_loop) @loop.outer

; Attributes / assignments
(attribute) @assignment.outer
(let_binding) @assignment.outer
