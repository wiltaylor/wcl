---
status: accepted
---

# Decorators are checked in every position

A decorator used to be inert everywhere: the language parsed it, a consumer read
it back, and nothing checked that the name meant anything. We are making an
undeclared decorator an error — and doing so in **all eleven positions** it can be
written, on schema declarations as much as on document data, rather than only on
block instances where the request originated.

## Considered options

The narrow option was to check only the data side (`block` and `field`
positions). It has a real principle behind it — data is validated against a
schema, whereas a decorator on a declaration is metadata about the schema itself,
and WCL has no schema-for-schemas — and it has a much smaller migration. It was
rejected because the resulting line is unholdable in a user's head: `@dve` on a
block would be an error while `@dve` on the type declaring that block would not,
same document, same typo, different answer, for a reason nobody can state at the
point of confusion.

## Consequences

**It ends "arbitrary undeclared metadata on a declaration" as a feature.**
`examples/types.wcl` demonstrated hanging `@deprecated`, `@validate`, `@ui`,
`@ui_default` and `@internal` on a type with nothing declared, read back through
the `decorator_names` / `decorator_arg` reflection builtins. Those builtins keep
working — they read declared decorators exactly as well — but the decorator now
needs a declaration first. In exchange it gains a doc comment, typed slots, LSP
hover and go-to-def, where before it was a string nobody could find.

**It obliged us to complete the builtin registry.** `@doc`, `@min`, `@max`,
`@non_empty`, `@ref`, `@by_ref`, `@dynamic` and `@unit` had no decorator schema;
several were matched by bare string comparison in `schema_check.rs`. They are now
synthesised alongside the twelve that already were, which is a net simplification
independent of this decision.

**The escape hatch is `@schemaless(annotations = true)`**, which exempts a node's
annotations while leaving its fields and children fully checked. Bare
`@schemaless` also exempts annotations, but on a block it early-returns from
every check inside — so the narrow form exists precisely so that keeping one
annotation does not cost you validation of the whole block.

**The value is asymmetric, and that is accepted.** On the data side this catches
misspelled and misplaced annotations. On the schema side, with applicability
unstated by default (ADR-0002), the only thing enforced is that the name is
declared. That is the price of one rule with no position exemptions.
