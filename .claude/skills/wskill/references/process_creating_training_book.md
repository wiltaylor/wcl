# Creating the training view

## Purpose

Ship the optional tutorial series: declare the artifact, design the course, author lessons with exercises, render.

## Prerequisites

- The reference content exists — lessons teach toward the book, they don't replace it.

## Flowchart

![diagram](../_wdoc/process_creating_training_book-diagram-1.svg)

## Steps

### Step 1: Declare the artifact

```wcl
// wskill.wcl — uncomment (or add) the artifact line
artifact training { kind = :training  entry = "wdoc/training/main.wcl"  output = "out/training" }
// and make sure the data import is active:
import "./data/training/main.wcl"
```

A scaffold created with the training answer set to `yes` already has everything wired. Enabling later: uncomment the artifact and data import, and copy `wdoc/training/main.wcl` + a starter `data/training/main.wcl` from a fresh scaffold (`wcl init wskill /tmp/t --defaults -D include_training=yes`).

### Step 2: Design the course

Sequence the lessons before writing any: what can the learner DO after each one (`objectives`), and what must come first (`prerequisites`)? Group into `module`s when the course has parts. Each lesson should teach a small cluster of reference units — note their ids for `related`.

### Step 3: Author lessons and exercises

```wcl
// data/training/main.wcl
lesson getting_started {
  n          = 1
  title      = "Getting started"
  objectives = ["First capability the learner gains"]
  related    = [<unit_ids>]           // links back into the reference book
  body { p "The lesson material — any wdoc blocks." }
  exercise try_it {
    title    = "Try it"
    task     = "What to do, imperatively."
    code     = "echo hello"
    code_lang = "bash"
    expected = "How the learner knows it worked."
    hint     = "A nudge for when they get stuck."
  }
}
```

Every lesson ends in at least one `exercise` with an `expected` result — hands-on verification is what separates training from prose. Lessons order by `n` within their module (or the course).

### Step 4: Render and walk it

```console
$ just training-build     # → out/training/ (a separate book)
```

Walk the built course as a learner would: do every exercise and check it against its expected result. An exercise you can't verify needs a better `expected`; a lesson that assumes something unstated needs a prerequisite.

> [!TIP]
> **Verification**
> out/training/ renders a syllabus plus one page per lesson in order, and every exercise's expected result is verifiable by following the lesson alone.

## Related

- [The training view](../references/concept_training_view.md)

- [The view family](../references/concept_views.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [Process](../references/concept_process.md)

[← Back to SKILL.md](../SKILL.md)
