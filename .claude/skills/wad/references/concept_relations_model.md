# Relations wire the diagrams

_One `relation` block family drives every generated C4 diagram, via roll-up._

A `relation` is a directed edge between two node ids — any mix of systems, containers, components, externals, and personas, which share one id space. Author each relation once, at the **lowest meaningful level** (container↔container, container↔external, persona↔container).

Diagrams above that level \*derive\*: the system-context diagram rolls both endpoints up to their owning systems, drops self-edges, and merges parallel edges (labels joined); each system's container diagram rolls to container level and keeps edges touching that system. Re-drawing an edge at a higher level is always wrong — it double-counts on roll-up.

`kind` (a closed vocabulary: `:sync_api`, `:async_msg`, `:reads`, `:publishes`, …) supplies the default edge label; `label`, `protocol` and `data` refine it.

## Related

- [The C4 drill-down](../references/concept_c4_drilldown.md)

- [Context blocks](../references/fact_context_blocks.md)

[← Back to SKILL.md](../SKILL.md)
