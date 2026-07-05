# Context blocks

| Block | Fields | Notes |
| --- | --- | --- |
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
