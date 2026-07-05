# Plain skill or wskill? — the routing rule

One question decides the tool: **who consumes the knowledge?** If only AI agents ever read
it, build a plain Claude Code skill (`/meta-skill new` — the canonical SKILL.md format).
If humans also need it — as a book, a deck, a training series, or any rendered
documentation — capture it as a **wskill** (`wcl init wskill`), which projects the same
typed data into a human book AND a canonical Claude skill.


![diagram](../_wdoc/fact_skill_vs_wskill-diagram-1.svg)

## The criteria

| Signal | Plain skill | wskill |
| --- | --- | --- |
| Audience | AI agents only | Humans too — book / deck / training views |
| Lifespan | Tool-shaped, replaceable | Long-lived knowledge worth maintaining |
| Shape | A workflow: commands, boundaries, scripts | A topic: typed units (concepts, entities, facts, processes), curated indexes |
| Research | Not stored | `research` blocks ship as `references/research_*.md` other pipelines reuse |
| Consumers | One repo's agents | Multiple projections and repos (skill folders are generated, book is hosted) |

Two clarifications. A wskill's generated skill IS meta-skill-canonical (same frontmatter
rules, same overview/variables/boundaries sections, lints clean), so choosing a wskill never
costs skill quality — it adds the human views on top. And the rule is about \*consumption\*,
not authorship: documentation written by agents but read by people is human-facing, so it
routes to a wskill.


## Related

- [What is it?](../references/concept_wskill_concept.md)

- [The view family](../references/concept_views.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

[← Back to SKILL.md](../SKILL.md)
