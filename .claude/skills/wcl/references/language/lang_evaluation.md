# How a document evaluates

This page is the model behind the surprises: why a broken field can sit in a file that
`wcl check` calls `OK`, why a `let` you can plainly see is not in the JSON, and why a tool that
edits a file has to reopen it before reading a value back.

## Two paths, and you pick one per parse

WCL has two mutually exclusive entry points.

| | Evaluating path | Editing path |
| --- | --- | --- |
| Entry point | `Document::open` / `open_with` | `parse_for_edit` |
| You get | A lazy, evaluation-only view | An owned AST with public fields |
| Imports | Resolved | Not resolved |
| Schema checks | Run | Do not run |
| Mutation | Impossible | The point |

**There is deliberately no AST escape hatch on `Document`.** Mixing editing and evaluation in
one parse would silently invalidate the document's cached field values, so the API makes you
choose. A host that edits a file and then wants a value out of it re-opens the file as a
`Document`.

This is why `wcl fmt` and `wcl set` preserve your comments and blank-line groupings — they run
on the editing path, over a real AST — while `wcl get` and `wcl check` see only evaluated
values.

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

### The consequence worth remembering

**Validation does not force a field whose expression fails.** A field that errors during
evaluation is *skipped* by the schema check rather than reported, because a computed field may
legitimately refer to bindings that only exist once a host expands it. So:

```console
$ wcl check b.wcl
OK
```

`wcl check` type-checks every value it can evaluate — a wrong type is caught:

```console
$ wcl check c.wcl
wcl::eval::schema_violation

  × field 's' declared as utf8 but value is i64
```

— but a value it *cannot* evaluate is not an error. There is one exception: a literal list whose
element type is a union is static authored data, so a failure to infer one of its record
variants is reported.

Practical rule: `wcl check` proves the shape, not that every expression runs. Render or consume
the document to prove that.

## Cycle detection

Each field cell carries an "is being evaluated" flag. Re-entering a cell already in progress is
an error, not a hang:

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

## Scope and lookup

A bare identifier inside a block resolves by walking the enclosing block frames innermost to
outermost, then falling through to the document root. A frame carries the block's fields, its
nested blocks and its `let` bindings — plus, when a host expands a `@contextual` block, the
bindings that expansion injected (a loop variable, a component's slot values).

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
composition helper: sibling and descendant expressions resolve it by name, but it is absent from
`fields`, from `blocks`, from `get`, from JSON and from schema validation. It is not in the
symbol index either.

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

Two more things a path lookup will not do:

- **It must end at a leaf.** `wcl get config.wcl servers` fails with `not_a_leaf`, because a
  gathered block list is not a scalar.
- **It walks a block list by label, not by index.** `servers.web.host` works; `servers.0.host`
  does not.

## JSON serialization

Values serialize **one way only**. There is a custom `Serialize` impl and deliberately no
`Deserialize`: round-tripping JSON back into a value would lose the numeric variant (`i32` vs
`i64` vs `u32`), which the evaluator assumes is preserved.

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

Three families, all carrying a span and a `NamedSource`, all rendered by `miette` with a
snippet and a caret.

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

**`EvalError`** — everything that happens while a value is produced. Its diagnostic codes are
what you match on: `wcl::eval::cycle`, `wcl::eval::unresolved_reference`,
`wcl::eval::unknown_builtin`, `wcl::eval::builtin_arity`, `wcl::eval::user_error`,
`wcl::eval::import_failed`, `wcl::eval::not_a_leaf`, and `wcl::eval::schema_violation`.

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
- Advisory: `DocumentFieldShadow` — a **warning**, never an error. See
  [`lang_schemas.md`](lang_schemas.md).

Warnings go to stderr and never change the exit code:

```console
$ wcl check n.wcl
warning: gather field 'widgets' … silently vanish; rename one field
n.wcl: 1 warning
OK
```

`wcl check --json` emits errors and warnings as structured data instead.

## Gotchas

- `wcl check` says `OK` for `n = error("boom")`. An evaluation failure during validation is
  skipped, not reported.
- A `let` is not in the evaluated document. If you want it in the output, make it a field.
- Editing and evaluating are separate parses. Re-open a file after writing to it.
- A `get` path must end at a leaf and must address blocks by label.
- A symbol and an identifier both serialize as plain strings. The `:` is syntax.
- An imported file's top-level blocks are part of *your* document — including its fields, which
  share the merged `@document` field-name space.
