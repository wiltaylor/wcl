# Creating the presentation view

## Purpose

Ship the optional overview deck: declare the artifact, author the slides, render and present.

## Prerequisites

- The reference content the deck will feature already exists — a deck arranges units, it doesn't replace them.

## Flowchart

![diagram](../_wdoc/process_creating_presentation-diagram-1.svg)

## Steps

### Step 1: Declare the artifact

```wcl
// wskill.wcl — uncomment (or add) the artifact line
artifact slides { kind = :presentation  entry = "wdoc/presentation/main.wcl"  output = "out/presentation" }
// and make sure the data import is active:
import "./data/presentation/main.wcl"
```

A scaffold created with the presentation answer set to `yes` already has the artifact, the data import, and the `wdoc/presentation/` template. Enabling later: uncomment the artifact line and the data import, and copy `wdoc/presentation/main.wcl` + a starter `data/presentation/main.wcl` from a fresh scaffold (`wcl init wskill /tmp/t --defaults -D include_presentation=yes`).

### Step 2: Author the deck data

```wcl
// data/presentation/main.wcl
presentation intro {
  summary = "One-line deck subtitle."
  pres_section what {
    title = "What & why"
    pres_slide opening {
      unit = <concept_id>              // pulls that unit's headline + summary
      speaker_notes = "What to say while this slide is up."
    }
    pres_slide detail {
      title = "Explicit slide title"
      body {
        p "- bullet one"
        p "- bullet two"
      }
    }
  }
}
```

One `presentation` block; each `pres_section` is a deck column, each `pres_slide` one slide (author one slide per line). Prefer `unit = <id>` over restating content — if a slide needs substance the model lacks, capture the unit first. Keep it an introduction: 8–15 slides is plenty.

### Step 3: Render and present

```console
$ just presentation-build     # → out/presentation/index.html
```

Open the built single-file deck: ← → move between sections, ↑ ↓ within one, Space steps through reveals, `s` toggles the speaker notes. The projection prepends a title slide from the topic automatically.

> [!TIP]
> **Verification**
> `out/presentation/index.html` opens as a navigable deck: title slide from the topic, one slide per pres_slide, featured units showing their headline and summary.

## Related

- [The presentation view](../references/concept_presentation_view.md)

- [The view family](../references/concept_views.md)

- [Adding content to a wskill](../references/process_adding_content.md)

[← Back to SKILL.md](../SKILL.md)
