# Separation of Data and Presentation

_Keep data and presentation separate: declare the topic once as data in a natural shape, and let the templates own how it looks — so one model can feed several outputs with nothing hand-duplicated._

The wskill model separates data from presentation. You declare the topic once as
structured data in a natural shape; the wdoc templates carry no topic content, only
presentation. Working on the data and on how it looks in two separate places keeps a
document easy to update, and lets one data set feed several outputs in a uniform way.


Presentation runs in two steps over that data. Gather is the query — the root
document collects every block instance of a kind into a list, and the templates walk
those lists. Projection is the render — the gathered data is laid out into an output
format. Because the data is the single source, every presentation of it stays in sync.


That separation is what lets the one data set become multiple outputs: the wdoc book (a
human-readable site), the Claude Code skill (a SKILL.md plus reference files), and the
optional presentation deck and training book. Change a block once and it appears,
hand-duplicated nowhere, in every view on the next render.


## Related

- [What is it?](../references/concept_wskill_concept.md)

- [The view family](../references/concept_views.md)

- [Components: one look for every unit](../references/concept_components_look_feel.md)

[← Back to SKILL.md](../SKILL.md)
