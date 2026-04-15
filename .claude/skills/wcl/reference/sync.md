# Skill Sync Map

When source files change, update the listed reference file(s) so the skill stays accurate. This is the authoritative map used by `CLAUDE.md`'s Change Checklist.

## Language core

| Source | Reference to update |
|--------|---------------------|
| `crates/wcl_lang/src/schema/decorator.rs` (`register_builtins`) | `reference/schemas-and-decorators.md` |
| `crates/wcl_lang/src/eval/functions.rs` (`builtin_registry`) | `reference/builtin-functions.md` |
| `crates/wcl_lang/src/lang/ast.rs` / `lang/parser/mod.rs` | `reference/syntax.md` |
| `crates/wcl_lang/src/schema/schema.rs`, `schema/types.rs`, `schema/document.rs` | `reference/schemas-and-decorators.md`, `reference/error-codes.md` |
| `docs/appendix-error-codes.wcl` | `reference/error-codes.md` |

## CLI

| Source | Reference to update |
|--------|---------------------|
| `crates/wcl/src/cli/mod.rs` (clap `Commands` enum) | `reference/cli.md` |
| `crates/wcl/src/cli/{validate,fmt,eval,set,add,remove,table,docs,transform,wdoc}.rs` | `reference/cli.md` |

## wdoc and drawings

| Source | Reference to update |
|--------|---------------------|
| `crates/wcl_wdoc/src/wdoc.wcl` (schemas + widget templates) | `reference/wdoc.md`, `reference/wdoc-drawings.md` |
| `crates/wcl_wdoc/src/model.rs` | `reference/wdoc.md` |
| `crates/wcl_wdoc/src/shapes.rs` (ShapeKind, Alignment, Connection) | `reference/wdoc-drawings.md` |
| `crates/wcl_wdoc/src/graph_layout.rs` | `reference/wdoc-drawings.md` |
| `crates/wcl/src/cli/wdoc.rs` (template dispatch) | `reference/wdoc.md`, `reference/cli.md` |

## Bindings

| Source | Reference to update |
|--------|---------------------|
| `bindings/python/**` | `reference/bindings/python.md` |
| `bindings/wasm/**` (includes TypeScript types) | `reference/bindings/javascript.md` |
| `bindings/go/**` | `reference/bindings/go.md` |
| `bindings/dotnet/**` | `reference/bindings/dotnet.md` |
| `bindings/jvm/**` | `reference/bindings/jvm.md` |
| `bindings/ruby/**` | `reference/bindings/ruby.md` |
| `bindings/zig/**` | `reference/bindings/zig.md` |
| `crates/wcl_ffi/**` (`wcl.h`, exported C API) | `reference/bindings/c.md` |
| `crates/wcl_lang/src/lib.rs` (public Rust API) | `reference/bindings/rust.md` |

## Examples

| Source | Reference to update |
|--------|---------------------|
| `examples/**` | `reference/examples.md` (if new top-level examples added) |
| `docs/guide-*.wcl`, `docs/wdoc-*.wcl` | `reference/examples.md` |

## SKILL.md

Update `SKILL.md` only when the routing table changes — e.g. a new reference file is added, or the binding set grows/shrinks.
