# WCL

WCL is a typed configuration and schema language: a document declares data, a
schema declares what that data may be, and the language checks one against the
other. This file is the glossary — the words we use for WCL's own concepts, and
the ones we've decided not to use.

## Language

### Decorators

**Decorator**:
An annotation *about* a thing, written `@name` or `@name(args)` above or beside
it. Distinct from a block, which is a thing *inside* another thing.
_Avoid_: attribute, annotation, marker, tag

**Position**:
One of the eleven syntactic places a decorator may be written — `field`, `fn`,
`block`, `type`, `interface`, `type_field`, `union`, `variant`, `symbol_set`,
`symbol`, `connection`. A closed vocabulary derived from the grammar.
_Avoid_: target, site, location

**Instance decorator**:
A decorator in the `block` or `field` position — i.e. on data rather than on a
declaration. A description of where a decorator sits, never of whether it is
checked: every position is checked alike.
_Avoid_: using this to mean "the checked ones"

**Decorator schema**:
The type carrying `@decorator("name")`, whose fields are the argument slots of
every `@name` written anywhere.
_Avoid_: decorator type, decorator definition

**Slot**:
One field of a decorator schema — the shape of one argument. A slot marked
`@inline(N)` may be filled positionally; one without is named-only.
_Avoid_: parameter, argument (an argument is what fills a slot at a use site)

**Declared decorator**:
A decorator whose name resolves to a decorator schema. Only declared decorators
are legal; an undeclared one is an error wherever it is written.
_Avoid_: known decorator, registered decorator

**Inert decorator**:
Historical: a decorator the language parsed and a consumer read back, but
nothing checked. No longer a category — retained here only because older
comments and documents use it.

**Applicability**:
The positions, and for the `block` position the block kinds, where a decorator
is legal. Declared on the decorator itself with `@applies_to`; unstated means
legal everywhere.
_Avoid_: scope, validity, allowed targets

**Annotation exemption**:
`@schemaless(annotations = true)` — opting one node's decorators out of
checking while its fields and children stay fully checked. Bare `@schemaless`
exempts annotations too, on top of the contents exemption it already carries.
_Avoid_: schemaless decorators, decorator opt-out

### Kinds and schemas

**Kind**:
The name a block is written with (`vm`, `nic`, `page`). What a block *is*,
independent of which type schemas it.
_Avoid_: block type (that's the type, not the kind), tag

**Derived kind**:
A kind declared by a block *instance* rather than by a `@block("…")` type — see
`@declares_kind`. An ordinary kind in every respect: it validates, it can be
named by `@applies_to`, and it is absent from `type_decls()`.
_Avoid_: component kind, synthetic kind

**Document schema** / **Block schema**:
The type carrying `@document` / `@block("kind")`. Document schemas *merge* per
namespace; block schemas do not.
