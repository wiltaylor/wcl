# The presentation view

_An optional overview deck introducing the topic — slides feature existing units; the detail stays in the book._

The presentation view renders a single-file slide deck from data in `data/presentation/`:
one `presentation` block holding `pres_section`s (deck columns), each holding
`pres_slide`s. It is the topic's \*introduction\* — what you'd flip through before opening
the reference book, or present to a room.


A slide is deliberately thin: it can name a `unit` to feature (the projection pulls that
concept/entity/fact/process's headline and summary onto the slide), carry a short wdoc
`body` (bullets, a diagram), and hold `speaker_notes` for the presenter overlay. Content
discipline follows from that shape — a deck \*arranges\* existing units; it is not a second
home for reference material. If a slide needs substance the model lacks, capture the
substance as a unit first and feature it.


The view is optional. Ship it by declaring `artifact slides { kind = :presentation … }`
in `wskill.wcl` and authoring the deck data; `just presentation-build` renders it. In the
built deck, arrow keys move between sections and slides, Space steps through reveals, and
`s` toggles the speaker notes.


[← Back to SKILL.md](../SKILL.md)
