# What a WAD is

_A living architecture document: one typed WCL data model of a system, rendered into a twelve-chapter book._

A WAD (Wil's Architecture Document) is a folder — `wad.wcl` + `schema/` + `data/` + `wdoc/` — whose **data model is the document**. Every architecture fact is a typed block instance (a `system`, a `relation`, an `adr`); every diagram, table and page in the rendered book derives from those blocks. Nothing is drawn by hand.

The shape is arc42-inspired (a fixed set of views a reviewer can navigate blind) over a C4 drill-down (system → container → component → code). It serves three jobs: **track** an existing system honestly, **design** a new one in enough detail to review, and **drive change** — a reviewed WAD revision becomes the baseline that `wcl diff` turns edits into specs against.

Create one with `wcl init wad`; validate with `wcl check wad.wcl`; the whole model either checks or it doesn't — there is no partially-valid WAD.

## Related

- [The twelve views](../references/concept_twelve_views.md)

- [The C4 drill-down](../references/concept_c4_drilldown.md)

- [WAD folder layout](../references/fact_wad_layout.md)

[← Back to SKILL.md](../SKILL.md)
