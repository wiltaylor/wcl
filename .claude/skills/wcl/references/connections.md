# Connections

A `connection` declaration defines a typed relationship between block instances; arrow statements then populate it. The result is a list of records that hosts can consume — render edges, build dependency graphs, validate references.

## Declaring a connection

A `connection` names a relationship's source type, destination type, and (optionally) a tag drawn from a `symbol_set`.

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
