; Injection queries for WCL
; Used by Neovim and other tree-sitter hosts for embedded language support.

; Interpolation expressions inside strings get WCL highlighting
(interpolation
  (expression) @injection.content
  (#set! injection.language "wcl"))
