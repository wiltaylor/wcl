# How a document evaluates

This page is the model behind three surprises. Why does a broken field sit in a file that
`wcl check` calls `OK`? Why is a `let` you can plainly see absent from the JSON? Why must a
tool that edits a file re-open it before it reads a value back?

## Two paths, and you pick one per parse

WCL has two mutually exclusive entry points.

| | Evaluating path | Editing path |
| --- | --- | --- |
| Entry point | `Document::open` / `open_with` | `parse_for_edit` |
| You get | A lazy, evaluation-only view | An owned AST with public fields |
| Imports | Resolved | Not resolved |
| Schema checks | Run | Do not run |
| Mutation | Impossible | The point |

**There is deliberately no AST escape hatch on `Document`.** One parse that both edits and
evaluates would silently invalidate the cached field values. The API therefore makes you
choose. A host that edits a file and then wants a value out of it re-opens it as a `Document`.

This is why `wcl fmt` and `wcl set` preserve your comments and blank-line groupings. They run
on the editing path, over a real AST. `wcl get` and `wcl check` see only evaluated values.

Two library calls cross the paths for you, and are what the CLI itself runs:

| Call | Does | CLI |
| --- | --- | --- |
| `wcl_lang::edit::set_field(source, name, path, value)` | Opens `source` as a `Document`, finds the field at the dotted `path`, parses `value` as an expression, returns the reprinted source. Refuses a field from an import (`EditError::Imported`) | — |
| `edit::locate_field(&doc, path)` then `edit::replace_field(source, name, span, expr)` | The same in two halves: `locate_field` follows imports and returns the declaring file and span, `replace_field` rewrites that file's source | `wcl set` |
| `wcl_lang::diff::diff_documents(&old, &new)` | Compares evaluated documents. Returns `Diff { changes, warnings }`; `warnings` lists blocks and fields that failed to evaluate and were left out | `wcl diff` |

`replace_field` re-parses its own output and returns `EditError::Unprintable` rather than
text that does not parse. Like `wcl fmt`, it reprints the whole file, not only the edited line.

## Fields evaluate lazily, and cache

A field's expression runs the first time something asks for its value, and the result is cached
for the life of the document. Nothing forces a field you never read.

```wcl
@document type Doc {
  good: i64?
  bad:  i64?
}

good = 1
bad  = error("boom")
```

```console
$ wcl get b.wcl good
1
$ wcl get b.wcl bad
wcl::eval::user_error

  × error: boom
```

### Validation skips a field that fails to evaluate

**The schema check *skips* a field that errors during evaluation, instead of reporting it.** A
computed field may legitimately refer to bindings that only exist once a host expands it. So:

```console
$ wcl check b.wcl
OK
```

`wcl check` type-checks every value it can evaluate — a wrong type is caught:

```console
$ wcl check c.wcl
wcl::eval::schema_violation

  × field 's' declared as utf8 but value is i64
   ╭─[c.wcl:3:1]
 2 │
 3 │ s = 1
   · ──┬──
   ·   ╰── schema violation
   ╰────

c.wcl: 1 schema violation
```

— but a value it *cannot* evaluate is not an error. There is one exception. A literal list whose
element type is a union is static authored data, so a failure to infer one of its record
variants is reported.

Practical rule: `wcl check` proves the shape, not that every expression runs. Render or consume
the document to prove that.

## Cycle detection

Re-entering a field or `let` that is already being evaluated is an error, not a hang:

```wcl
@document type Doc { a: i64?  b: i64? }
a = b + 1
b = a + 1
```

```console
$ wcl get a.wcl a
wcl::eval::cycle

  × cycle while evaluating 'a'
```

Unions have their own cycle check (`union_cycle`) for a variant chain that refers back to
itself.

A cycle's message names the binding where the loop closed, which depends on which field was read
first. Reading `b` first in the file above reports `'b'`.

A `Document` may be read from several threads at once. A field another thread is evaluating is
not a cycle. The second thread computes the value itself.

## Depth limit

Evaluation nests at most **200** levels, counting fields, `let`s and `fn` calls together. A long
chain that never loops still fails once it goes deeper than that:

```console
$ wcl get chain.wcl cfg.a0        # cfg { a0 = a1 + 1 ... a4999 = a5000 + 1  a5000 = 0 }
wcl::eval::depth_exceeded

  × evaluation depth limit exceeded (max 200)
$ wcl get chain.wcl cfg.a4850     # 150 links: within the limit
150
```

A call past the limit reports `wcl::eval::call_depth_exceeded` (`call depth limit exceeded (max
200)`) instead. The limit is on nesting, not document size. Fix it by shortening the chain, for
example by computing a total with `sum(...)` over a list rather than by field-to-field
accumulation.

The same two errors also fire **before** 200 when the thread's real stack runs low: a small
thread (the LSP's 2 MiB workers), an unoptimised build, or a `fn` whose body is a very deep
expression recursing close to the cap. Evaluation never overflows the stack; it reports the
depth error.

## Scope and lookup

A bare identifier inside a block resolves by walking the enclosing block frames innermost to
outermost, then falling through to the document root. A frame carries the block's fields, its
nested blocks and its `let` bindings. When a host expands a `@contextual` block, the frame also
carries the bindings that expansion injected — a loop variable, a component's slot values.

At the root, the search covers the root file **and every eagerly-imported file**, which is what
makes an imported declaration usable as if local.

`self` and `parent` address frames explicitly rather than by name.

## What the document view exposes — and hides

`Document::fields()` and `Document::blocks()` (and the CLI paths over them) iterate the root
file **plus every eagerly-imported file**. A block written in an imported file is a top-level
block of the document:

`part.wcl`:

```wcl
@block("server") type Server { @inline(0) id: identifier  host: utf8 }
server extra { host = "from-import" }
```

`root.wcl`:

```wcl
import "./part.wcl"
@document type Cfg { @children("server") servers: list<Server> }
server main { host = "root" }
```

```console
$ wcl get root.wcl servers.extra.host
"from-import"
$ wcl get root.wcl servers.main.host
"root"
```

**`let` items are invisible to the document view.** A top-level `let name = expr` is a
composition helper. Sibling and descendant expressions resolve it by name. It is absent from
`fields`, from `blocks`, from `get`, from JSON and from schema validation, and it is not in the
symbol index.

```wcl
let base = 10

@document type Doc { total: i64? }

total = base * 2
```

```console
$ wcl get e.wcl total
20
$ wcl get e.wcl base
no such path: base
```

`wcl fmt` still prints the `let` — it is source, and the editing path sees everything.

A `let` **item** at file or block scope is not the same construct as a `let … ;` **binding**
inside a `{ }` block expression. See [`lang_control_flow.md`](lang_control_flow.md).

Two more rules bind a path lookup:

- **It must end at a leaf.** `wcl get config.wcl servers` fails with `not_a_leaf`, because a
  gathered block list is not a scalar.
- **It walks a block list by label, not by index.** `servers.web.host` works; `servers.0.host`
  does not.

## JSON serialization

Values serialize **one way only**. There is a custom `Serialize` impl and deliberately no
`Deserialize`. Round-tripping JSON back into a value would lose the numeric variant — `i32` vs
`i64` vs `u32` — which the evaluator assumes is preserved.

| Value | JSON |
| --- | --- |
| Every integer and float width | A JSON number |
| `utf8` / `ascii` / `utf16` / `utf32` | A string |
| `identifier`, `symbol` | A string — **the colon and the quotes are syntax, not content** |
| `bool` | `true` / `false` |
| `none`, a function value | `null` |
| A list | An array |
| A record | An object of its fields |
| A tensor | `{ "shape": [...], "data": [...] }` |
| A unit variant | The variant name as a string |
| A payload variant | `{ "<Variant>": <payload> }` |
| A reference that stayed a handle | `{ "kind": "...", "path": [...] }` |
| An unresolved literal unit | An error — a unit literal needs a declared type |

```console
$ wcl get config.wcl servers.web.port --json
8080
```

## The error model

Three families, all carrying a span, all rendered by `miette` — with a snippet and a caret when
the source is known (below).

**`ParseError`** — lexing and parsing, plus the checks that run when a document opens: import
resolution, import cycles, `use` targets, duplicate aliases.

```console
wcl::parse

  × namespace declaration must be the first item in the file
   ╭─[a.wcl:3:1]
 3 │ namespace company
   · ────────┬────────
   ·         ╰── must be first item
```

A syntax error does not stop the parser: it skips to the next item (the next line starting one
at the same nesting level, or the `}` closing the enclosing block) and carries on, so one
`ParseError` carries every syntax error in the file (at most 100). The first is the error; the
rest are `SyntaxError::others`, rendered after it as related diagnostics, and
`ParseError::syntax_errors()` walks them all. The file still does not open.
`parse_for_edit_recovering` returns the partial tree, its symbols and the errors instead.

**`EvalError`** — everything that happens while a value is produced. Its diagnostic codes are
what you match on. There are 27 in all; these are the ones you will actually meet:
`wcl::eval::cycle`, `wcl::eval::depth_exceeded`, `wcl::eval::call_depth_exceeded`,
`wcl::eval::unresolved_reference`, `wcl::eval::type_mismatch`,
`wcl::eval::unknown_builtin`, `wcl::eval::builtin_arity`, `wcl::eval::user_error`,
`wcl::eval::import_failed`, `wcl::eval::not_a_leaf`, `wcl::eval::missing_expander`,
`wcl::eval::expansion_limit`, and
`wcl::eval::schema_violation`.

**`SchemaViolationKind`** — the classification carried inside a `schema_violation`. Knowing the
names helps you read a message and search for its cause:

- Structure: `NoDocumentSchema`, `MultipleDocumentSchemas`, `UnknownField`, `UnregisteredKind`,
  `DisallowedChild`, `MissingRequired`, `ChildrenTooFew`, `ChildrenTooMany`,
  `BlockChildrenOverflow`, `UnexpectedExtraChild`, `DuplicateBlockKind`, `DuplicateBlockId`,
  `DeclaredKindCollision`.
- Types and values: `FieldTypeMismatch`, `ConstraintViolation`, `SymbolNotInSet`,
  `InterfaceNotImplemented`, `VariantUnionMismatch`, `VariantNoMatch`, `VariantAmbiguous`,
  `DuplicateVariant`, `VariantShapeCollision`.
- Decorators: `UndeclaredDecorator`, `DecoratorNotApplicable`, `DecoratorCardinality`,
  `InvalidDecoratorApplicability`.
- Graphs: `UnknownConnectionOperand`, `UnknownConnection`, `AmbiguousConnection`,
  `UnknownConnectionKind`, `DanglingReference`.
- Advisory: `DocumentFieldShadow` — a **warning**, never an error. It is the only one, and
  [`lang_schemas.md`](lang_schemas.md) carries the transcript and the fix.

**Which file a span counts into.** A `ParseError` carries its source. So does every
`schema_violation`, strict (`schema_errors` / `schema_diagnostics`) or lazy (a field read): the
file holding the offending text — root, imported file, or a file an in-block `import` spliced
in. `EvalError::schema_source()` returns it, and `miette::Report::new(err)` renders the snippet
against it with no source attached by the host. `schema_diagnostics()` pairs each error with the
same source (`None` only for a library-synthesised declaration). Every *other* `EvalError`
carries a span only; the host supplies the text. `wcl check --json` exposes the file per error
(see [`lang_cli.md`](lang_cli.md)).

## Gotchas

- `wcl parse` prints a failed value as `<error: …>` *inside* an otherwise normal-looking tree.
  The diagnostics are on stderr and the exit code is 3; a script reading only stdout sees a
  complete tree. Values in template bodies (repeater children, a component's `wdoc_body`)
  print as `<deferred: …>` and are not errors.
- Every syntax error is reported in one run, but only one per broken item: the parser skips
  the rest of an item after its first mistake, so `x = [1 2, 3 4]` is one error. Fix and
  re-run. A missing `}` is reported at the **end of the file**, not where the brace belongs —
  everything after it parsed as the block's body.
- `wcl check` says `OK` for `n = error("boom")`. An evaluation failure during validation is
  skipped, not reported.
- A `let` is not in the evaluated document. If you want it in the output, make it a field.
- Editing and evaluating are separate parses. Re-open a file after writing to it.
- A `get` path must end at a leaf and must address blocks by label.
- A symbol and an identifier both serialize as plain strings. The `:` is syntax.
- An imported file's top-level blocks are part of *your* document — including its fields, which
  share the merged `@document` field-name space.
