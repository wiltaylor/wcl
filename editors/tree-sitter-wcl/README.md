# tree-sitter-wcl

A tree-sitter grammar for WCL, for editor highlighting, folding and outlines. It tracks the Rust parser in `crates/wcl_lang/src/parser` and parses every `.wcl` file under `examples/` and `docs/` without error nodes, apart from the deliberate syntax errors in `examples/errors/`.

## Build and test

Requires `tree-sitter` CLI 0.26 and a C compiler.

```bash
cd editors/tree-sitter-wcl
tree-sitter generate   # grammar.js → src/parser.c (not committed)
tree-sitter test       # test/corpus/*.txt and test/highlight/*.wcl
```

Install the CLI with `cargo install tree-sitter-cli --version '~0.26'` or `npm install` (it is the package's dev dependency).

To check a file: `tree-sitter parse path/to/file.wcl`.

## Layout

- `grammar.js` — the grammar.
- `src/scanner.c` — external scanner for the two constructs the Rust lexer decides from raw bytes: heredoc bodies (`<<TAG`, `<<'TAG'`, `$<<TAG`, `ascii<<TAG`) and the end of a body-less block, which ends at a line break (`hr` on its own line).
- `queries/highlights.scm` — highlight captures in the common nvim-treesitter / Helix / Zed vocabulary.
- `test/corpus/` — parse tests, one file per area, using snippets from `examples/`.
- `test/highlight/` — highlight assertions.

## Coverage

- Items: fields, blocks (labels, `?` conditional, `ns::kind` qualified kinds, body-less form), tables with `|` rows, `let` and `fn` items, `slot` declarations, connection statements (`a -> b :kind`), `@schemaless` string keys.
- Declarations: `type` (body or `= alias`), `interface`, `union` (record, type, `&Interface` and `none` variants), `symbol_set`, `connection`, `namespace`, `use` (alias and `.{…}` list forms), `import "path"` and `import <path>`, `extends`.
- Decorators with positional and named arguments.
- Types: named and dotted, generic `T<…>`, `&T`, `fn(…) -> T`, `tensor<T, [dims]>`, optional `?` and repeated `*` markers.
- Expressions: every binary operator including `??`, unary `-` / `!`, member access (including numeric segments), calls, lists, records, block expressions with `let` bindings, `if` / `else if` / `if let`, `match` with guards and alternation, `try … catch`, function literals, variant construction, `parent` and `self`.
- Patterns: wildcard, bindings, `name @ pattern`, qualified and unqualified variants, record patterns with `..`, literals.
- Literals: numbers in every base with type suffixes and literal units (`5MiB`, `30s`), booleans, `none`, symbols, plain and encoded strings, interpolated strings with `${…}` parsed as expressions, heredocs.

Contextual keywords behave as in the Rust parser: `type = 1` is a field, `slot "x" {}` is a block.

## Known differences from the Rust parser

- `-3` is a unary minus applied to `3`; the Rust lexer folds a tightly written sign into the number. The tree differs, the highlighting does not.
- `${…}` slots inside heredocs are not parsed; the heredoc is one token.
