# The plan/ template layout

| Path | Purpose |
| --- | --- |
| plan.wcl | Entry point; imports the schema and every data file - one import line per spec and finding file |
| schema/plan_schema.wcl | All block types (question, research, finding, goal, requirement, rule, spec, status, lesson, gate) |
| questions.wcl | Interview questions with :open/:answered/:dropped status and verbatim answers |
| research.wcl | Research items; each points at its finding file |
| research/<id>.wcl | One finding file per research item (parallel-safe) |
| prd.wcl | Goals, non-goals, prioritised requirements, and project rules copied into every brief |
| surfaces.wcl | Every screen/command/endpoint as a typed contract; wireframes in bodies (book-only) |
| scenarios.wcl | End-to-end usage scenarios - the system-level definition of done |
| contracts.wcl | Exact signatures crossing spec boundaries (provider implements, consumers build against) |
| models.wcl | Data models: fields, validation, persistence - one defining spec each |
| asbuilt.wcl | Verifier-recorded deviations, rendered into dependent briefs |
| signoffs.wcl | Explicit :done/:not_applicable per phase; enforced by check-full |
| specs/spec_NNN_name.wcl | One spec per file; spec_000_repo and spec_010_build are mandatory bootstraps |
| status.wcl | Verification agent's ledger - one status row per spec; never shown to implementation agents |
| lessons.wcl | Durable observations from build runs |
| gates.wcl | Gate blocks; the planning model configures these |
| justfile | check / book / specs / render / serve / status recipes, plus wad-init / wad-extract / wad-book / wad-serve |
| scripts/extract_plan.py | Derives wad/data/generated/plan.wcl from the plan; its attribution tables are the WAD steering knobs |
| wad/ | The plan's WAD - the architecture book of the future system (see the plan's-WAD concept); scaffolded by `just wad-init` |
| wdoc/book/main.wcl | Human book projection (PRD, interview, research, DAG diagram, waves, per-spec pages) |
| wdoc/agent/main.wcl | Agent projection - one self-contained .md brief per spec plus index.md |
| out/ | Generated output - never hand-edited |

[← Back to SKILL.md](../SKILL.md)
