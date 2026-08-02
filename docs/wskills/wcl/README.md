# {{TOPIC_NAME}} — wskill

A **wskill**: one self-contained folder capturing everything about *{{TOPIC_NAME}}*
— reference, processes, and curated indexes — as a single WCL data model, projected into
a human-readable book and a Claude Code skill.

## Layout

```
wskill.wcl            # entry point: topic, version pin, meta, sources, data imports
schema/kinds.wcl      # topic-owned vocabularies (entity kinds, …) — extend freely
schema/extensions.wcl # custom block types for this topic
data/                 # the content: reference / processes
assets/               # images, PDFs, data files referenced by pages (see assets/README.md)
wdoc/book, wdoc/skill # projection entries (no content — pure structure)
out/                  # generated outputs (gitignored)
```

The base schema and the shared book template are **not** files here: they ship with the
`wcl` binary and arrive through `import <wskill.wcl>` / `import <wskill/book.wcl>`, the
way `import <wdoc.wcl>` already works.

## Build

```bash
just                 # list recipes
just wskill-check    # build every projection and report coverage
just render          # build out/book (site) and out/skill (SKILL.md + references)
just book-serve      # live-preview the book
```

Render and install the skill and any agents into a repo:

```bash
wcl wskill install . --repo <repo>
```

## Editing

Add content by writing block instances into `data/`. The templates project them
automatically — never hand-edit `out/`. Keep `wskill-check` green and re-render
after changes.
