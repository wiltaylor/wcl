# @connections

`@connections(S)` accumulates `->` connection statements on a field, turning each into a record of the connection schema `S`. This is how graph-shaped blocks (flowcharts, state machines) collect their edges declaratively.

```wcl
@block("graph") type Graph {
  @connections(Edge) edges: list<Edge>
}

type Edge {
  from: identifier
  to:   identifier
}

graph flow {
  a -> b
  b -> c
}
```

## Related

- [Connections](../references/concept_connections.md)

- [@block](../references/fact_dec_block.md)

[← Back to SKILL.md](../SKILL.md)
