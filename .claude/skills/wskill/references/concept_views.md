# The view family

_One data model, four projections: the book and the AI skill are standard; the presentation deck and the training book are optional._

A wskill's data renders into up to four \*views\*, each declared as an `artifact` block in
`wskill.wcl` and built from its own template under `wdoc/`. The data is the single source;
the views never carry topic content of their own.


| View | Audience | What it is | Ships |
| --- | --- | --- | --- |
| **Book** | humans, reference | The comprehensive reference site — every unit gets a page, indexes shape the nav | always |
| **AI skill** | agents | SKILL.md + references/, filtered to `:ai`/`:both` content, with boundaries and parameters | always |
| **Presentation** | humans, first contact | An overview deck introducing the topic — slides feature existing units | optional |
| **Training book** | humans, learning | An ordered tutorial series with hands-on exercises and expected results | optional |

The four serve different moments: the deck for \*meeting\* a topic, the training book for
\*learning\* it, the book for \*using\* it day to day, and the skill for \*an agent doing it
for you\*. Ship the optional two only when the topic warrants them — declaring the
`artifact` and authoring `data/presentation/` or `data/training/` is all it takes.


## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Audience control](../references/concept_audience_control.md)

- [The presentation view](../references/concept_presentation_view.md)

- [The training view](../references/concept_training_view.md)

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

[← Back to SKILL.md](../SKILL.md)
