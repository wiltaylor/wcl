# Infrastructure blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `environment` | `name`, `summary?`, `kind?`, `order?`, `promotes_to?`, `url?`, `body?` | `promotes_to` draws the promotion chain; `order` sorts the landing table |
| `infra_node` | `name`, `kind`, `parent?`, `environments[]`, `technology?`, `address?`, `notes?`, `body?` | flat `parent` refs nest to arbitrary depth (the book renders four levels); more than one environment = shared infra, flagged on the landing page |
| `deploy_target` | `container`, `environment`, `infra_node`, `replicas?`, `url?`, `notes?` | the row that says where each container actually runs — the deployment map |

Hosting, CI, CDN, git, and package-registry platforms all live here (as `infra_node`s), not in externals.

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
