# WAD folder layout

```text
wad.wcl               # root: `wad` metadata block, schema_version, imports
schema/base.wcl       # block schema — scaffold-owned, synced, never hand-edited
schema/kinds.wcl      # symbol_set vocabularies — extend freely
schema/extensions.wcl # system-specific custom blocks (+ merging @document)
data/main.wcl         # imports every view hub
data/<view>/main.wcl  # per-view hub: one import line per data file
data/generated/       # extractor-owned, committed; main.wcl imports each output
scripts/              # extractor scripts (uv single-file Python)
wdoc/book/main.wcl    # site + toc + shared helpers (the projection)
wdoc/pages/*.wcl      # one file per view: landing page + per-entity page repeaters
_site/ _md/           # rendered output (gitignored)
```

Everything lives in the `wcl.wad` namespace (declared at the top of every schema and data
file), so WAD block kinds never collide with wdoc's — which is why the C4 unit is plain
`container`. A WAD is fully interpretable — check + render — with nothing but the `wcl`
binary; the canonical schema ships inside the `wcl init wad` scaffold and is synced to
instances.


Adding data = write a block file under its view + one import line in that view's hub. Every
top-level block carries an `@inline(0) id` that becomes its page route (`A-Za-z0-9_-` only),
and diagram-node kinds (boundary / system / container / component /
external_system / persona / environment / infra_node / screen) share **one id space** so
relation endpoints resolve unambiguously.


Gather-list names for templates: `wads`, `stakeholders`, `raci`, `adrs`, `relations`,
`externals`, `systems`, `containers`, `sw_components` (bare `components` collides with wdoc's
own document schema), `cli_commands`, `code_items`, `environments`, `infra_nodes`,
`deploy_targets`, `repositories`, `pipelines`, `releases`, `personas`, `use_cases`, `sops`,
`standards`, `domain_objects`, `terms`, `specs`, `doc_items`, `screens`, `articles`.


[← Back to SKILL.md](../SKILL.md)
