# Canonical Examples

Pointers to idiomatic WCL and wdoc in the repo. Read the source when you need a pattern you don't see in the other reference files — these compile and pass tests.

## WCL configuration

| File | Demonstrates |
|------|--------------|
| `examples/config/app.wcl` | Let bindings, schemas, blocks with inline IDs, if/else, ternary, for loops, tables, validation, `@deprecated` |
| `examples/transforms/` | Transform definitions for `wcl transform run` |

## wdoc documentation

| File | Demonstrates |
|------|--------------|
| `examples/wdoc/site.wcl` | `doc` block with outline, global config |
| `examples/wdoc/pages.wcl` | Pages with content and diagrams |

## Authoring guides (themselves written in wdoc)

`docs/guide-*.wcl` are the canonical source for how to write each language feature:

| File | Topic |
|------|-------|
| `guide-basic-syntax.wcl` | Blocks, attributes, literals |
| `guide-blocks.wcl` | Block patterns, inline args, inline IDs |
| `guide-data-types.wcl` | All literal types |
| `guide-expressions.wcl` | Operators, function calls, indexing, queries |
| `guide-variables-and-scoping.wcl` | `let`, partial `let`, export |
| `guide-namespaces.wcl` | `namespace`, `use`, aliasing |
| `guide-imports.wcl` | Relative, library, optional, glob, lazy, `import_raw`, `import_table` |
| `guide-schemas.wcl` | Schema declaration, decorators, constraints |
| `guide-decorators-builtin.wcl` | Every built-in decorator |
| `guide-control-flow.wcl` | If / else, ternary |
| `guide-for-loops.wcl` | For loops, ranges, query iteration |
| `guide-macros.wcl`, `guide-macros-function.wcl`, `guide-macros-attribute.wcl` | Macros |
| `guide-functions-ref-query.wcl` | Ref, queries, selectors |
| `guide-tables.wcl` | Tables in all forms |
| `guide-validation-blocks.wcl` | `validation { ... }` |
| `guide-partials.wcl` | Partial blocks |

## wdoc drawings

| File | Topic |
|------|-------|
| `docs/wdoc-overview.wcl` | wdoc quick start |
| `docs/wdoc-content.wcl` | Content elements |
| `docs/wdoc-styling.wcl` | Styles |
| `docs/wdoc-drawing-overview.wcl` | Drawings intro |
| `docs/wdoc-drawing-shapes.wcl` | Primitive shapes |
| `docs/wdoc-drawing-connections.wcl` | Connections |
| `docs/wdoc-drawing-layouts.wcl` | Layout algorithms |
| `docs/wdoc-drawing-custom-shapes.wcl` | Custom-shape patterns |
| `docs/wdoc-drawing-widgets.wcl` | All widgets |
| `docs/wdoc-example-flowchart.wcl` | Worked flowchart |
| `docs/wdoc-example-swimlane.wcl` | Swimlane diagram |
| `docs/wdoc-example-wireframe.wcl` | UI wireframe |

## Consumer / library guides

Walkthroughs for using WCL from each host language — a natural complement to `reference/bindings/*.md`:

| File | Binding |
|------|---------|
| `docs/getting-started-rust-library.wcl` | Rust |
| `docs/getting-started-python-library.wcl` | Python |
| `docs/getting-started-js-library.wcl` | JavaScript |
| `docs/getting-started-go-library.wcl` | Go |
| `docs/getting-started-dotnet-library.wcl` | .NET |
| `docs/getting-started-jvm-library.wcl` | JVM |
| `docs/getting-started-ruby-library.wcl` | Ruby |
| `docs/getting-started-c-library.wcl` | C/C++ |
| `docs/getting-started-zig-library.wcl` | Zig |

## CLI walkthroughs

| File | Command |
|------|---------|
| `docs/cli-overview.wcl` | `wcl --help` |
| `docs/cli-validate.wcl`, `cli-fmt.wcl`, `cli-eval.wcl`, `cli-lsp.wcl`, `cli-set.wcl`, `cli-add.wcl`, `cli-remove.wcl`, `cli-table.wcl`, `cli-docs.wcl`, `cli-transform.wcl` | Per-command guide |

## Appendix

| File | Topic |
|------|-------|
| `docs/appendix-error-codes.wcl` | All E/W codes (authoritative) |
| `docs/appendix-ebnf.wcl` | EBNF grammar |
| `docs/appendix-comparison.wcl` | Comparison with other config languages |
