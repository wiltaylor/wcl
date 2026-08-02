# @declares_kind

`@declares_kind` on a `@block` type says that **instances** of that type declare block kinds
of their own. It is how a host adds a template-like concept — a component, a widget, a macro —
without the language knowing that concept's name.


This is **new ground**, and worth knowing as such. Every other decorator a consumer applies to
its own types — `@only`, `@except`, `@wdoc.file`, `@answerable` — is \*inert\*: the language
parses it and the consumer reads it back. `@declares_kind` and
[@contextual](../references/fact_dec_contextual.md) are the first two where the language changes **its own**
behaviour on a decorator the consumer applied. Writing one on a type does not annotate it; it
changes how every instance of it is looked up and checked.


Kind lookup falls back to a schema **derived** from the declarer: one field per param block,
optional when the param carries a `default`, required otherwise. An instance of the declared
kind is then an ordinary block — an undeclared param is an unknown field, an unfilled required
one a missing required field, both reported by `wcl check` like any other schema violation.


The decorator takes `name` (which `@inline(N)` label of the declarer carries the kind's name,
default `0`), `params` (the declarer field holding its param blocks) and `body` (the field
holding the template body — the language never reads it; expansion belongs to the host's
expander, see [@contextual](../references/fact_dec_contextual.md)).


A derived schema is reachable through kind lookup (`block_schema`) but is deliberately **not**
a declaration: it does not appear among the document's type declarations, because the document
did not declare it. Param values are not type-checked — a param is filled with whatever the
host binds to it.


```wcl
// The host declares its component concept once...
@block("component")
@declares_kind(name = 0, params = "slots", body = "body")
type Component {
  @inline(0) name: identifier
  @children("slot") slots: list<Slot>
  @child("body")    body:  ComponentBody
}
@block("slot") type Slot { @inline(0) name: identifier  default: utf8? }

// ...and every instance of it declares a new kind.
component metric_card {
  slot label
  slot status { default = "ok" }
  body { p $"${label}: ${status}" }
}

// `metric_card` is now an ordinary block kind: `label` is required,
// `status` optional, and a misspelt slot is an unknown field.
page dash {
  metric_card { label = "CPU" }
}
```

## Related

- [@block](../references/fact_dec_block.md)

- [@contextual](../references/fact_dec_contextual.md)

- [@inline](../references/fact_dec_inline.md)

- [Block Schema](../references/concept_block_schema.md)

[← Back to SKILL.md](../SKILL.md)
