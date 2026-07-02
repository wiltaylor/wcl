# Built-in site templates

A `site`'s `default_template` (or a page's `template`) names one of four built-in projections. The first three render HTML/PDF/Markdown with `wcl wdoc build`; `:ai_skill` is a Markdown-only target rendered with `wcl wdoc skill`.

| Template | What it produces | Render command |
| --- | --- | --- |
| :webpage | A Hugo-style site: header, sticky top navbar from `menu`, reading column | wcl wdoc build |
| :book | An mdBook-style book: fixed left sidebar with nested `toc` chapters, reading column | wcl wdoc build |
| :presentation | A reveal.js-style slide deck laid out by a `deck` block, navigated by keyboard | wcl wdoc build |
| :ai_skill | A Claude skill folder: SKILL.md plus references/, scripts/, assets/ | wcl wdoc skill |

## Related

- [Sites](../references/concept_sites.md)

- [Templates](../references/concept_templates.md)

- [Skill folders](../references/concept_skills.md)

[← Back to SKILL.md](../SKILL.md)
