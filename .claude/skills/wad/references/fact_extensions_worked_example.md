# Custom blocks (extensions.wcl) — a worked example

System-specific typed content is a three-part recipe, all instance-owned: **(1)** declare an `@block` with your fields in `schema/extensions.wcl`, **(2)** declare a merging `@document` in the same file that gathers it (imported `@document` schemas merge with the base, so the new gather list simply appears), and **(3)** render the gather list from a book template. Gather-field names share the merged document space — avoid the base schema's names and wdoc's own (`components`, `pages`, `sites`, `bodies`, …).

The reference WAD (the WCL repo's `.wad/`) exercises this with two extension blocks on the Build & deploy page:

| Block | Fields | Populated by |
| --- | --- | --- |
| `dev_command` | `name`, `command`, `description`, `category?` | extractor — `scripts/extract_justfile.py` reads `just --dump` and owns `data/generated/justfile.wcl`; the page renders the recipe catalogue as a table |
| `release_trigger` | `trailer`, `effect`, `notes?` | hand — three rows in `data/build/release_triggers.wcl` capture the commit-trailer release rules that used to be article prose |

A page template that renders extension blocks legitimately **diverges from the scaffold copy** — note the divergence in the page's header comment (the reference WAD's `wdoc/pages/build.wcl` does). Prefer a typed extension block over article prose whenever the content is a repeating shape (a table, a rule set, a catalogue): typed rows stay queryable, extractable, and honest, while `article` stays the escape hatch for genuine narrative.

## Related

- [WAD folder layout](../references/fact_wad_layout.md)

- [Extractor scripts](../references/fact_extractor_anatomy.md)

[← Back to SKILL.md](../SKILL.md)
