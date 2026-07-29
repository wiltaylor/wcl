# Context blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `boundary` | `name`, `summary?`, `kind?`, `tags[]`, `body?` | a band around systems and external systems — an enterprise, an org unit, a trust zone, a vendor estate (`kind` is BoundaryKind). Members point AT it (`boundary = acme` on the system/external), so a boundary never changes as the estate grows; with none authored the context diagram draws one band named after the WAD |
| `relation` | `source`, `destination`, `kind`, `label?`, `protocol?`, `data?`, `notes?` | a directed edge between two node-space ids; `kind` (RelationKind) supplies the default edge label |

Authoring rules: lowest meaningful level only (roll-up derives every higher diagram); endpoints must resolve to declared ids **written bare** (`source = shop_api`, never `source = "shop_api"` — quoted endpoints break the roll-up on older binaries); one relation per meaningful edge — parallel relations between the same rolled pair merge into one labelled edge. Who populates: hand for business/persona edges; extractors for dependency edges a build system can prove (see the reference WAD's `extract_cargo.py`).

```wcl
relation r_customer_shop {
  source      = customer          // persona id, bare
  destination = shop_api          // container id, bare
  kind        = :sync_api
  label       = "browses and orders via"
  protocol    = "HTTPS"
}
```

## Related

- [Relations wire the diagrams](../references/concept_relations_model.md)

[← Back to SKILL.md](../SKILL.md)
