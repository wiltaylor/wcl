# What is it?

_A wskill is a self-contained, shareable unit of knowledge on one topic — one data model projected into up to four views._

A wskill is one folder that captures everything about a specific topic — the ideas, the
concrete things, the values, the tasks — as a single structured data model. It is
self-contained: someone can pick the folder up and learn the topic from it, or follow its
processes to get work done, without anything outside it.


The same data projects into up to [four views](../references/concept_views.md): a reference **book** for
humans, an **AI skill** an agent like Claude Code loads to do the topic's work for you,
and optionally an overview **presentation deck** and a hands-on **training book**.
Capture once, render everywhere — nothing is hand-duplicated between views.


## Examples

### Starting a new wskill

Create a fresh wskill folder with the built-in template, then check and render.

```console
$ wcl init wskill docs/wskills/git      # prompts: topic id, name, summary, optional views
$ cd docs/wskills/git
$ wcl check wskill.wcl                  # must pass
$ just render                           # out/book + out/skill (+ any optional views)
```

**Expected:** A docs/wskills/git/ folder with schema, templates, and data skeleton; wcl check passes; out/ holds every shipped view.

## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [The view family](../references/concept_views.md)

- [Decomposing information](../references/concept_decomposing_information.md)

[← Back to SKILL.md](../SKILL.md)
