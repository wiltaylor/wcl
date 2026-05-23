# tree-sitter-wcl

A tree-sitter grammar for WCL, targeted at editor highlighting and outlines (not a full re-implementation of the Rust parser).

## Build

```bash
cd editors/tree-sitter-wcl
npm install            # installs tree-sitter-cli
npm run generate       # → src/parser.c
npm test               # runs the corpus tests under corpus/
```

After `npm run generate`, point your editor (Helix, Neovim with `nvim-treesitter`, Zed, …) at this directory as a grammar source. Most editors want the grammar repo path + a query file (highlights, locals, etc.) which this stub doesn't ship yet.

## Coverage

- Declarations: `type`, `interface`, `union`, `symbol_set`, `connection`, `namespace`, `use`, `import`.
- Decorators with positional and named arguments.
- Blocks with labels (identifier / string / number / symbol).
- Fields, type refs (`list<T>`, `[T]`, `fn(...) -> T`, references with `&`).
- Expressions: literals, identifiers, member access, calls, unary / binary operators, parentheses, lists.
- Strings: plain, encoded (`utf8` / `ascii` / `utf16` / `utf32`), interpolated (`$"...${expr}..."`).
- Numbers with all integer / float suffixes; hex / binary / octal bases.
- Control flow: `if` / `else`, `match`, block expressions, `let` bindings, function literals.

## Status

Contributor stub — not published to npm or to a tree-sitter index. Mostly here so editors that consume tree-sitter grammars have something to point at while we iterate on the language.
