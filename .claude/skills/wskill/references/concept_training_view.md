# The training view

_An optional tutorial-series book — ordered lessons with hands-on exercises and expected results, for learners rather than practitioners._

The training view renders a separate book from `lesson` data in `data/training/`: an
ordered course a newcomer walks through to \*learn\* the topic. Lessons order by `n`, group
into `module`s when the course has parts, declare `objectives` ("after this lesson you
can …") and `prerequisites` (other lessons), and end in `exercise`s — each with a task,
optional starter code, a `hint`, and an `expected` result so the learner (or a grading
agent) can verify their work.


Training is not process documentation. A [process](../references/concept_process.md) is a runbook for a
practitioner who already knows the topic and needs the reliable sequence; a lesson
\*teaches\*, building understanding in order. The two link rather than duplicate: a
lesson's `related` names the reference units it teaches, so the course points into the
book instead of restating it.


The view is optional. Ship it by declaring `artifact training { kind = :training … }` in
`wskill.wcl` and authoring lessons; `just training-build` renders it.


## Examples

### A lesson with a verifiable exercise

Every lesson ends in an exercise whose expected result the learner can check.

```wcl
lesson first_render {
  n          = 1
  title      = "Render your first book"
  objectives = ["Build a wskill book and find a unit's page"]
  related    = [views]
  body { p "The book is one `just` recipe away." }
  exercise build_it {
    title     = "Build the book"
    task      = "Render the book projection and open it."
    code_lang = "bash"
    code      = "just book-build && open out/book/index.html"
    expected  = "The browser shows the topic overview with your units in the sidebar."
  }
}
```

## Related

- [The view family](../references/concept_views.md)

- [The presentation view](../references/concept_presentation_view.md)

- [Process](../references/concept_process.md)

- [Creating the training view](../references/process_creating_training_book.md)

[← Back to SKILL.md](../SKILL.md)
