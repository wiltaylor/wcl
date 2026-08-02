# Domain blocks

The domain view captures the **business objects** — the nouns the business uses — and how they
relate: the ubiquitous language, in typed form.


| Block | Fields | Notes |
| --- | --- | --- |
| `domain_object` | `name`, `summary`, `body?` | one business object; fields and relations nest inside |
| `field` | inline name, `type`, `required`, `summary?` | nested — one per field that matters to the business |
| `domain_rel` | `to`, `kind`, `label?`, `via_field?` | nested — `kind` is has_one / has_many / references / extends / uses; `via_field` pins the ER edge to a field row |

The landing page derives an ER diagram from the objects and their relations; each object also
gets its own page.


[← Back to SKILL.md](../SKILL.md)
