---
status: accepted
---

# Applicability is declared on the decorator, and defaults to everywhere

Where a decorator is legal — which of the eleven positions, and for the `block`
position which kinds — is declared on the **decorator** with `@applies_to`, not on
the thing being decorated. When `@applies_to` is absent the decorator is legal
everywhere.

## Considered options

The obvious alternative was target-side: `@decorators(["dev"])` on
`@block("vm")`, mirroring how `@child` / `@children` already declare legal
children on the parent's type. It keeps a block type the single place you look to
learn what is legal on it.

It was rejected because it cannot express most of the vocabulary. Under ADR-0001
decorators are checked in all eleven positions, and only one of them — `block` —
has a declaration to hang a list on. A type *field* is a line in a type, not a
declaration of its own; a type *declaration* has no declaration-of-declarations
above it. So `@inline`, `@block`, `@doc`, `@unit`, `@min` and `@max` would have
nowhere to state their applicability, and we would end up declaring the same
concept in two places by two mechanisms.

Applicability lives in its own decorator rather than as slots on `@decorator`
because it is a separate concern with its own vocabulary. Cardinality does not:
`repeatable` is a slot on `@decorator`, because a decorator with no applicability
restriction must still be able to state that it repeats — `@unit` is written ten
times on one alias and constrains nothing about where it may appear.

## Consequences

**A library cannot declare a decorator for its importer's kinds.** `kinds`
entries must resolve to a block kind declared or imported in the document, so a
decorator constrained to `vm` genuinely depends on `vm`. The alternative — free
strings matched textually — makes a typo (`kinds = ["vmm"]`) fire an error at
every *correct* use site (`@dev is not applicable to vm`), pointing at the
innocent party. A library that wants to ship a decorator for kinds it cannot see
simply omits `@applies_to`.

**Defaulting to "everywhere" is what makes the feature incremental.** The
migration needs no applicability at all: the existing declarations
(`@only`, `@except`, `@native`, `@answerable`, `@wdoc.file`, `@wdoc.editable`)
need no edit, and the newly-synthesised builtins need only a name. Name-checking
lands and delivers on its own; applicability arrives per decorator, as authors
want it, with no flag day. Defaulting to "nowhere" would make `@applies_to`
mandatory in practice — which would have argued for slots on `@decorator` after
all.

**Derived kinds are addressable and do not inherit.** `kinds = ["card"]`
resolves through `block_schema`'s `derived_block_schema` fallback, because a
component instance is an ordinary block. Applicability to a `@declares_kind`
declarer does *not* propagate to the kinds its instances declare: they are
different kinds with different schemas, and propagation would make applicability
unreadable without enumerating the whole tree.
