# {{TOPIC_NAME}} — wskill

A **wskill**: one self-contained folder capturing everything about *{{TOPIC_NAME}}*
— reference, processes, and curated indexes — as a single WCL data model, projected into
a human-readable book and a Claude Code skill.

## Layout

```
wskill.wcl            # entry point: topic, version pin, meta, sources, data imports
schema/base.wcl       # base block types (DO NOT hand-edit)
schema/extensions.wcl # custom block types for this topic
data/                 # the content: reference / processes
wdoc/book, wdoc/skill # projection templates (no content — pure structure)
out/                  # generated outputs (gitignored)
```

## Build

```bash
just                 # list recipes
just wskill-check    # validate against the schema
just render          # build out/book (site) and out/skill (SKILL.md + references)
just book-serve      # live-preview the book
```

Install the rendered skill into a repo by copying it:

```bash
cp -r out/skill <repo>/.claude/skills/<name>
```

## Editing

Add content by writing block instances into `data/`. The templates project them
automatically — never hand-edit `out/`. Keep `wskill-check` green and re-render
after changes.
