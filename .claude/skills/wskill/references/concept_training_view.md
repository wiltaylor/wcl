# The training view

_An optional interactive course — ordered sections of material ending in graded checks, for learners rather than practitioners._

The training view renders an interactive course from `lesson` data in `data/training/`:
an ordered series a newcomer walks through to \*learn\* the topic. It is not a book — its
own `training` template renders a syllabus rail that tracks progress, and each section
reads material → exercises → checks. Lessons order by `n`, group into `module`s when the
course has parts, declare `objectives` ("after this section you can …") and
`prerequisites` (other lessons), and carry `exercise`s — each with a task, optional
starter code, a `hint`, and an `expected` result.


A section ends in `check`s. A multiple-choice check (`choice` children, one or more
marked `correct`) grades in the page the moment it is answered, revealing a per-choice
`note` and the check's `explanation`. A `:text` check takes a written answer and is
graded against its `rubric` by an agent: the built site works with no server at all
(progress lives in the browser), but under `wcl wdoc serve` answers persist to a
`training.wcl` sidecar, `wcl wdoc training` lists what is pending, and a verdict written
with `wcl wdoc training grade` reaches the learner's page without a rebuild.


Training is not process documentation. A [process](../references/concept_process.md) is a runbook for a
practitioner who already knows the topic and needs the reliable sequence; a lesson
\*teaches\*, building understanding in order. The two link rather than duplicate: a
lesson's `related` names the reference units it teaches, and the course renders those as
"Covered in the reference" links into the book instead of restating it.


The view is optional. Ship it by declaring `artifact training { kind = :training … }` in
`wskill.wcl` and authoring lessons; `just training-build` renders it.


## Related

- [The view family](../references/concept_views.md)

- [The presentation view](../references/concept_presentation_view.md)

- [Process](../references/concept_process.md)

- [Creating the training view](../references/process_creating_training_book.md)

[← Back to SKILL.md](../SKILL.md)
