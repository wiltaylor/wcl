# Connections

_Typed relationships between block instances, populated by arrow statements._

A `connection` declaration defines a typed relationship between block instances; arrow statements then populate it. The result is a list of records that hosts can consume — render edges, build dependency graphs, validate references.

## Declaring a connection

A `connection` names a relationship's source type, destination type, and optionally a tag drawn from a `symbol_set`.

```wcl
symbol_set EdgeKind { uses  depends_on }
connection DependsOn: Service -> Service : EdgeKind
```

## Connection statements

Inside a `@connections(SchemaName)` field, write `source -> destination :tag` to populate it. The tag is optional; omit the `:kind` for an untagged edge.

```wcl
@document
type Config {
  @connections(DependsOn) edges: list<DependsOn>
}

web   -> db                   // untagged
web   -> cache :uses
api   -> db    :depends_on
```

Each statement produces a record with `source`, `destination`, and `kind` slots, ready for a host to interpret.

## Polymorphic endpoints

An endpoint type need not be a single concrete block type. A connection matches a statement when each operand's concrete type satisfies the endpoint: a nominal match (the exact type, or a subtype via `extends`), an **interface endpoint** (`&Iface`) the operand implements, or a **union endpoint** the operand is a variant member of. See [References](../references/concept_references.md).

```wcl
interface Entity { name: utf8 }
symbol_set RelKind { implements }
connection Rel: &Entity -> &Entity : RelKind   // any Entity -> any Entity
```

## Dynamic endpoints

By default every operand must name a literal block in scope; an operand resolving to nothing is a schema error. Tag the declaration with `@dynamic` to relax that: an unresolved operand is projected as its raw id string instead of being dropped, and `wcl check` no longer flags it. This is for endpoints a host materialises at consume time.

```wcl
symbol_set EdgeKind { uses }
@dynamic
connection DependsOn: Service -> Service : EdgeKind
```

A still-unmatched id is the host's responsibility. Leave `@dynamic` off for connections whose endpoints are always literal, so typos stay caught.

## Related

- [References](../references/concept_references.md)

- [Symbols](../references/concept_symbols.md)

[← Back to SKILL.md](../SKILL.md)
