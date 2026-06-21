# WCL — concepts

Each concept has its own page. This is the index.

- [**Numbers**](../references/concept_numbers.md) — Fixed-width signed and unsigned integers plus two float widths, with literal suffixes.

- [**Strings**](../references/concept_strings.md) — UTF-8 by default, with encoding prefixes, heredocs, and opt-in interpolation.

- [**Booleans**](../references/concept_booleans.md) — The bool type with exactly two values, true and false.

- [**Symbols**](../references/concept_symbols.md) — Identifier-like :name values for tags and enum-like choices, plus symbol_set vocabularies.

- [**Identifiers**](../references/concept_identifiers.md) — The naming rule for fields, types, kinds, labels, and bindings, plus reserved words.

- [**Lists**](../references/concept_lists.md) — Ordered, homogeneous sequences — list<T> — and the collection builtins over them.

- [**Tables**](../references/concept_tables.md) — Pipe-row syntax for writing many records of the same shape compactly.

- [**Tensors**](../references/concept_tensors.md) — Shape-carrying N-dimensional arrays — tensor<T, \[dims...\]>.

- [**Optionals**](../references/concept_optionals.md) — Values that may be present or absent — the none literal and the ? type suffix.

- [**Records**](../references/concept_records.md) — Named records via the type keyword — fixed sets of named, typed fields.

- [**Unions**](../references/concept_unions.md) — Tagged variant sets — a value that is exactly one of several alternatives.

- [**Interfaces**](../references/concept_interfaces.md) — Structural contracts — a set of fields a type must have, satisfied automatically.

- [**References**](../references/concept_references.md) — &T fields that accept any value structurally satisfying an interface.

- [**Namespaces**](../references/concept_namespaces.md) — Scope declarations under a dotted path; use and :: control how names resolve.

- [**Imports & Modules**](../references/concept_imports.md) — Pull another file's declarations into the document — disk and system forms.

- [**Fields & Blocks**](../references/concept_fields_blocks.md) — The structural backbone — fields bind names to values, blocks group and nest them.

- [**Comments**](../references/concept_comments.md) — Line comments with `//` or `#`, preserved across fmt and edits.

- [**Expressions**](../references/concept_expressions.md) — The expression grammar — operators, member access, calls, and constructor forms.

- [**Control Flow**](../references/concept_control_flow.md) — if/else, match with patterns, if let, let bindings, blocks, and try/catch.

- [**Functions**](../references/concept_functions.md) — First-class function values — literals, fn items, higher-order, and function types.

- [**Schema & Decorators**](../references/concept_schema.md) — Decorators that describe a document's legal structure, validated by wcl check.

- [**Connections**](../references/concept_connections.md) — Typed relationships between block instances, populated by arrow statements.

- [**CLI Reference**](../references/concept_cli.md) — The wcl binary: parse, check, eval, set, fmt, repl, lsp, init, wdoc, and diff.
