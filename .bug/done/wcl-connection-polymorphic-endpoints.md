# wcl: connections need concrete endpoint types — no interface/union endpoint

**Reported by:** WAD skill implementation (2026-06-13)
**Component:** `wcl_lang` connection schema resolution
**Severity:** moderate (forces large boilerplate; blocks polymorphic links)

## Summary

A `connection` schema's endpoints are matched **nominally** against the concrete
block types of the operands. An interface endpoint (`&Iface`) parses but never
matches, and a union endpoint is rejected at resolution time:

```
connection Rel: &Entity -> &Entity : RelKind     // parses, but...
//   × no connection schema accepts 'Component -> Procedure'   (at use site)

union AnyEnt { Component Procedure }
connection Rel: AnyEnt -> AnyEnt : RelKind        // also:
//   × no connection schema accepts 'Component -> Procedure'
```

So a model that wants typed relationships across many entity kinds must declare
**one `connection` per ordered (Source, Dest) type-pair** plus a matching
`@connections` field. In WAD this is ~47 connection schemas, and any
"spec/adr → *any* entity" link must be enumerated per concrete destination type.

## Repro

```wcl
@block("component") type Component { @inline(0) id: identifier  name: utf8 }
@block("procedure") type Procedure { @inline(0) id: identifier  name: utf8 }
interface Entity { name: utf8 }
symbol_set RelKind { implements }
connection Rel: &Entity -> &Entity : RelKind
@document type M {
  @children("component") comps: list<Component>
  @children("procedure") procs: list<Procedure>
  @connections(Rel) rels: list<Rel>
}
component "auth" { name = "Auth" }
procedure "login" { name = "Login" }
auth -> login :implements        // schema_violation: no connection schema accepts 'Component -> Procedure'
```

## Expected / requested

Allow an **interface-ref** (`&Iface`) or **union** as a connection endpoint and
resolve it structurally / by variant membership: a statement matches the schema
when each operand's concrete type satisfies the endpoint type. One
`connection Rel: &Entity -> &Entity` would then span every entity pair, and
polymorphic `spec -> &Entity :implements` links would be expressible directly.

## Workaround in use

WAD generates one `connection` + `@connections` field per ordered type-pair (a
table-driven codegen in the schema) and enumerates spec/adr links to the core
entity types. Documented in the schema's `relationships.wcl`.
