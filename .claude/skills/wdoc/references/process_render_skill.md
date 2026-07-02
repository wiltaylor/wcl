# Render a document into a Claude skill folder

## Purpose

Project an `:ai_skill` site into a SKILL.md plus references/, scripts/, and assets/.

## Prerequisites

- A `site` with `default_template = :ai_skill` and a `skill { }` block
- A page marked `start = true`

## Flowchart

![diagram](../_wdoc/process_render_skill-diagram-1.svg)

## Steps

### Step 1: Run the skill target

```console
$ wcl wdoc skill site.wcl --out out/skill
```

Render the document to a skill folder under `out/skill`. The output directory is created if missing; with several skill sites, each renders into its own `out/skill/<name>/` subfolder.

### Step 2: Inspect the folder layout

```console
$ ls -R out/skill
SKILL.md  references  scripts  assets

out/skill/references:
usage.md
```

The `start` page becomes `SKILL.md` at the root with its front matter from the `skill { }` block; every other page is written under `references/<name>.md`; and each `file` block ships into the subfolder named by its `dir` (`scripts/`, `assets/`).

### Step 3: Check it builds as a skill, not HTML

> [!WARNING]
> **Use the skill target**
> Running `wcl wdoc build` on an `:ai_skill` site fails with a message pointing you at `wcl wdoc skill` — `:ai_skill` is a Markdown-only target.

If `wcl wdoc build` errors on this site, that is expected: skill sites only render through `wcl wdoc skill`.

> [!TIP]
> **Verification**
> `out/skill/SKILL.md` exists with name/description front matter, and the non-start pages appear under `out/skill/references/`.

## Related

- [Skill folders](../references/concept_skills.md)

- [Sites](../references/concept_sites.md)

[← Back to SKILL.md](../SKILL.md)
