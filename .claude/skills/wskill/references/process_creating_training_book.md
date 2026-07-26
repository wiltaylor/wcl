# Creating the training view

## Purpose

Ship the optional interactive course: declare the artifact, design the sections, author material, exercises and checks, render.

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

Sequence the sections before writing any: what can the learner DO after each one (`objectives`), and what must come first (`prerequisites`)? Group into `module`s when the course has parts. Each section should teach a small cluster of reference units — note their ids for `related`, which the course renders as links back into the book.

### Step 3: Author material, exercises, and checks

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
  check why_it_matters {                // multiple choice: graded in the page
    prompt = "Why does this step come first?"
    choice right { label = "The real reason"  correct = true  note = "Why that is right." }
    choice wrong { label = "A plausible trap"  note = "Why that is wrong." }
    explanation = "The point to take away, shown once answered."
  }
  check explain_it {                    // free text: graded against the rubric
    prompt = "Explain the idea in your own words."
    kind   = :text
    rubric = "What a good answer covers — this is the grader's brief."
  }
}
```

The lesson `body` is the material: any wdoc blocks, so prose, `code`, `image` and `video` all work. Follow it with `exercise`s carrying an `expected` result, then `check`s — a multiple-choice check grades in the page, a `:text` check is graded against its `rubric`. Sections order by `n` within their module (or the course).

### Step 4: Render and walk it

```console
$ just training-build          # → out/training/ (works with no server)
$ wcl wdoc serve wdoc/training/main.wcl   # …plus answer capture and grading
$ wcl wdoc training . --pending           # what is waiting on a grader
$ wcl wdoc training . grade <id> "Feedback for the learner."
```

Walk the built course as a learner would: do every exercise, then answer every check. An exercise you can't verify needs a better `expected`; a check whose distractors are obviously wrong teaches nothing; a `:text` rubric that doesn't say what a good answer contains can't be graded consistently. Serve the course to exercise the grading loop end to end.

> [!TIP]
> **Verification**
> out/training/ renders a syllabus rail plus one page per section in order; every exercise's expected result is verifiable from the section alone; multiple-choice checks grade in the page and mark the section complete; and under `wcl wdoc serve` a `:text` answer reaches `training.wcl`, lists via `wcl wdoc training`, and its graded verdict appears in the page without a rebuild.

## Related

- [The training view](../references/concept_training_view.md)

- [The view family](../references/concept_views.md)

- [Adding content to a wskill](../references/process_adding_content.md)

- [Process](../references/concept_process.md)

[← Back to SKILL.md](../SKILL.md)
