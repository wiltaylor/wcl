# The wskill folder layout

Every wskill is one self-contained folder with the same canonical tree — `wcl init wskill`
generates it. Three zones: `schema/` is the data model, `data/` is the content you write,
`wdoc/` is the projection templates that render it. Never hand-edit `out/`.


```text
<topic>/
  wskill.wcl              # entry point: topic, version, artifacts, sources, data imports
  schema/
    base.wcl              # base block types + root document (generated — DO NOT hand-edit)
    kinds.wcl             # topic-owned vocabularies (entity kinds, …) — extend freely
    extensions.wcl        # per-topic typed block types (custom schemas)
    presentation.wcl      # optional-view module: the overview deck as data
    training.wcl          # optional-view module: the tutorial series as data
  data/
    reference/   *.wcl    # concept / entity / fact / term / example instances
    processes/   *.wcl    # procedure { … step … } runbooks
    presentation/ *.wcl   # pres_section / pres_slide deck data (when shipping the deck)
    training/    *.wcl    # module / lesson / exercise course data (when shipping training)
  assets/                 # images, PDFs, datasets referenced by bodies
  skill/
    scripts/  assets/     # real files bundled into the emitted AI skill
  wdoc/
    book/main.wcl         # book projection          (wcl wdoc build)
    skill/main.wcl        # AI-skill projection      (wcl wdoc skill)
    presentation/main.wcl # deck projection          (wcl wdoc build) — optional view
    training/main.wcl     # training-book projection (wcl wdoc build) — optional view
    component/  pages/    # shared per-unit components + standalone pages
  out/                    # generated outputs (gitignored) — never hand-edit
```

A larger wskill splits `data/` further (one file per unit under per-kind folders, each with
a `main.wcl` aggregator) — the layout above is the contract, not a limit. The projection
templates contain no topic content; everything a reader sees comes from `data/`.


## Related

- [What is it?](../references/concept_wskill_concept.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Structured data](../references/concept_structured_data.md)

- [Custom projections (schema extension modules)](../references/concept_custom_projection.md)

[← Back to SKILL.md](../SKILL.md)
