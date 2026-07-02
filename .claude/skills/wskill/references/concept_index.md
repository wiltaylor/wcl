# Index

_A curated grouping that pins units into the wskill's navigation as a top-level tree — beyond the auto-gathered Reference._

An index is how you add a curated branch to the wskill's navigation. The four unit kinds
(concept, entity, fact, process) are all gathered automatically under Reference, but an
`index` lets you hand-pick some of them and surface them as their own top-level entry,
arranged the way the topic actually reads rather than by kind.


Declare an `index` block with a `name` and a `related` list of unit ids. In the book it
becomes a sidebar heading whose children are those units, pinned into the nav tree (it has
no page of its own). In the skill it renders to its own link-collection page, where the
`body` prose and the `related` links appear together. An index may nest child `index`
blocks one level deep, each pinning its own units — so a single index can group several
related branches under one heading.


## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Concept](../references/concept_concept.md)

[← Back to SKILL.md](../SKILL.md)
